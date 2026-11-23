//! GRF container, versions 0x102, 0x103, 0x200 and 0x300: 46-byte header and file table (zlib
//! compressed from 0x200 on, plain with DES-scrambled names before; see docs/02-format-notes.md).
//!
//! Derived from observation of real archives; the 0x300 header rule (64-bit offset only if
//! bytes 35..37 are zero) and the key-block meaning follow GRF Editor's reader (Tokeiburu,
//! used with permission, see docs/05-licensing.md).

use crate::des;
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::GrfPath;
use std::io::Read;

pub const HEADER_LEN: u64 = 46;
/// Upper bound for the decompressed file table, against corrupt headers that ask for huge buffers.
const MAX_TABLE_SIZE: u64 = 1 << 30;
/// Upper bound for the protected-archive key block (GRF Editor uses the same limit).
const MAX_KEY_BLOCK: u32 = 20 + 65535 * 64;

/// Which container format an [`crate::Archive`] was read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// GRF (also the `.gpf` patch files and the `.gsf` "Gravity Secure Patch" files of Gravity, which
    /// are GRFs: GRF Editor opens `.gsf` with its GRF reader, too).
    Grf,
    /// Thor patch (`.thor`).
    Thor,
    /// gzip-compressed file list (`.rgz`).
    Rgz,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Grf => "grf",
            Self::Thor => "thor",
            Self::Rgz => "rgz",
        }
    }
}

/// What a patch container says about how it is to be applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchInfo {
    /// Merge into the target GRF (else the files go into the client folder).
    pub merge_into_grf: bool,
    /// Name of the target GRF; empty means the client's default one.
    pub target_grf: Vec<u8>,
}

pub mod flags {
    /// Entry is a file (0 = folder/empty marker).
    pub const FILE: u8 = 0x01;
    /// Header blocks encrypted (DES, mixed); with `FILE` this is 0x03, GRF Editor: `FileAndHeaderCrypted`.
    pub const HEADER_CRYPT: u8 = 0x02;
    /// Whole content encrypted (DES); with `FILE` this is 0x05, GRF Editor: `FileAndDataCrypted`.
    pub const DATA_CRYPT: u8 = 0x04;
    /// Patch directive: delete this path in the target (GRF Editor: `RemoveFile`; Thor and GPF).
    pub const REMOVE: u8 = 0x10;
    /// Encrypted with GRF Editor's own scheme (GRF Editor: `GrfEditorCrypted`).
    pub const GRF_EDITOR_CRYPT: u8 = 0x20;
    /// Gravity's protected-entry scheme of newer archives (needs the key block of a 0x300 archive).
    pub const GRAVITY_CRYPT: u8 = 0x80;
    /// Not stored in any file: set when reading an archive that has the hidden `__encryption.info`
    /// entry and this entry's stream looks encrypted (GRF Editor writes no flag for it). Cleared
    /// when the key is set.
    pub const DETECTED_CRYPT: u8 = 0x40;
    pub const ANY_CRYPT: u8 =
        HEADER_CRYPT | DATA_CRYPT | GRF_EDITOR_CRYPT | GRAVITY_CRYPT | DETECTED_CRYPT;
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
    /// GRF version (0x102 to 0x300); for Thor the mode byte (0x30 patch, 0x21 single file); 0 for RGZ.
    pub version: u32,
    pub kind: Kind,
    /// Thor patches: how to apply them.
    pub patch: Option<PatchInfo>,
}

impl Header {
    pub fn parse(b: &[u8; HEADER_LEN as usize]) -> Result<Self> {
        // "Master of Magic\0" (0x200) or "Event Horizon\0" followed by two more bytes (0x300,
        // measured). Like GRF Editor, another magic is accepted when the version is a GRF's: tools
        // and servers change it (GRF Editor lets it be edited).
        let known_magic = (b[..15] == *b"Master of Magic" && b[15] == 0)
            || (b[..13] == *b"Event Horizon" && b[13] == 0);
        let u32_at = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let version = u32_at(42);
        if !matches!(version, 0x102 | 0x103 | 0x200 | 0x300) {
            return Err(if known_magic {
                Error::UnsupportedVersion(version)
            } else {
                Error::NotGrf("magic \"Master of Magic\" or \"Event Horizon\" not found".into())
            });
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
            kind: Kind::Grf,
            patch: None,
        })
    }

    /// Bytes per entry in the table: 17 (32-bit offset) or 21 (64-bit offset).
    pub fn entry_tail_len(&self) -> usize {
        if self.version == 0x300 { 21 } else { 17 }
    }

    /// "GRF 0x200", "Thor (mode 0x30)", "RGZ": for messages.
    pub fn format_name(&self) -> String {
        match self.kind {
            Kind::Grf => format!("GRF 0x{:03x}", self.version),
            Kind::Thor => format!("Thor (mode 0x{:02x})", self.version),
            Kind::Rgz => "RGZ".to_owned(),
        }
    }

    /// Versions 0x102 and 0x103: scrambled names, plain file table, all file data scrambled.
    pub fn is_legacy(&self) -> bool {
        self.kind == Kind::Grf && self.version < 0x200
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

    /// Empty marker: flags 0 and no data (e.g. a folder entry), a folder entry of libgrf
    /// ([`Entry::is_folder`]), or a delete directive of a patch. Measured: 1 entry in the 2024
    /// `data.grf`; the other flags-0 entries of real archives carry data, see [`Entry::is_raw`].
    pub fn is_marker(&self) -> bool {
        self.is_removal()
            || self.is_folder()
            || self.flags & flags::FILE == 0 && self.zsize == 0 && self.size == 0
    }

    /// Folder entry as libgrf (GRF Tool and the tools built on it) writes it: no file bit and the
    /// fixed sizes `0x449`/`0x714`/`0x55C` at offset `0x55C` (`GRFFILE_DIR_*`), whatever the
    /// other flags say. The client only reads entries with the file bit. Measured: 16 entries in
    /// `Thoth.grf`, 7 in `nodelay.grf` (flags 0x02, the offset points into another entry).
    pub fn is_folder(&self) -> bool {
        self.flags & flags::FILE == 0
            && self.zsize == 0x449
            && self.zsize_aligned == 0x714
            && self.size == 0x55C
            && self.offset == 0x55C
    }

    /// Flags 0 with data: a file stored **uncompressed** (`zsize == size`, GRF Editor:
    /// `RawDataFile`). Measured: all 30 and 25 flags-0 entries of the 0x300 archives (PNGs).
    pub fn is_raw(&self) -> bool {
        self.flags & (flags::FILE | flags::ANY_CRYPT) == 0 && (self.zsize != 0 || self.size != 0)
    }

    /// Any encryption flag: the stored stream is not the plain compressed stream. Copying such an
    /// entry (repack, merge) keeps the stored bytes and flags unchanged.
    /// A patch directive to delete the path (Thor, GPF).
    pub fn is_removal(&self) -> bool {
        self.flags & flags::REMOVE != 0
    }

    /// Content stored without a zlib/LZMA stream (flags without the file bit and with data),
    /// whether or not the old DES scrambling is applied on top. Not Gravity's protected entries
    /// (no file bit either, but their own compressed body).
    pub fn stores_uncompressed(&self) -> bool {
        self.flags & flags::FILE == 0
            && !self.is_protected()
            && !self.is_folder()
            && (self.zsize != 0 || self.size != 0)
    }

    pub fn is_encrypted(&self) -> bool {
        self.flags & flags::ANY_CRYPT != 0
    }

    /// Scrambled with the old DES scheme of [`crate::des`]: readable without a key.
    pub fn is_des(&self) -> bool {
        !self.is_folder()
            && self.flags & (flags::HEADER_CRYPT | flags::DATA_CRYPT) != 0
            && self.flags & (flags::GRF_EDITOR_CRYPT | flags::GRAVITY_CRYPT | flags::DETECTED_CRYPT)
                == 0
    }

    /// Encrypted in a way that needs a key we do not have (GRF Editor's scheme without its key,
    /// unknown combinations). Gravity's protected entries ([`Entry::is_protected`]) are not counted:
    /// they are read with the archive's key block.
    pub fn needs_key(&self) -> bool {
        !self.is_protected()
            && self.flags & (flags::GRF_EDITOR_CRYPT | flags::GRAVITY_CRYPT | flags::DETECTED_CRYPT)
                != 0
    }

    /// Protected by Gravity (flags exactly `0x80`, newer 0x300 archives): readable through
    /// [`crate::gravity`] with the archive's key block, but only kept as it is in a 0x300 archive
    /// that carries the same key block.
    pub fn is_protected(&self) -> bool {
        self.flags == flags::GRAVITY_CRYPT
    }

    /// How the data of a DES entry is scrambled.
    pub fn des_cycle(&self) -> Option<des::Cycle> {
        if !self.is_des() {
            None
        } else if self.flags & flags::DATA_CRYPT != 0 {
            Some(des::Cycle::Head)
        } else {
            Some(des::Cycle::for_size(self.zsize))
        }
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

/// The type byte of a 0x102/0x103 entry: 1 file, 0 "directory" (an entry with a dot in its name is
/// a file stored uncompressed).
const LEGACY_FILE: u8 = 1;

/// Extensions whose legacy entries only have their first 20 blocks scrambled.
pub(crate) fn legacy_head_only(name: &[u8]) -> bool {
    let n = name.len();
    n > 4
        && [&b".gnd"[..], b".gat", b".act", b".str"]
            .iter()
            .any(|e| name[n - 4..].eq_ignore_ascii_case(e))
}

/// Entries of a 0x102/0x103 table: `[u32 len][u16][name: len-6 bytes, DES][u32][u32 zsize+size+715]
/// [u32 zsize_aligned+37579][u32 size][u8 type][u32 offset]`. Directory entries are dropped. The
/// flags are synthesised: `FILE|HEADER_CRYPT` (0x03) or, for `.gnd/.gat/.act/.str`,
/// `FILE|DATA_CRYPT` (0x05), the same flags a 0x200 archive uses for DES entries.
pub fn parse_legacy_entries(table: &[u8], count: usize) -> Result<Vec<Entry>> {
    let corrupt = |i: usize, what: &str| Error::CorruptTable(format!("entry {i}: {what}"));
    let mut entries = Vec::with_capacity(count.min(table.len() / 32));
    let mut p = 0usize;
    for i in 0..count {
        let head = table.get(p..p + 6).ok_or_else(|| corrupt(i, "truncated"))?;
        let len = u32::from_le_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let name_len = len
            .checked_sub(6)
            .ok_or_else(|| corrupt(i, "bad name length"))?;
        let name_at = p + 6;
        let tail_at = p + 4 + len;
        let stored = table
            .get(name_at..name_at + name_len)
            .ok_or_else(|| corrupt(i, "name outside the table"))?;
        let tail = table
            .get(tail_at..tail_at + 17)
            .ok_or_else(|| corrupt(i, "truncated"))?;
        p = tail_at + 17;
        let u32_at =
            |o: usize| u32::from_le_bytes([tail[o], tail[o + 1], tail[o + 2], tail[o + 3]]);
        let name = des::decode_name(stored);
        let kind = tail[12];
        if kind != LEGACY_FILE && !name.contains(&b'.') {
            continue; // a directory
        }
        let size = u32_at(8);
        let zsize = u32_at(0)
            .checked_sub(size)
            .and_then(|v| v.checked_sub(715))
            .ok_or_else(|| corrupt(i, "stored size is negative"))?;
        let zsize_aligned = u32_at(4)
            .checked_sub(37579)
            .ok_or_else(|| corrupt(i, "aligned size is negative"))?;
        let crypt = if legacy_head_only(&name) || name.len() <= 4 {
            flags::DATA_CRYPT
        } else {
            flags::HEADER_CRYPT
        };
        entries.push(Entry {
            name: GrfPath::new(name),
            zsize,
            zsize_aligned,
            size,
            // type 0 with a dot: stored uncompressed (no FILE bit, like `Entry::is_raw`)
            flags: if kind == LEGACY_FILE {
                flags::FILE | crypt
            } else {
                crypt
            },
            offset: u64::from(u32_at(13)),
        });
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
    read_container_with(src, None)
}

/// Like [`read_container`]; `key` opens a file table that is encrypted (GRF Editor scheme).
pub fn read_container_with(
    src: &dyn ReadAt,
    key: Option<&crate::grfcrypt::Key>,
) -> Result<Container> {
    let file_len = src.len()?;
    if file_len < HEADER_LEN {
        return Err(Error::NotGrf(format!("file has only {file_len} bytes")));
    }
    let mut hb = [0u8; HEADER_LEN as usize];
    src.read_exact_at(&mut hb, 0)?;
    let header = Header::parse(&hb)?;

    if header.is_legacy() {
        return read_legacy_container(src, header, file_len);
    }
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
    let encrypted = crate::grfcrypt::looks_encrypted(&packed);
    if let (true, Some(key)) = (encrypted, key) {
        crate::grfcrypt::crypt(key, &mut packed, u64::from(size) + 8);
    }
    let mut table = Vec::with_capacity(size as usize);
    flate2::read::ZlibDecoder::new(&packed[..])
        .take(u64::from(size) + 1)
        .read_to_end(&mut table)
        .map_err(|e| {
            if encrypted && key.is_none() {
                Error::EncryptedTable
            } else if encrypted {
                // the table was decrypted with the wrong key
                Error::WrongKey
            } else {
                Error::CorruptTable(format!("file table does not decompress: {e}"))
            }
        })?;
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

/// 0x102/0x103: the table is not compressed and runs to the end of the file.
fn read_legacy_container(src: &dyn ReadAt, header: Header, file_len: u64) -> Result<Container> {
    let abs = HEADER_LEN
        .checked_add(header.table_offset)
        .filter(|&a| a <= file_len)
        .ok_or_else(|| Error::CorruptTable("file table lies outside the file".into()))?;
    let size = file_len - abs;
    if size > MAX_TABLE_SIZE {
        return Err(Error::CorruptTable(format!("file table of {size} bytes")));
    }
    let mut table = vec![0u8; size as usize];
    src.read_exact_at(&mut table, abs)?;
    let entries = parse_legacy_entries(&table, header.entry_count as usize)?;
    let size =
        u32::try_from(size).map_err(|_| Error::CorruptTable("file table too large".into()))?;
    Ok(Container {
        header,
        table: TableInfo {
            abs_offset: abs,
            prefix_len: 0,
            zsize: size,
            size,
            key_block: 0,
        },
        entries,
        file_len,
    })
}
