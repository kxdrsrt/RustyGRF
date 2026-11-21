//! Writing GRF 0x200 / 0x300 archives: header, data area, zlib-compressed file table.
//!
//! Layouts follow docs/02-format-notes.md (measured on real archives). Entries are laid out in
//! the order they are put; the file table lists them in the order the caller passes.

use std::io::{Seek, SeekFrom, Write};

use flate2::{Compression, write::ZlibEncoder};

use crate::container::HEADER_LEN;
use crate::error::{Error, Result};
use crate::name::GrfPath;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Old format of the first clients (and `event.grf`): scrambled names, plain file table, DES
    /// scrambled file data. 0x102 and 0x103 differ only in the version number.
    V102,
    V103,
    /// 32-bit offsets, limit 4 GiB; the format `alpha.exe` and other older clients read.
    V200,
    /// 64-bit offsets ("Event Horizon"); read by current kRO clients and GRF Editor.
    V300,
}

/// A magic for the header from text: up to 16 printable ASCII characters, padded with NUL bytes
/// ("Master of Magic" becomes the usual `Master of Magic\0`).
pub fn magic_from_text(text: &str) -> Result<[u8; 16]> {
    if text.is_empty() || text.len() > 16 || !text.bytes().all(|b| (0x20..0x7f).contains(&b)) {
        return Err(Error::InvalidInput(format!(
            "the magic must be 1 to 16 printable ASCII characters, got {text:?}"
        )));
    }
    let mut m = [0u8; 16];
    m[..text.len()].copy_from_slice(text.as_bytes());
    Ok(m)
}

/// The magic as text (up to the first NUL byte; other bytes shown as `\xNN`).
pub fn magic_to_text(m: &[u8; 16]) -> String {
    m.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| {
            if (0x20..0x7f).contains(&b) {
                (b as char).to_string()
            } else {
                format!("\\x{b:02x}")
            }
        })
        .collect()
}

impl Format {
    pub fn version(self) -> u32 {
        match self {
            Self::V102 => 0x102,
            Self::V103 => 0x103,
            Self::V200 => 0x200,
            Self::V300 => 0x300,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().trim_start_matches("0x") {
            "102" => Some(Self::V102),
            "103" => Some(Self::V103),
            "200" | "2" => Some(Self::V200),
            "300" | "3" => Some(Self::V300),
            _ => None,
        }
    }

    /// Whether the stored compressed size is rounded up to a multiple of 8 by default. Measured:
    /// official 0x200 archives round up (and pad), the current kRO 0x300 archive does not.
    pub fn aligns_by_default(self) -> bool {
        self != Self::V300
    }

    /// 0x102/0x103: file data is DES scrambled on writing, the table is plain.
    pub fn is_legacy(self) -> bool {
        matches!(self, Self::V102 | Self::V103)
    }

    /// Largest file size the format can address (the client uses 32-bit positions for 0x200).
    fn max_file_len(self) -> u64 {
        match self {
            Self::V102 | Self::V103 | Self::V200 => u64::from(u32::MAX),
            Self::V300 => u64::MAX,
        }
    }
}

/// Where a stream landed in the data area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placed {
    /// Relative to the end of the header.
    pub offset: u64,
    pub zsize: u32,
    pub aligned: u32,
}

#[derive(Debug, Clone)]
pub struct OutEntry {
    pub name: GrfPath,
    pub placed: Placed,
    pub size: u32,
    pub flags: u8,
}

pub struct Writer<W: Write + Seek> {
    out: W,
    format: Format,
    align: bool,
    max_file_len: u64,
    /// Bytes written after the header.
    data_len: u64,
    /// 0x300: the key block written behind the file table (Gravity's protected entries).
    key_block: Vec<u8>,
    /// The first 16 bytes of the header instead of the format's ("Master of Magic\0",
    /// "Event Horizon\0c\0"); GRF Editor lets this be edited and reads any.
    magic: Option<[u8; 16]>,
    /// Encrypts the compressed file table with this key (GRF Editor's "Encrypt File Table").
    table_key: Option<crate::grfcrypt::Key>,
}

fn ceil8(n: u32) -> u32 {
    n.div_ceil(8).saturating_mul(8)
}

impl<W: Write + Seek> Writer<W> {
    /// `max_file_len` overrides the format's limit (tests); `None` uses the format default.
    pub fn new(mut out: W, format: Format, align: bool, max_file_len: Option<u64>) -> Result<Self> {
        out.write_all(&[0u8; HEADER_LEN as usize])?;
        Ok(Self {
            out,
            format,
            // the DES blocks of the legacy formats need the alignment
            align: align || format.is_legacy(),
            max_file_len: max_file_len.unwrap_or_else(|| format.max_file_len()),
            data_len: 0,
            key_block: Vec::new(),
            magic: None,
            table_key: None,
        })
    }

    /// Encrypts the file table with `key` (0x200 and 0x300 only); the entries stay as they are.
    pub fn set_table_key(&mut self, key: crate::grfcrypt::Key) -> Result<()> {
        if self.format.is_legacy() {
            return Err(Error::Unsupported(
                "the file table of the old 0x102/0x103 formats cannot be encrypted".into(),
            ));
        }
        self.table_key = Some(key);
        Ok(())
    }

    /// Writes `magic` as the first 16 bytes of the header instead of the format's own.
    pub fn set_magic(&mut self, magic: [u8; 16]) {
        self.magic = Some(magic);
    }

    /// The key block to write behind the file table; only the 0x300 format has one.
    pub fn set_key_block(&mut self, block: Vec<u8>) -> Result<()> {
        if !block.is_empty() && self.format != Format::V300 {
            return Err(Error::Unsupported(
                "only the 0x300 format holds a key block".into(),
            ));
        }
        self.key_block = block;
        Ok(())
    }

    pub fn is_legacy(&self) -> bool {
        self.format.is_legacy()
    }

    pub fn data_len(&self) -> u64 {
        self.data_len
    }

    fn reserve(&mut self, n: u64) -> Result<u64> {
        let offset = self.data_len;
        let end = HEADER_LEN + offset + n;
        if end > self.max_file_len {
            return Err(Error::TooLarge(format!(
                "the data would reach {end} bytes, over the limit of {} bytes for this format \
                 (use a smaller method such as LZMA, deduplicate, or the 0x300 format)",
                self.max_file_len
            )));
        }
        Ok(offset)
    }

    /// Appends a compressed stream (padded with zeros to a multiple of 8 if aligning).
    pub fn put_stream(&mut self, stream: &[u8]) -> Result<Placed> {
        let zsize = u32::try_from(stream.len())
            .map_err(|_| Error::TooLarge("a single entry stream exceeds 4 GiB".into()))?;
        let aligned = if self.align { ceil8(zsize) } else { zsize };
        let offset = self.reserve(u64::from(aligned))?;
        self.out.write_all(stream)?;
        for _ in zsize..aligned {
            self.out.write_all(&[0])?;
        }
        self.data_len += u64::from(aligned);
        Ok(Placed {
            offset,
            zsize,
            aligned,
        })
    }

    /// Appends bytes unchanged (encrypted or damaged entries) and records the given sizes.
    pub fn put_raw(&mut self, bytes: &[u8], zsize: u32, aligned: u32) -> Result<Placed> {
        let offset = self.reserve(bytes.len() as u64)?;
        self.out.write_all(bytes)?;
        self.data_len += bytes.len() as u64;
        Ok(Placed {
            offset,
            zsize,
            aligned,
        })
    }

    /// Writes the file table and the header; returns the final file length and the output.
    pub fn finish(mut self, entries: &[OutEntry]) -> Result<(u64, W)> {
        if self.format.is_legacy() {
            return self.finish_legacy(entries);
        }
        let tail = if self.format == Format::V300 { 21 } else { 17 };
        let mut table = Vec::with_capacity(entries.len() * (tail + 48));
        for e in entries {
            table.extend_from_slice(e.name.as_bytes());
            table.push(0);
            table.extend_from_slice(&e.placed.zsize.to_le_bytes());
            table.extend_from_slice(&e.placed.aligned.to_le_bytes());
            table.extend_from_slice(&e.size.to_le_bytes());
            table.push(e.flags);
            match self.format {
                Format::V300 => table.extend_from_slice(&e.placed.offset.to_le_bytes()),
                _ => table.extend_from_slice(
                    &u32::try_from(e.placed.offset)
                        .map_err(|_| Error::TooLarge("entry offset does not fit 32 bits".into()))?
                        .to_le_bytes(),
                ),
            }
        }
        let mut z = ZlibEncoder::new(
            Vec::with_capacity(table.len() / 4 + 64),
            Compression::new(9),
        );
        z.write_all(&table)?;
        let mut packed = z.finish()?;
        if let Some(key) = &self.table_key {
            // the counter is the table's size + 8; a result that could pass for zlib stays plain
            crate::grfcrypt::encrypt_stream(key, &mut packed, table.len() as u64 + 8);
        }
        let (zsize, size) = (
            u32::try_from(packed.len())
                .map_err(|_| Error::TooLarge("file table too large".into()))?,
            u32::try_from(table.len())
                .map_err(|_| Error::TooLarge("file table too large".into()))?,
        );

        let prefix_len = if self.format == Format::V300 { 12 } else { 8 };
        let total = HEADER_LEN
            + self.data_len
            + prefix_len
            + u64::from(zsize)
            + self.key_block.len() as u64;
        if total > self.max_file_len {
            return Err(Error::TooLarge(format!(
                "the archive would be {total} bytes, over the limit of {} bytes for this format",
                self.max_file_len
            )));
        }
        if self.format == Format::V300 {
            self.out
                .write_all(&(self.key_block.len() as u32).to_le_bytes())?;
        }
        self.out.write_all(&zsize.to_le_bytes())?;
        self.out.write_all(&size.to_le_bytes())?;
        self.out.write_all(&packed)?;
        self.out.write_all(&self.key_block)?;

        let count =
            u32::try_from(entries.len()).map_err(|_| Error::TooLarge("too many entries".into()))?;
        let mut h = Vec::with_capacity(HEADER_LEN as usize);
        match (self.magic, self.format) {
            (Some(m), _) => h.extend_from_slice(&m),
            (None, Format::V300) => h.extend_from_slice(b"Event Horizon\0c\0"),
            (None, _) => h.extend_from_slice(b"Master of Magic\0"),
        }
        h.extend(1..=14u8);
        match self.format {
            Format::V300 => {
                h.extend_from_slice(&self.data_len.to_le_bytes());
                h.extend_from_slice(&count.to_le_bytes());
            }
            _ => {
                h.extend_from_slice(&(self.data_len as u32).to_le_bytes());
                h.extend_from_slice(&0u32.to_le_bytes());
                h.extend_from_slice(&(count + 7).to_le_bytes());
            }
        }
        h.extend_from_slice(&self.format.version().to_le_bytes());
        debug_assert_eq!(h.len() as u64, HEADER_LEN);
        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&h)?;
        self.out.flush()?;
        Ok((total, self.out))
    }
}

impl<W: Write + Seek> Writer<W> {
    /// 0x102/0x103: `[u32 len][u16 0][name, DES][u32 0][zsize+size+715][aligned+37579][size][type]
    /// [u32 offset]` per entry, no compression and no size prefix. Empty markers are left out (the
    /// format has no way to tell them from folders).
    fn finish_legacy(mut self, entries: &[OutEntry]) -> Result<(u64, W)> {
        let mut table = Vec::with_capacity(entries.len() * 64);
        let mut count = 0u32;
        for e in entries {
            let marker =
                e.flags & crate::container::flags::FILE == 0 && e.placed.zsize == 0 && e.size == 0;
            if marker || e.flags & crate::container::flags::REMOVE != 0 {
                continue;
            }
            count += 1;
            let name = crate::des::encode_name(e.name.as_bytes());
            let too_large = |what: &str| Error::TooLarge(format!("{what} does not fit 32 bits"));
            let name_len = u32::try_from(name.len() + 6).map_err(|_| too_large("name"))?;
            let packed = e
                .placed
                .zsize
                .checked_add(e.size)
                .and_then(|v| v.checked_add(715))
                .ok_or_else(|| too_large("the sum of the sizes"))?;
            let aligned = e
                .placed
                .aligned
                .checked_add(37579)
                .ok_or_else(|| too_large("the aligned size"))?;
            let offset =
                u32::try_from(e.placed.offset).map_err(|_| too_large("an entry offset"))?;
            table.extend_from_slice(&name_len.to_le_bytes());
            table.extend_from_slice(&0u16.to_le_bytes());
            table.extend_from_slice(&name);
            table.extend_from_slice(&0u32.to_le_bytes());
            table.extend_from_slice(&packed.to_le_bytes());
            table.extend_from_slice(&aligned.to_le_bytes());
            table.extend_from_slice(&e.size.to_le_bytes());
            table.push(u8::from(e.flags & crate::container::flags::FILE != 0));
            table.extend_from_slice(&offset.to_le_bytes());
        }
        let total = HEADER_LEN + self.data_len + table.len() as u64;
        if total > self.max_file_len {
            return Err(Error::TooLarge(format!(
                "the archive would be {total} bytes, over the limit of {} bytes for this format",
                self.max_file_len
            )));
        }
        self.out.write_all(&table)?;
        let mut h = Vec::with_capacity(HEADER_LEN as usize);
        h.extend_from_slice(&self.magic.unwrap_or(*b"Master of Magic\0"));
        h.extend(1..=14u8);
        h.extend_from_slice(&(self.data_len as u32).to_le_bytes());
        h.extend_from_slice(&0u32.to_le_bytes());
        h.extend_from_slice(&(count + 7).to_le_bytes());
        h.extend_from_slice(&self.format.version().to_le_bytes());
        debug_assert_eq!(h.len() as u64, HEADER_LEN);
        self.out.seek(SeekFrom::Start(0))?;
        self.out.write_all(&h)?;
        self.out.flush()?;
        Ok((total, self.out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::Archive;
    use crate::codec::DecodeStatus;
    use crate::codec::tests_support::zlib;
    use crate::container::flags;
    use std::io::Cursor;

    fn entry(name: &str, placed: Placed, size: u32) -> OutEntry {
        OutEntry {
            name: GrfPath::new(name.as_bytes()),
            placed,
            size,
            flags: flags::FILE,
        }
    }

    const A: &[u8] = b"hello hello hello";

    fn build(format: Format, align: bool) -> Vec<u8> {
        let mut w = Writer::new(Cursor::new(Vec::new()), format, align, None).unwrap();
        let b = vec![5u8; 1000];
        let pa = w.put_stream(&zlib(A)).unwrap();
        let pb = w.put_stream(&zlib(&b)).unwrap();
        let marker = Placed {
            offset: 0,
            zsize: 0,
            aligned: 0,
        };
        let entries = [
            entry("data\\a.txt", pa, A.len() as u32),
            OutEntry {
                name: GrfPath::new(&b"data\\dir.txt"[..]),
                placed: marker,
                size: 0,
                flags: 0,
            },
            entry("data\\b.bin", pb, 1000),
            entry("data\\b2.bin", pb, 1000), // shares b's data block
        ];
        let (len, out) = w.finish(&entries).unwrap();
        let bytes = out.into_inner();
        assert_eq!(len, bytes.len() as u64);
        bytes
    }

    #[test]
    fn written_archives_are_read_back_in_both_formats() {
        for format in [Format::V200, Format::V300] {
            for align in [false, true] {
                let g = Archive::from_source(build(format, align)).unwrap();
                assert_eq!(g.header().version, format.version());
                assert_eq!(g.len(), 4);
                assert_eq!(g.read(0).unwrap().data, A);
                assert!(g.entry(1).unwrap().is_marker());
                assert_eq!(g.read(2).unwrap().data, vec![5u8; 1000]);
                assert_eq!(g.read(3).unwrap().data, vec![5u8; 1000]);
                assert_eq!(
                    g.entry(2).unwrap().offset,
                    g.entry(3).unwrap().offset,
                    "shared block"
                );
                for e in g.entries().iter().filter(|e| e.is_file()) {
                    let rounded = e.zsize.div_ceil(8) * 8;
                    assert_eq!(
                        e.zsize_aligned,
                        if align { rounded } else { e.zsize },
                        "{format:?} {align}"
                    );
                }
                let rep = crate::verify::verify(
                    &g,
                    crate::verify::VerifyOptions::default(),
                    &crate::progress::Control::default(),
                )
                .unwrap();
                assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
            }
        }
    }

    #[test]
    fn a_custom_magic_is_written_and_read_like_grf_editor_does() {
        for format in [Format::V200, Format::V300] {
            let mut bytes = build(format, true);
            let magic = magic_from_text("My Own Magic").unwrap();
            bytes[..16].copy_from_slice(&magic);
            let g = Archive::from_source(bytes).unwrap();
            assert_eq!(magic_to_text(&g.header().magic), "My Own Magic");
            assert_eq!(g.read(0).unwrap().data, A);
        }
        let mut w = Writer::new(Cursor::new(Vec::new()), Format::V200, true, None).unwrap();
        w.set_magic(magic_from_text("0123456789abcdef").unwrap());
        let (_, out) = w.finish(&[]).unwrap();
        assert_eq!(&out.into_inner()[..16], b"0123456789abcdef");
        // without a GRF version the file is still no GRF
        let mut junk = build(Format::V200, true);
        junk[..16].copy_from_slice(b"not an archive!!");
        junk[42..46].copy_from_slice(&7u32.to_le_bytes());
        assert!(Archive::from_source(junk).is_err());
        assert!(magic_from_text("").is_err());
        assert!(magic_from_text("seventeen chars!!").is_err());
        assert!(magic_from_text("tab\there").is_err());
    }

    #[test]
    fn headers_match_the_measured_layouts() {
        let v200 = build(Format::V200, true);
        assert_eq!(&v200[..16], b"Master of Magic\0");
        assert_eq!(&v200[16..30], &(1..=14u8).collect::<Vec<_>>()[..]);
        assert_eq!(
            u32::from_le_bytes(v200[34..38].try_into().unwrap()),
            0,
            "seed"
        );
        assert_eq!(
            u32::from_le_bytes(v200[38..42].try_into().unwrap()),
            4 + 7,
            "count + 7"
        );
        assert_eq!(u32::from_le_bytes(v200[42..46].try_into().unwrap()), 0x200);
        let v300 = build(Format::V300, false);
        assert_eq!(&v300[..16], b"Event Horizon\0c\0");
        assert_eq!(
            u32::from_le_bytes(v300[38..42].try_into().unwrap()),
            4,
            "plain count"
        );
        assert_eq!(u32::from_le_bytes(v300[42..46].try_into().unwrap()), 0x300);
        let table_at = 46 + u64::from_le_bytes(v300[30..38].try_into().unwrap()) as usize;
        assert_eq!(&v300[table_at..table_at + 4], &[0, 0, 0, 0], "no key block");
    }

    #[test]
    fn raw_entries_keep_their_sizes_and_bytes() {
        let mut w = Writer::new(Cursor::new(Vec::new()), Format::V200, true, None).unwrap();
        let p = w.put_raw(&[9u8; 24], 20, 24).unwrap();
        let e = OutEntry {
            name: GrfPath::new(&b"data\\enc.lub"[..]),
            placed: p,
            size: 33,
            flags: flags::FILE | flags::GRF_EDITOR_CRYPT,
        };
        let (_, out) = w.finish(&[e]).unwrap();
        let g = Archive::from_source(out.into_inner()).unwrap();
        let e = g.entry(0).unwrap();
        assert_eq!(
            (e.zsize, e.zsize_aligned, e.size, e.flags),
            (20, 24, 33, 0x21)
        );
        assert_eq!(g.read(0).unwrap().status, DecodeStatus::Encrypted);
        assert_eq!(g.read_raw(0).unwrap(), vec![9u8; 20]);
    }

    #[test]
    fn the_format_limit_is_enforced_while_writing_and_at_the_end() {
        let big = vec![1u8; 100];
        let mut w =
            Writer::new(Cursor::new(Vec::new()), Format::V200, false, Some(46 + 150)).unwrap();
        w.put_stream(&big).unwrap();
        assert!(matches!(w.put_stream(&big), Err(Error::TooLarge(_))));

        // the table itself must fit as well
        let mut w = Writer::new(
            Cursor::new(Vec::new()),
            Format::V200,
            false,
            Some(46 + 100 + 8 + 20),
        )
        .unwrap();
        let p = w.put_stream(&big).unwrap();
        let entries: Vec<OutEntry> = (0..50)
            .map(|i| entry(&format!("data\\file{i}.bin"), p, 100))
            .collect();
        assert!(matches!(w.finish(&entries), Err(Error::TooLarge(_))));
    }

    #[test]
    fn format_parsing() {
        assert_eq!(Format::parse("0x200"), Some(Format::V200));
        assert_eq!(Format::parse("300"), Some(Format::V300));
        assert_eq!(Format::parse("0x103"), Some(Format::V103));
        assert_eq!(Format::parse("0x102"), Some(Format::V102));
        assert_eq!(Format::parse("0x104"), None);
        assert!(Format::V200.aligns_by_default() && !Format::V300.aligns_by_default());
    }
}
