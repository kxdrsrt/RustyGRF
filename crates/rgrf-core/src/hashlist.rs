//! GRF Editor's hash lists (`TKHO` files, GrfCL's `hash` and `hashComp`): the hashes of all files
//! of a folder, with the entries of the archives in it (`archive.grf?data\path`), to compare a
//! client with a master list. Ported from GRF Editor's `HashObject`, `FolderHash` and
//! `HashObjectComparer` (with the permission of its author, see `docs/05-licensing.md`).
//!
//! The "quick" hash GRF Editor uses for folders is the MD5 of the first 4096 bytes of a file
//! (what its code does, though it means to take 2048 bytes from the start and 2048 from the end).

use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;

use crate::hash::Digest;
use crate::name::NameMode;
use crate::progress::Control;
use crate::{Archive, DecodeStatus};

const MAGIC: &[u8; 4] = b"TKHO";
/// What GRF Editor writes for an empty item of a list.
const EMPTY_ITEM: &str = "__%EmptyString%";
const ARCHIVE_EXTENSIONS: [&str; 5] = ["grf", "rgz", "gpf", "thor", "gsf"];

#[derive(Debug, thiserror::Error)]
pub enum HashListError {
    #[error("not a hash list (no TKHO header)")]
    NotAHashList,
    #[error("this version of hash list is not supported")]
    Version,
    #[error("unknown hash method {0}")]
    UnknownMethod(u8),
    #[error("the hash list is damaged")]
    Damaged,
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
    #[error("the operation was cancelled")]
    Cancelled,
}

type Result<T> = std::result::Result<T, HashListError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashMethod {
    Crc32,
    Md5,
    FileSize,
    /// MD5 of the first 4096 bytes (see the module docs).
    QuickMd5,
}

impl HashMethod {
    fn id(self) -> u8 {
        match self {
            Self::Crc32 => 0,
            Self::Md5 => 1,
            Self::FileSize => 2,
            Self::QuickMd5 => 3,
        }
    }

    fn from_id(id: u8) -> Result<Self> {
        Ok(match id {
            0 => Self::Crc32,
            1 => Self::Md5,
            2 => Self::FileSize,
            3 => Self::QuickMd5,
            n => return Err(HashListError::UnknownMethod(n)),
        })
    }

    pub fn len(self) -> usize {
        match self {
            Self::Crc32 | Self::FileSize => 4,
            Self::Md5 | Self::QuickMd5 => 16,
        }
    }

    pub fn is_empty(self) -> bool {
        false
    }

    /// What stands for a file that could not be read.
    pub fn error(self) -> Vec<u8> {
        vec![0xff; self.len()]
    }

    pub fn compute(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Crc32 => crc32fast::hash(data).to_le_bytes().to_vec(),
            Self::FileSize => (data.len() as i32).to_le_bytes().to_vec(),
            Self::Md5 => Digest::of(data).md5.to_vec(),
            Self::QuickMd5 => Digest::of(&data[..data.len().min(4096)]).md5.to_vec(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Crc32 => "Crc32",
            Self::Md5 => "Md5",
            Self::FileSize => "File size",
            Self::QuickMd5 => "Md5 Quick",
        }
    }
}

/// How the client of a hash list is searched (kept as GRF Editor wrote it).
pub const EXPLORE_EVERYTHING: u8 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HashList {
    pub explore: u8,
    pub partial: bool,
    pub method: HashMethod,
    pub head_dir: String,
    /// The hash of every file by its path: `folder\file.ext`, or `archive.grf?data\file.ext`.
    pub hashes: BTreeMap<String, Vec<u8>>,
}

/// `Path.GetDirectoryName` of a path with `\` or `/`.
fn dirname(p: &str) -> &str {
    p.rfind(['\\', '/']).map_or("", |i| &p[..i])
}

fn basename(p: &str) -> &str {
    p.rfind(['\\', '/']).map_or(p, |i| &p[i + 1..])
}

/// `Path.Combine` on Windows (an archive's top level, `a.grf?`, takes the name as it is).
fn combine(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else if dir.ends_with(['\\', '/', '?']) {
        format!("{dir}{name}")
    } else {
        format!("{dir}\\{name}")
    }
}

fn head_path(key: &str) -> String {
    match key.split_once('?') {
        None => dirname(key).to_string(),
        Some((file, rel)) => format!("{file}?{}", dirname(rel)),
    }
}

fn file_part(key: &str) -> &str {
    match key.split_once('?') {
        None => basename(key),
        Some((_, rel)) => basename(rel),
    }
}

fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn from_utf16(b: &[u8]) -> Result<String> {
    let (pairs, rest) = b.as_chunks::<2>();
    if !rest.is_empty() {
        return Err(HashListError::Damaged);
    }
    let units: Vec<u16> = pairs.iter().map(|c| u16::from_le_bytes(*c)).collect();
    String::from_utf16(&units).map_err(|_| HashListError::Damaged)
}

fn list_to_string(items: &[String]) -> String {
    items
        .iter()
        .map(|s| if s.is_empty() { EMPTY_ITEM } else { s })
        .collect::<Vec<_>>()
        .join(":")
}

fn string_to_list(s: &str) -> Vec<String> {
    if s.is_empty() {
        return Vec::new();
    }
    s.split(':')
        .map(|v| if v == EMPTY_ITEM { "" } else { v }.to_string())
        .collect()
}

/// The directories the file entries refer to, with every parent, and then each shortened by
/// the first earlier directory it starts with (`|3_` stands for the directory number 3).
fn compress_directories(keys: &[&String]) -> (Vec<String>, Vec<i32>) {
    let mut heads: Vec<String> = Vec::new();
    for k in keys {
        let h = head_path(k);
        if !heads.contains(&h) {
            heads.push(h);
        }
    }
    let mut all: BTreeSet<String> = BTreeSet::new();
    for h in &heads {
        all.insert(h.clone());
        let mut d = h.as_str();
        while !d.is_empty() {
            d = dirname(d);
            if !d.is_empty() {
                all.insert(d.to_string());
            }
        }
    }
    let mut dirs: Vec<String> = all.into_iter().collect();
    let refs: Vec<i32> = keys
        .iter()
        .map(|k| {
            let h = head_path(k);
            dirs.iter().position(|d| *d == h).unwrap_or(0) as i32
        })
        .collect();
    for i in 0..dirs.len().saturating_sub(1) {
        let cur = dirs[i].clone();
        if cur.is_empty() {
            continue;
        }
        for d in &mut dirs[i + 1..] {
            if d.starts_with(&cur) {
                *d = format!("|{i}_{}", &d[cur.len()..]);
            }
        }
    }
    (dirs, refs)
}

fn expand_directories(dirs: &mut [String]) -> Result<()> {
    for i in (1..dirs.len()).rev() {
        let item = dirs[i].clone();
        if !item.starts_with('|') {
            continue;
        }
        let end = item[1..].find('_').ok_or(HashListError::Damaged)? + 1;
        let index: usize = item[1..end].parse().map_err(|_| HashListError::Damaged)?;
        if index >= dirs.len() {
            return Err(HashListError::Damaged);
        }
        let marker = format!("|{index}_");
        let with = dirs[index].clone();
        for d in dirs.iter_mut() {
            *d = d.replace(&marker, &with);
        }
    }
    Ok(())
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.0.len() {
            return Err(HashListError::Damaged);
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }

    fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn string(&mut self) -> Result<String> {
        let n = usize::try_from(self.i32()?).map_err(|_| HashListError::Damaged)?;
        from_utf16(self.take(n)?)
    }
}

impl HashList {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 9 || &bytes[..4] != MAGIC {
            return Err(HashListError::NotAHashList);
        }
        if bytes[4] != 1 {
            return Err(HashListError::Version);
        }
        let (explore, partial, method) = (bytes[6], bytes[7] != 0, HashMethod::from_id(bytes[8])?);
        let mut table = Vec::new();
        ZlibDecoder::new(&bytes[9..])
            .read_to_end(&mut table)
            .map_err(|_| HashListError::Damaged)?;
        let mut r = Reader(&table);
        let head_dir = r.string()?;
        let mut dirs = string_to_list(&r.string()?);
        expand_directories(&mut dirs)?;
        let count = usize::try_from(r.i32()?).map_err(|_| HashListError::Damaged)?;
        if count > table.len() {
            return Err(HashListError::Damaged);
        }
        let refs = (0..count).map(|_| r.i32()).collect::<Result<Vec<_>>>()?;
        let names = string_to_list(&r.string()?);
        if names.len() != count {
            return Err(HashListError::Damaged);
        }
        let mut hashes = BTreeMap::new();
        for (name, dir) in names.iter().zip(&refs) {
            let dir = usize::try_from(*dir)
                .ok()
                .and_then(|d| dirs.get(d))
                .ok_or(HashListError::Damaged)?;
            let hash = r.take(method.len())?.to_vec();
            hashes.insert(combine(dir, name), hash);
        }
        Ok(Self {
            explore,
            partial,
            method,
            head_dir,
            hashes,
        })
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let keys: Vec<&String> = self.hashes.keys().collect();
        let (dirs, refs) = compress_directories(&keys);
        let mut table = Vec::new();
        let put_string = |t: &mut Vec<u8>, s: &str| {
            let b = utf16(s);
            t.extend((b.len() as i32).to_le_bytes());
            t.extend(b);
        };
        put_string(&mut table, &self.head_dir);
        put_string(&mut table, &list_to_string(&dirs));
        table.extend((keys.len() as i32).to_le_bytes());
        for r in &refs {
            table.extend(r.to_le_bytes());
        }
        let names: Vec<String> = keys.iter().map(|k| file_part(k).to_string()).collect();
        put_string(&mut table, &list_to_string(&names));
        for h in self.hashes.values() {
            if h.len() != self.method.len() {
                return Err(HashListError::Damaged);
            }
            table.extend(h);
        }
        let mut out = MAGIC.to_vec();
        out.extend([1, 0, self.explore, u8::from(self.partial), self.method.id()]);
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&table)
            .map_err(|e| HashListError::Io("zlib".into(), e))?;
        out.extend(
            z.finish()
                .map_err(|e| HashListError::Io("zlib".into(), e))?,
        );
        Ok(out)
    }

    pub fn read(path: &Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).map_err(|e| HashListError::Io(path.display().to_string(), e))?;
        Self::parse(&bytes)
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        std::fs::write(path, self.to_bytes()?)
            .map_err(|e| HashListError::Io(path.display().to_string(), e))
    }
}

fn is_archive(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ARCHIVE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|e| HashListError::Io(dir.display().to_string(), e))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

/// A `*` and `?` pattern for the file names of the folder hash (`*` is everything).
pub(crate) fn wildcard(pattern: &str, name: &str) -> bool {
    fn go(p: &[u8], n: &[u8]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some(b'*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some(b'?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a.eq_ignore_ascii_case(b) && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    go(pattern.as_bytes(), name.as_bytes())
}

/// GRF Editor's "hash a folder": every file below `folder` (matching `pattern` by name), and the
/// entries of the GRF, RGZ, GPF, Thor and GSF files, hashed after decoding. Progress is
/// reported per file.
pub fn hash_folder(
    folder: &Path,
    pattern: &str,
    method: HashMethod,
    ctl: &Control,
) -> Result<HashList> {
    if !folder.is_dir() {
        return Err(HashListError::Io(
            folder.display().to_string(),
            std::io::Error::from(std::io::ErrorKind::NotFound),
        ));
    }
    let mut files = Vec::new();
    walk(folder, &mut files)?;
    let mut hashes = BTreeMap::new();
    let total = files.len() as u64;
    for (i, file) in files.iter().enumerate() {
        ctl.check().map_err(|_| HashListError::Cancelled)?;
        ctl.report(i as u64, total);
        let name = file
            .file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        if !wildcard(pattern, &name) {
            continue;
        }
        let rel = file
            .strip_prefix(folder)
            .unwrap_or(file)
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\\");
        if is_archive(file) {
            let a = Archive::open(file).map_err(|e| {
                HashListError::Io(
                    file.display().to_string(),
                    std::io::Error::other(e.to_string()),
                )
            })?;
            for (n, e) in a.entries().iter().enumerate() {
                if e.is_marker() {
                    continue;
                }
                let key = format!("{rel}?{}", e.name.display(NameMode::Cp1252));
                let hash = match a.read(n) {
                    Ok(d) if d.status == DecodeStatus::Ok => method.compute(&d.data),
                    _ => method.error(),
                };
                hashes.insert(key, hash);
            }
        } else {
            let data = std::fs::read(file)
                .map_err(|e| HashListError::Io(file.display().to_string(), e))?;
            hashes.insert(rel, method.compute(&data));
        }
    }
    ctl.report(total, total);
    Ok(HashList {
        explore: EXPLORE_EVERYTHING,
        partial: false,
        // GRF Editor hashes quickly but writes "Md5" in the file, and its tools read it that way
        method: if method == HashMethod::QuickMd5 {
            HashMethod::Md5
        } else {
            method
        },
        head_dir: folder.display().to_string(),
        hashes,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// In the master list, not at the user.
    Missing,
    /// Both have the file, with other content.
    Different,
    /// The user has the file at another place, which overrides the master's.
    PathOverride,
    /// Only the user has it.
    Extra,
    /// The user's file could not be read.
    Unreadable,
}

impl Issue {
    pub fn name(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Different => "different",
            Self::PathOverride => "path override",
            Self::Extra => "extra",
            Self::Unreadable => "unreadable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Difference {
    pub issue: Issue,
    /// The path in the master list (or the user's for extra and unreadable files).
    pub path: String,
    /// For a path override: where the user has the file.
    pub user_path: Option<String>,
}

/// GRF Editor's `HashObjectComparer`: what the user's list lacks, changes or adds against the
/// master's. Unless `exact_path`, a file of an archive of the master (`a.grf?data\x`) is also
/// looked for as a loose file `data\x` and in the archives the user has, because the client
/// finds it there too.
pub fn compare(master: &HashList, user: &HashList, exact_path: bool) -> Vec<Difference> {
    let mut out = Vec::new();
    let mut users: BTreeMap<&str, &[u8]> = BTreeMap::new();
    for (k, v) in &user.hashes {
        if *v == master.method.error() {
            out.push(Difference {
                issue: Issue::Unreadable,
                path: k.clone(),
                user_path: None,
            });
        } else {
            users.insert(k, v);
        }
    }
    let scanned: BTreeSet<&str> = user
        .hashes
        .keys()
        .filter_map(|k| {
            k.split_once('?')
                .filter(|(_, r)| !r.is_empty())
                .map(|(f, _)| f)
        })
        .collect();
    let mut processed: Vec<&str> = Vec::new();
    for (key, hash) in &master.hashes {
        if !exact_path && let Some((_, data_path)) = key.split_once('?') {
            let data_path = data_path.split('?').next().unwrap_or(data_path);
            let mut missing = true;
            if let Some(h) = users.remove(data_path) {
                if h != hash.as_slice() {
                    out.push(Difference {
                        issue: Issue::PathOverride,
                        path: key.clone(),
                        user_path: Some(data_path.to_string()),
                    });
                }
                missing = false;
                processed.push(key);
            }
            for grf in &scanned {
                let p = format!("{grf}?{data_path}");
                if let Some(h) = users.remove(p.as_str()) {
                    if h != hash.as_slice() {
                        out.push(Difference {
                            issue: if p == *key {
                                Issue::Different
                            } else {
                                Issue::PathOverride
                            },
                            path: key.clone(),
                            user_path: (p != *key).then_some(p),
                        });
                    }
                    missing = false;
                    processed.push(key);
                }
            }
            if missing {
                out.push(Difference {
                    issue: Issue::Missing,
                    path: key.clone(),
                    user_path: None,
                });
            }
            continue;
        }
        match users.remove(key.as_str()) {
            Some(h) => {
                if h != hash.as_slice() {
                    out.push(Difference {
                        issue: Issue::Different,
                        path: key.clone(),
                        user_path: None,
                    });
                }
                processed.push(key);
            }
            None => out.push(Difference {
                issue: Issue::Missing,
                path: key.clone(),
                user_path: None,
            }),
        }
    }
    for p in processed {
        users.remove(p);
    }
    out.extend(users.keys().map(|k| Difference {
        issue: Issue::Extra,
        path: (*k).to_string(),
        user_path: None,
    }));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(items: &[(&str, &[u8])]) -> HashList {
        HashList {
            explore: EXPLORE_EVERYTHING,
            partial: false,
            method: HashMethod::Md5,
            head_dir: "C:\\client".into(),
            hashes: items
                .iter()
                .map(|(k, v)| {
                    let mut h = Digest::of(v).md5.to_vec();
                    h.truncate(16);
                    ((*k).to_string(), h)
                })
                .collect(),
        }
    }

    #[test]
    fn the_quick_hash_is_the_md5_of_the_first_4096_bytes() {
        let big: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        assert_eq!(
            HashMethod::QuickMd5.compute(&big),
            Digest::of(&big[..4096]).md5
        );
        assert_eq!(HashMethod::QuickMd5.compute(b"abc"), Digest::of(b"abc").md5);
        assert_eq!(HashMethod::FileSize.compute(&big), 10_000i32.to_le_bytes());
        assert_eq!(
            HashMethod::Crc32.compute(b"123456789"),
            0xcbf4_3926u32.to_le_bytes()
        );
    }

    #[test]
    fn a_hash_list_round_trips_with_nested_folders_and_archive_entries() {
        let l = list(&[
            ("ro.exe", b"exe"),
            ("data\\a.txt", b"a"),
            ("data\\texture\\b.bmp", b"b"),
            ("data\\texture\\sub\\c.bmp", b"c"),
            ("data\\texture\\sub\\deeper\\d.bmp", b"d"),
            ("data.grf?data\\x.txt", b"x"),
            ("data.grf?data\\texture\\y.bmp", b"y"),
            ("data.grf?top.txt", b"t"),
        ]);
        let bytes = l.to_bytes().unwrap();
        assert_eq!(&bytes[..9], &[b'T', b'K', b'H', b'O', 1, 0, 2, 0, 1]);
        let back = HashList::parse(&bytes).unwrap();
        assert_eq!(back.hashes.len(), l.hashes.len());
        for (k, v) in &l.hashes {
            assert_eq!(back.hashes.get(k), Some(v), "{k}");
        }
        assert_eq!(
            (back.head_dir.as_str(), back.method),
            ("C:\\client", HashMethod::Md5)
        );
        // damaged and foreign data are refused
        assert!(HashList::parse(b"NOPE").is_err());
        assert!(HashList::parse(&bytes[..12]).is_err());
        let empty = list(&[]).to_bytes().unwrap();
        assert!(HashList::parse(&empty).unwrap().hashes.is_empty());
    }

    #[test]
    fn comparing_finds_missing_changed_extra_and_overriding_files() {
        let master = list(&[
            ("data.grf?data\\a.txt", b"a"),
            ("data.grf?data\\b.txt", b"b"),
            ("data.grf?data\\c.txt", b"c"),
            ("data.grf?data\\d.txt", b"d"),
            ("ro.exe", b"exe"),
        ]);
        let user = list(&[
            ("data.grf?data\\a.txt", b"a"),
            ("data.grf?data\\b.txt", b"CHANGED"),
            ("data\\c.txt", b"other"),
            ("ro.exe", b"exe"),
            ("extra.txt", b"e"),
        ]);
        let issues: Vec<(Issue, &str)> = compare(&master, &user, false)
            .iter()
            .map(|d| (d.issue, d.path.as_str()))
            .map(|(i, p)| (i, Box::leak(p.to_string().into_boxed_str()) as &str))
            .collect();
        assert!(issues.contains(&(Issue::Different, "data.grf?data\\b.txt")));
        assert!(issues.contains(&(Issue::PathOverride, "data.grf?data\\c.txt")));
        assert!(issues.contains(&(Issue::Missing, "data.grf?data\\d.txt")));
        assert!(issues.contains(&(Issue::Extra, "extra.txt")));
        assert_eq!(issues.len(), 4, "{issues:?}");
        // the exact comparison does not look for loose files
        let exact = compare(&master, &user, true);
        assert!(
            exact
                .iter()
                .any(|d| d.issue == Issue::Missing && d.path.ends_with("c.txt"))
        );
        assert!(
            exact
                .iter()
                .any(|d| d.issue == Issue::Extra && d.path == "data\\c.txt")
        );
    }

    #[test]
    fn a_folder_with_an_archive_is_hashed_by_its_decoded_entries() {
        let dir = std::env::temp_dir().join(format!("rgrf-hashlist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/f.txt"), b"hello").unwrap();
        std::fs::write(dir.join("skip.bin"), b"x").unwrap();
        let src = dir.join("src/data");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("in.txt"), b"inside").unwrap();
        crate::build::build(
            &dir.join("src"),
            &dir.join("sub/data.grf"),
            &crate::build::BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
        let l = hash_folder(&dir, "*", HashMethod::Md5, &Control::default()).unwrap();
        let keys: Vec<&str> = l.hashes.keys().map(String::as_str).collect();
        assert!(keys.contains(&"sub\\f.txt"), "{keys:?}");
        assert_eq!(
            l.hashes.get("sub\\data.grf?data\\in.txt"),
            Some(&Digest::of(b"inside").md5.to_vec())
        );
        let only = hash_folder(&dir, "*.txt", HashMethod::QuickMd5, &Control::default()).unwrap();
        assert_eq!(only.hashes.len(), 2, "sub\\f.txt and src\\data\\in.txt");
        assert!(hash_folder(&dir.join("none"), "*", HashMethod::Md5, &Control::default()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
