//! GRF 0x200 container: 46-byte header and zlib-compressed file table (see docs/02-format-notes.md).

use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::GrfPath;
use std::io::Read;

pub const HEADER_LEN: u64 = 46;
const MAGIC: &[u8; 15] = b"Master of Magic";
const ENTRY_TAIL_LEN: usize = 17;
/// Upper bound for the decompressed file table, against corrupt headers that ask for huge buffers.
const MAX_TABLE_SIZE: u64 = 1 << 30;

pub mod flags {
    /// Entry is a file (0 = folder/empty marker).
    pub const FILE: u8 = 0x01;
    /// Whole content encrypted (mixed DES).
    pub const MIXED_CRYPT: u8 = 0x02;
    /// Only the header blocks of the content are encrypted (GRF Editor: `FileAndHeaderCrypted` is 0x03).
    pub const HEADER_CRYPT: u8 = 0x04;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub key: [u8; 14],
    /// Offset of the file table relative to the end of the header.
    pub table_offset: u32,
    pub seed: u32,
    /// Value stored in the file: real entry count is `files_count_raw - seed - 7`.
    pub files_count_raw: u32,
    pub version: u32,
}

impl Header {
    pub fn parse(b: &[u8; HEADER_LEN as usize]) -> Result<Self> {
        if &b[..15] != MAGIC || b[15] != 0 {
            return Err(Error::NotGrf("magic \"Master of Magic\" not found".into()));
        }
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let header = Self {
            key: b[16..30].try_into().expect("14 bytes"),
            table_offset: u32_at(30),
            seed: u32_at(34),
            files_count_raw: u32_at(38),
            version: u32_at(42),
        };
        if header.version != 0x200 {
            return Err(Error::UnsupportedVersion(header.version));
        }
        Ok(header)
    }

    /// Number of entries in the table, `None` if the stored values are inconsistent.
    pub fn entry_count(&self) -> Option<u32> {
        self.files_count_raw.checked_sub(self.seed)?.checked_sub(7)
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
    pub offset: u32,
}

impl Entry {
    pub fn is_file(&self) -> bool {
        self.flags & flags::FILE != 0
    }

    /// Folder/empty marker (flags 0, size 0).
    pub fn is_marker(&self) -> bool {
        !self.is_file()
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & (flags::MIXED_CRYPT | flags::HEADER_CRYPT) != 0
    }

    pub fn abs_offset(&self) -> u64 {
        HEADER_LEN + u64::from(self.offset)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableInfo {
    /// Absolute offset of the compressed table (the 8-byte size prefix starts here).
    pub abs_offset: u64,
    pub zsize: u32,
    pub size: u32,
}

pub fn parse_entries(table: &[u8], count: usize) -> Result<Vec<Entry>> {
    let mut entries = Vec::with_capacity(count.min(table.len() / (ENTRY_TAIL_LEN + 1)));
    let mut p = 0usize;
    for i in 0..count {
        let rest = &table[p..];
        let nul = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error::CorruptTable(format!("entry {i}: name not terminated")))?;
        let tail_start = nul + 1;
        let tail = rest
            .get(tail_start..tail_start + ENTRY_TAIL_LEN)
            .ok_or_else(|| Error::CorruptTable(format!("entry {i}: truncated")))?;
        let u32_at =
            |o: usize| u32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]);
        entries.push(Entry {
            name: GrfPath::new(&rest[..nul]),
            zsize: u32_at(0),
            zsize_aligned: u32_at(4),
            size: u32_at(8),
            flags: tail[12],
            offset: u32_at(13),
        });
        p += tail_start + ENTRY_TAIL_LEN;
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
    let count = header
        .entry_count()
        .ok_or_else(|| Error::CorruptTable("entry count smaller than seed + 7".into()))?;

    let abs = HEADER_LEN + u64::from(header.table_offset);
    if abs + 8 > file_len {
        return Err(Error::CorruptTable(
            "file table lies outside the file".into(),
        ));
    }
    let mut sizes = [0u8; 8];
    src.read_exact_at(&mut sizes, abs)?;
    let zsize = u32::from_le_bytes([sizes[0], sizes[1], sizes[2], sizes[3]]);
    let size = u32::from_le_bytes([sizes[4], sizes[5], sizes[6], sizes[7]]);
    if abs + 8 + u64::from(zsize) > file_len {
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
    src.read_exact_at(&mut packed, abs + 8)?;
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
    let entries = parse_entries(&table, count as usize)?;
    Ok(Container {
        header,
        table: TableInfo {
            abs_offset: abs,
            zsize,
            size,
        },
        entries,
        file_len,
    })
}
