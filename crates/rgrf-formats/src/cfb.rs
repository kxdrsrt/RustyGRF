//! A reader for Microsoft's compound files (OLE storage, what `Thumbs.db` is). Only what the
//! thumbnail database needs: the streams by name, whatever folder they are in.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CfbError {
    #[error("this is not a compound file")]
    Header,
    #[error("the compound file is damaged ({0})")]
    Damaged(&'static str),
}

const END: u32 = 0xffff_fffe;
const FREE: u32 = 0xffff_ffff;

struct Entry {
    name: String,
    kind: u8,
    start: u32,
    size: u64,
}

pub struct Cfb<'a> {
    data: &'a [u8],
    sector: usize,
    mini_sector: usize,
    cutoff: u64,
    fat: Vec<u32>,
    mini_fat: Vec<u32>,
    mini_stream: Vec<u8>,
    entries: Vec<Entry>,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

impl<'a> Cfb<'a> {
    pub fn open(data: &'a [u8]) -> Result<Self, CfbError> {
        if data.len() < 512 || data[..8] != [0xd0, 0xcf, 0x11, 0xe0, 0xa1, 0xb1, 0x1a, 0xe1] {
            return Err(CfbError::Header);
        }
        let shift = u16::from_le_bytes([data[30], data[31]]);
        let mini_shift = u16::from_le_bytes([data[32], data[33]]);
        if !(7..=16).contains(&shift) || mini_shift > shift {
            return Err(CfbError::Damaged("sector size"));
        }
        let mut me = Self {
            data,
            sector: 1 << shift,
            mini_sector: 1 << mini_shift,
            cutoff: u64::from(u32_at(data, 56).ok_or(CfbError::Header)?),
            fat: Vec::new(),
            mini_fat: Vec::new(),
            mini_stream: Vec::new(),
            entries: Vec::new(),
        };
        let first_dir = u32_at(data, 48).ok_or(CfbError::Header)?;
        let first_mini = u32_at(data, 60).ok_or(CfbError::Header)?;
        let n_fat = u32_at(data, 44).ok_or(CfbError::Header)? as usize;
        // the sectors that hold the FAT: 109 numbers in the header, more in a chain of DIFAT sectors
        let mut fat_sectors: Vec<u32> = (0..109).filter_map(|i| u32_at(data, 76 + 4 * i)).collect();
        let mut difat = u32_at(data, 68).ok_or(CfbError::Header)?;
        let mut guard = 0;
        while difat != END && difat != FREE && guard < 4096 {
            let s = me.sector_bytes(difat)?;
            let per = me.sector / 4 - 1;
            fat_sectors.extend((0..per).filter_map(|i| u32_at(s, 4 * i)));
            difat = u32_at(s, per * 4).ok_or(CfbError::Damaged("DIFAT"))?;
            guard += 1;
        }
        fat_sectors.retain(|&s| s != FREE);
        fat_sectors.truncate(n_fat.max(1));
        for s in fat_sectors {
            let b = me.sector_bytes(s)?;
            me.fat
                .extend(b.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c)));
        }
        let dir = me.chain_bytes(first_dir)?;
        for raw in dir.as_chunks::<128>().0 {
            let len = usize::from(u16::from_le_bytes([raw[64], raw[65]])).min(64);
            let units: Vec<u16> = raw[..len.saturating_sub(2)]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
                .collect();
            me.entries.push(Entry {
                name: String::from_utf16_lossy(&units),
                kind: raw[66],
                start: u32_at(&raw[..], 116).unwrap_or(END),
                size: u64::from_le_bytes(raw[120..128].try_into().expect("8 bytes")),
            });
        }
        if let Some(root) = me.entries.iter().find(|e| e.kind == 5) {
            let (start, size) = (root.start, root.size as usize);
            if start != END && size > 0 {
                let mut s = me.chain_bytes(start)?;
                s.truncate(size);
                me.mini_stream = s;
            }
        }
        if first_mini != END && first_mini != FREE {
            let b = me.chain_bytes(first_mini)?;
            me.mini_fat = b
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
        }
        Ok(me)
    }

    fn sector_bytes(&self, n: u32) -> Result<&'a [u8], CfbError> {
        let at = (n as usize + 1)
            .checked_mul(self.sector)
            .ok_or(CfbError::Damaged("sector number"))?;
        self.data
            .get(at..at + self.sector)
            .ok_or(CfbError::Damaged("sector outside the file"))
    }

    fn chain_bytes(&self, start: u32) -> Result<Vec<u8>, CfbError> {
        let mut out = Vec::new();
        let mut s = start;
        let mut steps = 0;
        while s != END && s != FREE {
            out.extend_from_slice(self.sector_bytes(s)?);
            s = *self.fat.get(s as usize).ok_or(CfbError::Damaged("FAT"))?;
            steps += 1;
            if steps > self.data.len() / self.sector + 1 {
                return Err(CfbError::Damaged("a loop in the FAT"));
            }
        }
        Ok(out)
    }

    /// The names of all streams.
    pub fn stream_names(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|e| e.kind == 2)
            .map(|e| e.name.as_str())
            .collect()
    }

    pub fn stream(&self, name: &str) -> Result<Option<Vec<u8>>, CfbError> {
        let Some(e) = self.entries.iter().find(|e| e.kind == 2 && e.name == name) else {
            return Ok(None);
        };
        let size = usize::try_from(e.size).map_err(|_| CfbError::Damaged("stream size"))?;
        if e.size < self.cutoff {
            let mut out = Vec::new();
            let mut s = e.start;
            let mut steps = 0;
            while s != END && s != FREE && out.len() < size {
                let at = s as usize * self.mini_sector;
                out.extend_from_slice(
                    self.mini_stream
                        .get(at..at + self.mini_sector)
                        .ok_or(CfbError::Damaged("short sector"))?,
                );
                s = *self
                    .mini_fat
                    .get(s as usize)
                    .ok_or(CfbError::Damaged("mini FAT"))?;
                steps += 1;
                if steps > self.mini_fat.len() + 1 {
                    return Err(CfbError::Damaged("a loop in the mini FAT"));
                }
            }
            out.truncate(size);
            return Ok(Some(out));
        }
        let mut out = self.chain_bytes(e.start)?;
        out.truncate(size);
        Ok(Some(out))
    }
}
