use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use crate::codec::{self, Decoded};
use crate::container::{self, Entry, Header, TableInfo};
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::{GrfPath, NameMode, glob_match};

/// Default limit for the decoded size of one entry (the largest real entry measured is 21 MB).
/// Protects against archives whose table claims huge sizes; see [`Archive::set_max_entry_size`].
pub const DEFAULT_MAX_ENTRY_SIZE: u64 = 1 << 30;

/// An opened GRF 0x200. Reading is positional, so one `Archive` can be shared between threads.
pub struct Archive<R: ReadAt = File> {
    src: R,
    header: Header,
    table: TableInfo,
    entries: Vec<Entry>,
    file_len: u64,
    max_entry_size: u64,
}

impl Archive<File> {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_source(File::open(path)?)
    }
}

impl<R: ReadAt> Archive<R> {
    pub fn from_source(src: R) -> Result<Self> {
        let c = container::read_container(&src)?;
        Ok(Self {
            src,
            header: c.header,
            table: c.table,
            entries: c.entries,
            file_len: c.file_len,
            max_entry_size: DEFAULT_MAX_ENTRY_SIZE,
        })
    }

    /// Entries whose declared or stored size exceeds this are not decoded (status `Corrupt`).
    pub fn set_max_entry_size(&mut self, bytes: u64) {
        self.max_entry_size = bytes;
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn table_info(&self) -> TableInfo {
        self.table
    }

    pub fn file_len(&self) -> u64 {
        self.file_len
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, index: usize) -> Result<&Entry> {
        self.entries.get(index).ok_or(Error::NoSuchEntry(index))
    }

    /// The stored (compressed) bytes of an entry, `zsize` long, without alignment padding.
    pub fn read_raw(&self, index: usize) -> Result<Vec<u8>> {
        let e = self.entry(index)?;
        if e.zsize == 0 {
            return Ok(Vec::new());
        }
        let (offset, size) = (e.abs_offset(), u64::from(e.zsize));
        if offset + size > self.file_len {
            return Err(Error::EntryOutOfRange {
                index,
                offset,
                size,
                file_size: self.file_len,
            });
        }
        let mut buf = vec![0u8; e.zsize as usize];
        self.src.read_exact_at(&mut buf, offset)?;
        Ok(buf)
    }

    /// Codec of an entry from the first byte of its stream (one tiny read); `None` for markers,
    /// empty and encrypted entries.
    pub fn sniff_codec(&self, index: usize) -> Result<Option<codec::Codec>> {
        let e = self.entry(index)?;
        if e.is_marker() || e.is_encrypted() || e.zsize == 0 {
            return Ok(None);
        }
        if e.abs_offset() >= self.file_len {
            return Err(Error::EntryOutOfRange {
                index,
                offset: e.abs_offset(),
                size: u64::from(e.zsize),
                file_size: self.file_len,
            });
        }
        let mut b = [0u8; 1];
        self.src.read_exact_at(&mut b, e.abs_offset())?;
        Ok(codec::sniff(&b))
    }

    /// Decoded content with a status; damaged entries yield the recoverable part.
    pub fn read(&self, index: usize) -> Result<Decoded> {
        let e = self.entry(index)?;
        if u64::from(e.size).max(u64::from(e.zsize)) > self.max_entry_size {
            return Ok(Decoded {
                data: Vec::new(),
                status: codec::DecodeStatus::Corrupt(format!(
                    "entry of {} bytes ({} stored) exceeds the limit of {} bytes",
                    e.size, e.zsize, self.max_entry_size
                )),
            });
        }
        let raw = self.read_raw(index)?;
        Ok(codec::decode(&raw, e.size as usize, e.flags))
    }

    /// Case-insensitive lookup table (see [`GrfPath::key`]); indices in archive order.
    pub fn index_by_key(&self) -> HashMap<Vec<u8>, Vec<usize>> {
        let mut m: HashMap<Vec<u8>, Vec<usize>> = HashMap::with_capacity(self.entries.len());
        for (i, e) in self.entries.iter().enumerate() {
            m.entry(e.name.key()).or_default().push(i);
        }
        m
    }

    /// Entries matching `pattern` (all if `None`). With `*` or `?` it is a case-insensitive glob over
    /// the whole path, without them a case-insensitive substring match. `/` equals `\`.
    pub fn select(&self, pattern: Option<&str>, mode: NameMode) -> Vec<usize> {
        let Some(pattern) = pattern.filter(|p| !p.is_empty()) else {
            return (0..self.entries.len()).collect();
        };
        let fold = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if c == '/' {
                        '\\'
                    } else {
                        c.to_ascii_lowercase()
                    }
                })
                .collect()
        };
        let wildcard = pattern.contains(['*', '?']);
        let needle = fold(pattern);
        (0..self.entries.len())
            .filter(|&i| {
                let shown = self.entries[i].name.display(mode);
                if wildcard {
                    glob_match(pattern, &shown)
                } else {
                    fold(&shown).contains(&needle)
                }
            })
            .collect()
    }

    /// Last entry whose path equals `path` (case-insensitive, `/` = `\`). Linear scan.
    pub fn find(&self, path: &GrfPath) -> Option<usize> {
        let key = path.key();
        self.entries.iter().rposition(|e| e.name.key() == key)
    }
}
