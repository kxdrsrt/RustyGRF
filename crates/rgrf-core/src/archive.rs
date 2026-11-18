use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use crate::codec::{self, Decoded};
use crate::container::{self, Entry, Header, TableInfo};
use crate::des;
use crate::error::{Error, Result};
use crate::io::{ReadAt, Source};
use crate::name::{GrfPath, NameMode, glob_match};
use crate::{rgz, thor};

/// Default limit for the decoded size of one entry (the largest real entry measured is 21 MB).
/// Protects against archives whose table claims huge sizes; see [`Archive::set_max_entry_size`].
pub const DEFAULT_MAX_ENTRY_SIZE: u64 = 1 << 30;

/// An opened GRF (0x102, 0x103, 0x200 or 0x300). Reading is positional, so one `Archive` can be shared between threads.
pub struct Archive<R: ReadAt = Source> {
    src: R,
    header: Header,
    table: TableInfo,
    entries: Vec<Entry>,
    file_len: u64,
    max_entry_size: u64,
}

impl Archive<Source> {
    /// Opens a GRF (any version, also `.gpf` patch files, which are GRFs), a Thor patch or an RGZ
    /// patch; the kind is recognised from the content, not from the file name.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let file = File::open(path)?;
        let mut head = [0u8; 24];
        let n = (ReadAt::len(&file)?).min(head.len() as u64) as usize;
        file.read_exact_at(&mut head[..n], 0)?;
        if head[..n].starts_with(thor::MAGIC) {
            let c = thor::read_container(&file)?;
            let src = Source::Shifted {
                file,
                shift: container::HEADER_LEN,
            };
            return Ok(Self::from_parts(src, c));
        }
        if rgz::is_gzip(&head[..n]) {
            let (c, data) = rgz::read_container(&file)?;
            return Ok(Self::from_parts(Source::Memory(data), c));
        }
        Self::from_source(Source::File(file))
    }
}

impl<R: ReadAt> Archive<R> {
    /// Reads a GRF (0x102 to 0x300) from any positional source.
    pub fn from_source(src: R) -> Result<Self> {
        let c = container::read_container(&src)?;
        Ok(Self::from_parts(src, c))
    }

    fn from_parts(src: R, c: container::Container) -> Self {
        Self {
            src,
            header: c.header,
            table: c.table,
            entries: c.entries,
            file_len: c.file_len,
            max_entry_size: DEFAULT_MAX_ENTRY_SIZE,
        }
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

    /// Like [`Archive::read_raw`] but includes the alignment padding (`zsize_aligned` bytes, as far
    /// as they lie inside the file). Needed to copy encrypted entries unchanged.
    pub fn read_raw_aligned(&self, index: usize) -> Result<Vec<u8>> {
        let e = self.entry(index)?;
        let want = u64::from(e.zsize_aligned.max(e.zsize));
        let len = if e.abs_offset() + want <= self.file_len {
            want
        } else {
            u64::from(e.zsize)
        };
        if e.zsize == 0 {
            return Ok(Vec::new());
        }
        if e.abs_offset() + len > self.file_len {
            return Err(Error::EntryOutOfRange {
                index,
                offset: e.abs_offset(),
                size: len,
                file_size: self.file_len,
            });
        }
        let mut buf = vec![0u8; len as usize];
        self.src.read_exact_at(&mut buf, e.abs_offset())?;
        Ok(buf)
    }

    /// Codec of an entry from the first byte of its stream (one tiny read); `None` for markers,
    /// empty and encrypted entries.
    pub fn sniff_codec(&self, index: usize) -> Result<Option<codec::Codec>> {
        let e = self.entry(index)?;
        if !e.is_file() || e.is_encrypted() || e.zsize == 0 {
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
        if let Some(cycle) = e.des_cycle() {
            // the scrambling covers the stored bytes including the alignment padding
            let mut stream = self.read_raw_aligned(index)?;
            des::decrypt_data(&mut stream, cycle);
            stream.truncate(e.zsize as usize);
            let plain = e.flags & container::flags::FILE;
            return Ok(codec::decode(&stream, e.size as usize, plain));
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
