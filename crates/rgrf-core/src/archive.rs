use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use crate::codec::{self, Decoded};
use crate::container::{self, Entry, Header, TableInfo};
use crate::des;
use crate::error::{Error, Result};
use crate::gravity;
use crate::grfcrypt::{self, Key};
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
    crypt: Option<Crypt>,
    protected: Protected,
    table_encrypted: bool,
}

/// The key block of a 0x300 archive (Gravity's protected entries) and what could be read from it.
#[derive(Default)]
struct Protected {
    block: Vec<u8>,
    keys: gravity::ArchiveKeys,
    /// Why the block could not be read (its entries that need an archive key then fail).
    error: Option<String>,
}

/// State of an archive with GRF Editor's encryption (it has the hidden `__encryption.info` entry).
struct Crypt {
    /// CRC32 of the key, from the hidden entry (`None` when that entry cannot be read).
    marker_crc: Option<u32>,
    key: Option<Box<Key>>,
    /// Entries whose stored stream was encrypted.
    crypted: Vec<bool>,
}

/// What [`Archive::encryption`] tells about an encrypted archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncryptionInfo {
    pub marker_crc: Option<u32>,
    /// Entries whose stream is encrypted.
    pub encrypted_entries: usize,
    /// A key was set, so the entries are readable.
    pub key_set: bool,
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
            // the gzip stream is read from the start; positional reads move the cursor on Windows
            let mut file = file;
            std::io::Seek::seek(&mut file, std::io::SeekFrom::Start(0))?;
            let (c, data) = rgz::read_container(&file)?;
            return Ok(Self::from_parts(Source::Memory(data), c));
        }
        Self::from_source(Source::File(file))
    }

    /// Like [`Archive::open`] with the key of GRF Editor's encryption: opens a file table that is
    /// encrypted too, and makes the encrypted entries readable.
    pub fn open_with_key(path: impl AsRef<Path>, key: &Key) -> Result<Self> {
        let file = File::open(path)?;
        let mut a = Self::from_source_with_key(Source::File(file), Some(key))?;
        a.set_key(*key)?;
        Ok(a)
    }
}

impl<R: ReadAt> Archive<R> {
    /// Reads a GRF (0x102 to 0x300) from any positional source.
    pub fn from_source(src: R) -> Result<Self> {
        Self::from_source_with_key(src, None)
    }

    fn from_source_with_key(src: R, key: Option<&Key>) -> Result<Self> {
        let c = container::read_container_with(&src, key)?;
        // a file table that was encrypted (GRF Editor's "Encrypt File Table"; opened with its key)
        let mut head = [0u8; 3];
        let table_encrypted = c.header.version >= 0x200
            && src
                .read_exact_at(
                    &mut head,
                    c.table.abs_offset + u64::from(c.table.prefix_len),
                )
                .is_ok()
            && grfcrypt::looks_encrypted(&head);
        let mut a = Self::from_parts(src, c);
        a.table_encrypted = table_encrypted;
        Ok(a)
    }

    fn from_parts(src: R, c: container::Container) -> Self {
        let mut a = Self {
            src,
            header: c.header,
            table: c.table,
            entries: c.entries,
            file_len: c.file_len,
            max_entry_size: DEFAULT_MAX_ENTRY_SIZE,
            crypt: None,
            protected: Protected::default(),
            table_encrypted: false,
        };
        a.read_key_block();
        a.detect_encryption();
        a
    }

    /// Reads the key block behind the file table of a 0x300 archive (none in older versions).
    fn read_key_block(&mut self) {
        let t = self.table;
        if t.key_block == 0 {
            return;
        }
        let at = t.abs_offset + u64::from(t.prefix_len) + u64::from(t.zsize);
        let mut block = vec![0u8; t.key_block as usize];
        if let Err(e) = self.src.read_exact_at(&mut block, at) {
            self.protected.error = Some(format!("key block cannot be read: {e}"));
            return;
        }
        match gravity::decode_archive_keys(&block) {
            Ok(keys) => self.protected.keys = keys,
            Err(e) => self.protected.error = Some(e.to_string()),
        }
        self.protected.block = block;
    }

    /// The raw key block behind the file table (empty when there is none).
    pub fn key_block(&self) -> &[u8] {
        &self.protected.block
    }

    /// How many archive keys the key block holds, or why it could not be read.
    pub fn protected_keys(&self) -> std::result::Result<usize, &str> {
        match &self.protected.error {
            Some(e) => Err(e),
            None => Ok(self.protected.keys.len()),
        }
    }

    /// Looks for the hidden entry of GRF Editor's encryption and, if it is there, for the entries
    /// whose streams are encrypted (they get the flag `DETECTED_CRYPT`, "needs a key").
    fn detect_encryption(&mut self) {
        if self.header.is_legacy() {
            return;
        }
        let marker_key = GrfPath::new(grfcrypt::MARKER_NAME.as_bytes()).key();
        let Some(marker) = self.entries.iter().position(|e| e.name.key() == marker_key) else {
            return;
        };
        let marker_crc = self.read(marker).ok().and_then(|d| {
            d.data
                .get(..4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        });
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by_key(|&i| self.entries[i].offset);
        let mut crypted = vec![false; self.entries.len()];
        for i in order {
            let e = &self.entries[i];
            // plain streams of other kinds (stored uncompressed, markers, DES, empty) are not encrypted
            if i == marker
                || e.flags != container::flags::FILE
                || e.zsize < 3
                || e.abs_offset() + 3 > self.file_len
            {
                continue;
            }
            let mut head = [0u8; 3];
            if self.src.read_exact_at(&mut head, e.abs_offset()).is_ok()
                && grfcrypt::looks_encrypted(&head)
            {
                crypted[i] = true;
            }
        }
        for (e, &c) in self.entries.iter_mut().zip(&crypted) {
            if c {
                e.flags |= container::flags::DETECTED_CRYPT;
            }
        }
        self.crypt = Some(Crypt {
            marker_crc,
            key: None,
            crypted,
        });
    }

    /// Whether the file table was found encrypted (the archive was opened with its key).
    pub fn table_encrypted(&self) -> bool {
        self.table_encrypted
    }

    /// Whether the archive uses GRF Editor's encryption (has the hidden entry).
    pub fn encryption(&self) -> Option<EncryptionInfo> {
        self.crypt.as_ref().map(|c| EncryptionInfo {
            marker_crc: c.marker_crc,
            encrypted_entries: c.crypted.iter().filter(|&&x| x).count(),
            key_set: c.key.is_some(),
        })
    }

    /// Whether the stream of an entry was found encrypted with GRF Editor's scheme (also when a
    /// key is set and it reads as plain).
    pub fn entry_encrypted(&self, index: usize) -> bool {
        self.crypt
            .as_ref()
            .is_some_and(|c| c.crypted.get(index).copied().unwrap_or(false))
    }

    /// Sets the key of an encrypted archive; the entries then read as plain ones. A key that does
    /// not match the CRC32 in the hidden entry is refused. On an archive that is not encrypted
    /// this does nothing.
    pub fn set_key(&mut self, key: Key) -> Result<()> {
        let Some(c) = &mut self.crypt else {
            return Ok(());
        };
        if c.marker_crc
            .is_some_and(|crc| crc != grfcrypt::key_crc(&key))
        {
            return Err(Error::WrongKey);
        }
        c.key = Some(Box::new(key));
        for (e, &was) in self.entries.iter_mut().zip(&c.crypted) {
            if was {
                e.flags &= !container::flags::DETECTED_CRYPT;
            }
        }
        Ok(())
    }

    /// The key for an entry whose stream is encrypted and readable (a key is set).
    fn key_for(&self, index: usize) -> Option<&Key> {
        let c = self.crypt.as_ref()?;
        if c.crypted.get(index).copied().unwrap_or(false) {
            c.key.as_deref()
        } else {
            None
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
        if let Some(key) = self.key_for(index) {
            grfcrypt::crypt(key, &mut buf, u64::from(e.size));
        }
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
        if let Some(key) = self.key_for(index) {
            grfcrypt::crypt(key, &mut buf, u64::from(e.size));
        }
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
        if self.key_for(index).is_some() {
            return Ok(codec::sniff(&self.read_raw(index)?));
        }
        let mut b = [0u8; 1];
        self.src.read_exact_at(&mut b, e.abs_offset())?;
        Ok(codec::sniff(&b))
    }

    /// Decoded content with a status; damaged entries yield the recoverable part.
    pub fn read(&self, index: usize) -> Result<Decoded> {
        let e = self.entry(index)?;
        if e.is_folder() {
            return Ok(Decoded {
                data: Vec::new(),
                status: codec::DecodeStatus::Ok,
            });
        }
        if u64::from(e.size).max(u64::from(e.zsize)) > self.max_entry_size {
            return Ok(Decoded {
                data: Vec::new(),
                status: codec::DecodeStatus::Corrupt(format!(
                    "entry of {} bytes ({} stored) exceeds the limit of {} bytes",
                    e.size, e.zsize, self.max_entry_size
                )),
            });
        }
        if e.is_protected() {
            let raw = self.read_raw(index)?;
            return Ok(
                match gravity::decode(
                    &raw,
                    e.name.as_bytes(),
                    e.size as usize,
                    &self.protected.keys,
                ) {
                    Ok(data) => Decoded {
                        data,
                        status: codec::DecodeStatus::Ok,
                    },
                    // a variant that is not ported: reported as encrypted
                    Err(gravity::GravityError::Unsupported(_)) => Decoded {
                        data: Vec::new(),
                        status: codec::DecodeStatus::Encrypted,
                    },
                    Err(gravity::GravityError::Invalid(m)) => Decoded {
                        data: Vec::new(),
                        status: codec::DecodeStatus::Corrupt(match &self.protected.error {
                            Some(k) => format!("protected entry: {m} ({k})"),
                            None => format!("protected entry: {m}"),
                        }),
                    },
                },
            );
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
