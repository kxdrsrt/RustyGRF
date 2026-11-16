//! GRF container, versions 0x200 and 0x300: 46-byte header and zlib-compressed file table
//! (see docs/02-format-notes.md).
//!
//! Derived from observation of real archives; the 0x300 header rule (64-bit offset only if
//! bytes 35..37 are zero) and the key-block meaning follow GRF Editor's reader (Tokeiburu,
//! used with permission, see docs/05-licensing.md).

use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::GrfPath;
use std::io::Read;

pub const HEADER_LEN: u64 = 46;
/// Upper bound for the decompressed file table, against corrupt headers that ask for huge buffers.
const MAX_TABLE_SIZE: u64 = 1 << 30;
/// Upper bound for the protected-archive key block (GRF Editor uses the same limit).
const MAX_KEY_BLOCK: u32 = 20 + 65535 * 64;

pub mod flags {
    /// Entry is a file (0 = folder/empty marker).
    pub const FILE: u8 = 0x01;
    /// Header blocks encrypted (DES, mixed); with `FILE` this is 0x03, GRF Editor: `FileAndHeaderCrypted`.
    pub const HEADER_CRYPT: u8 = 0x02;
    /// Whole content encrypted (DES); with `FILE` this is 0x05, GRF Editor: `FileAndDataCrypted`.
    pub const DATA_CRYPT: u8 = 0x04;
    /// Encrypted with GRF Editor's own scheme (GRF Editor: `GrfEditorCrypted`).
    pub const GRF_EDITOR_CRYPT: u8 = 0x20;
    /// Gravity's protected-entry scheme of newer archives (needs the key block of a 0x300 archive).
    pub const GRAVITY_CRYPT: u8 = 0x80;
    pub const ANY_CRYPT: u8 = HEADER_CRYPT | DATA_CRYPT | GRF_EDITOR_CRYPT | GRAVITY_CRYPT;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub magic: [u8; 16],
    pub key: [u8; 14],
    /// Offset of the file table relative to the end of the header.
    pub table_offset: u64,
    /// Always 0 in 64-bit (0x300) headers.
    pub seed: u32,
    /// Real number of entries in the table (already corrected for seed and the +7 of 0x200).
    pub entry_count: u32,
    pub version: u32,
}

impl Header {
    pub fn parse(b: &[u8; HEADER_LEN as usize]) -> Result<Self> {
        // "Master of Magic\0" (0x200) or "Event Horizon\0" followed by two more bytes (0x300, measured)
        let known_magic = (b[..15] == *b"Master of Magic" && b[15] == 0)
            || (b[..13] == *b"Event Horizon" && b[13] == 0);
        if !known_magic {
            return Err(Error::NotGrf(
                "magic \"Master of Magic\" or \"Event Horizon\" not found".into(),
            ));
        }
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let version = u32_at(42);
        if version != 0x200 && version != 0x300 {
            return Err(Error::UnsupportedVersion(version));
        }
        let (table_offset, seed, entry_count) = if version == 0x300 && b[35..38] == [0, 0, 0] {
            let off = u64::from_le_bytes(b[30..38].try_into().expect("8 bytes"));
            (off, 0, u32_at(38))
        } else {
            let seed = u32_at(34);
            let count = u32_at(38)
                .checked_sub(seed)
                .and_then(|c| c.checked_sub(7))
                .ok_or_else(|| Error::CorruptTable("entry count smaller than seed + 7".into()))?;
            (u64::from(u32_at(30)), seed, count)
        };
        Ok(Self {
            magic: b[..16].try_into().expect("16 bytes"),
            key: b[16..30].try_into().expect("14 bytes"),
            table_offset,
            seed,
            entry_count,
            version,
        })
    }

    /// Bytes per entry in the table: 17 (32-bit offset) or 21 (64-bit offset).
    pub fn entry_tail_len(&self) -> usize {
        if self.version == 0x300 { 21 } else { 17 }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: GrfPath,
    pub zsize: u32,
    pub zsize_aligned: u32,
    pub size: u32,
    pub flags: u8,
    /// Relative to the end of the header; absolute position is `HEADER_LEN + offset`.
    pub offset: u64,
}

impl Entry {
    pub fn is_file(&self) -> bool {
        self.flags & flags::FILE != 0
    }

    /// Empty marker: flags 0 and no data (e.g. a folder entry). Measured: 1 entry in the 2024
    /// `data.grf`; the other flags-0 entries of real archives carry data, see [`Entry::is_raw`].
    pub fn is_marker(&self) -> bool {
        self.flags & flags::FILE == 0 && self.zsize == 0 && self.size == 0
    }

    /// Flags 0 with data: a file stored **uncompressed** (`zsize == size`, GRF Editor:
    /// `RawDataFile`). Measured: all 30 and 25 flags-0 entries of the 0x300 archives (PNGs).
    pub fn is_raw(&self) -> bool {
        self.flags & (flags::FILE | flags::ANY_CRYPT) == 0 && (self.zsize != 0 || self.size != 0)
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & flags::ANY_CRYPT != 0
    }

    pub fn abs_offset(&self) -> u64 {
        HEADER_LEN.saturating_add(self.offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableInfo {
    /// Absolute offset of the table block (its size prefix starts here).
    pub abs_offset: u64,
    /// Size prefix before the compressed table: 8 bytes (0x200) or 12 bytes (0x300, with key block size).
    pub prefix_len: u32,
    pub zsize: u32,
    pub size: u32,
    /// Size of the protected-archive key block after the table (0x300 only, 0 in all measured archives).
    pub key_block: u32,
}

impl TableInfo {
    /// First byte after the table block (including the key block).
    pub fn end(&self) -> u64 {
        self.abs_offset
            + u64::from(self.prefix_len)
            + u64::from(self.zsize)
            + u64::from(self.key_block)
    }
}

pub fn parse_entries(table: &[u8], count: usize, tail_len: usize) -> Result<Vec<Entry>> {
    let mut entries = Vec::with_capacity(count.min(table.len() / (tail_len + 1)));
    let mut p = 0usize;
    for i in 0..count {
        let rest = &table[p..];
        let nul = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error::CorruptTable(format!("entry {i}: name not terminated")))?;
        let tail_start = nul + 1;
        let tail = rest
            .get(tail_start..tail_start + tail_len)
            .ok_or_else(|| Error::CorruptTable(format!("entry {i}: truncated")))?;
        let u32_at =
            |o: usize| u32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]);
        let offset = if tail_len == 21 {
            u64::from_le_bytes(tail[13..21].try_into().expect("8 bytes"))
        } else {
            u64::from(u32_at(13))
        };
        entries.push(Entry {
            name: GrfPath::new(&rest[..nul]),
            zsize: u32_at(0),
            zsize_aligned: u32_at(4),
            size: u32_at(8),
            flags: tail[12],
            offset,
        });
        p += tail_start + tail_len;
    }
    if p != table.len() {
        return Err(Error::CorruptTable(format!(
            "{} unexpected bytes after {count} entries",
            table.len() - p
        )));
    }
    Ok(entries)
}

pub struct Container {
    pub header: Header,
    pub table: TableInfo,
    pub entries: Vec<Entry>,
    pub file_len: u64,
}

pub fn read_container(src: &dyn ReadAt) -> Result<Container> {
    let file_len = src.len()?;
    if file_len < HEADER_LEN {
        return Err(Error::NotGrf(format!("file has only {file_len} bytes")));
    }
    let mut hb = [0u8; HEADER_LEN as usize];
    src.read_exact_at(&mut hb, 0)?;
    let header = Header::parse(&hb)?;

    let prefix_len: u32 = if header.version == 0x300 { 12 } else { 8 };
    let abs = HEADER_LEN
        .checked_add(header.table_offset)
        .filter(|a| a.saturating_add(u64::from(prefix_len)) <= file_len)
        .ok_or_else(|| Error::CorruptTable("file table lies outside the file".into()))?;
    let mut prefix = [0u8; 12];
    src.read_exact_at(&mut prefix[..prefix_len as usize], abs)?;
    let u32_at =
        |i: usize| u32::from_le_bytes([prefix[i], prefix[i + 1], prefix[i + 2], prefix[i + 3]]);
    let (key_block, zsize, size) = if header.version == 0x300 {
        (u32_at(0), u32_at(4), u32_at(8))
    } else {
        (0, u32_at(0), u32_at(4))
    };
    if key_block > MAX_KEY_BLOCK {
        return Err(Error::CorruptTable(format!(
            "key block of {key_block} bytes"
        )));
    }
    let info = TableInfo {
        abs_offset: abs,
        prefix_len,
        zsize,
        size,
        key_block,
    };
    if info.end() > file_len {
        return Err(Error::CorruptTable(
            "compressed file table exceeds the file".into(),
        ));
    }
    if u64::from(size) > MAX_TABLE_SIZE {
        return Err(Error::CorruptTable(format!(
            "file table claims {size} bytes"
        )));
    }
    let mut packed = vec![0u8; zsize as usize];
    src.read_exact_at(&mut packed, abs + u64::from(prefix_len))?;
    let mut table = Vec::with_capacity(size as usize);
    flate2::read::ZlibDecoder::new(&packed[..])
        .take(u64::from(size) + 1)
        .read_to_end(&mut table)
        .map_err(|e| Error::CorruptTable(format!("file table does not decompress: {e}")))?;
    if table.len() != size as usize {
        return Err(Error::CorruptTable(format!(
            "file table is {} bytes, header says {size}",
            table.len()
        )));
    }
    let entries = parse_entries(&table, header.entry_count as usize, header.entry_tail_len())?;
    Ok(Container {
        header,
        table: info,
        entries,
        file_len,
    })
}
