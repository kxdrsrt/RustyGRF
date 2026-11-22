//! Updates a client's text tables from the RO Translation Project (`llchrisll/ROenglishRE`):
//! the Rust version of the former Python scripts (they belong to the client project, not to this tool).
//!
//! - [`assemble`]: the project's folders in the order of its `ClientGenerator.bat`.
//! - [`fill`]: entries of the client's tables that are still Korean, taken from the project.
//! - [`personal`] and [`mods`]: your own edits, carried over to a newer translation (per entry,
//!   and per file with a three-way merge).
//! - [`update`] and [`install`]: the whole update of `core.grf`/`cdata.grf` and putting it in
//!   place.
//!
//! Text files are handled as bytes (the scripts held them as Latin-1 text), so no byte of a
//! Korean or Western file is ever changed. Git is used as a program (history of the project,
//! three-way merge, line counts); the archives are read and written by `rgrf` itself
//! ([`GrfTool`]).

use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub mod assemble;
pub mod fill;
pub mod install;
pub mod mods;
pub mod personal;
pub mod update;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("{context}: {source}")]
    Io { context: String, source: io::Error },
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) fn msg<T>(m: impl Into<String>) -> Result<T> {
    Err(Error::Msg(m.into()))
}

pub(crate) trait IoContext<T> {
    fn ctx(self, what: impl FnOnce() -> String) -> Result<T>;
}

impl<T> IoContext<T> for io::Result<T> {
    fn ctx(self, what: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|source| Error::Io {
            context: what(),
            source,
        })
    }
}

/// Whitespace as Python's `str.isspace` sees it on text decoded as Latin-1.
pub(crate) fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t'..=b'\r' | 0x1c..=0x1f | 0x85 | 0xa0)
}

pub(crate) fn rstrip_ws(b: &[u8]) -> &[u8] {
    let n = b.iter().rposition(|&c| !is_ws(c)).map_or(0, |i| i + 1);
    &b[..n]
}

pub(crate) fn rstrip_byte(b: &[u8], c: u8) -> &[u8] {
    let n = b.iter().rposition(|&x| x != c).map_or(0, |i| i + 1);
    &b[..n]
}

pub(crate) fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

pub(crate) fn has_high(b: &[u8]) -> bool {
    b.iter().any(|&c| c >= 0x80)
}

/// Bytes as the text Python made of them (`bytes.decode("latin-1")`), for reports.
pub(crate) fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

pub(crate) fn find_sub(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() || from > hay.len() || hay.len() - from < needle.len() {
        return None;
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub(crate) fn replace_all(hay: &[u8], from: &[u8], to: &[u8]) -> (Vec<u8>, usize) {
    let mut out = Vec::with_capacity(hay.len());
    let (mut i, mut n) = (0, 0);
    while let Some(p) = find_sub(hay, from, i) {
        out.extend_from_slice(&hay[i..p]);
        out.extend_from_slice(to);
        i = p + from.len();
        n += 1;
    }
    out.extend_from_slice(&hay[i..]);
    (out, n)
}

pub(crate) fn split_lines_n(data: &[u8]) -> Vec<&[u8]> {
    data.split(|&b| b == b'\n').collect()
}

pub(crate) fn join_lines_n(lines: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push(b'\n');
        }
        out.extend_from_slice(l);
    }
    out
}

/// The existing spelling of `name` in `directory` when it differs only in case (the first such
/// entry in the order the file system lists them), else `name`.
pub fn fold_name(directory: &Path, name: &str) -> String {
    let lower = name.to_lowercase();
    if let Ok(rd) = fs::read_dir(directory) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            if n.to_lowercase() == lower {
                return n;
            }
        }
    }
    name.to_string()
}

/// [`fold_name`] for every part of a relative path (parts separated by `/` or the system's separator).
pub fn fold_parts(base: &Path, rel: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = base.to_path_buf();
    for part in rel.split(std::path::MAIN_SEPARATOR) {
        let p = fold_name(&cur, part);
        cur = cur.join(&p);
        out.push(p);
    }
    out
}

/// A path below `root` whose parts match case-insensitively (`None` when a part is missing).
pub(crate) fn find_ci(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = root.to_path_buf();
    for part in rel.split('/') {
        let lower = part.to_lowercase();
        let hit = fs::read_dir(&cur).ok()?.flatten().find_map(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            (n.to_lowercase() == lower).then_some(n)
        })?;
        cur = cur.join(hit);
    }
    Some(cur)
}

/// `shutil.copyfile`: the content only.
pub(crate) fn copy_file(src: &Path, dst: &Path) -> Result<()> {
    let mut from = fs::File::open(src).ctx(|| format!("cannot read {}", src.display()))?;
    let mut to = fs::File::create(dst).ctx(|| format!("cannot write {}", dst.display()))?;
    io::copy(&mut from, &mut to).ctx(|| format!("cannot copy {}", src.display()))?;
    Ok(())
}

pub(crate) fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).ctx(|| format!("cannot read {}", path.display()))
}

pub(crate) fn write(path: &Path, data: &[u8]) -> Result<()> {
    fs::write(path, data).ctx(|| format!("cannot write {}", path.display()))
}

pub(crate) fn make_dirs(path: &Path) -> Result<()> {
    fs::create_dir_all(path).ctx(|| format!("cannot create {}", path.display()))
}

/// Files below a folder in the order of `os.walk` (a folder's files, sorted, before its
/// sub-folders, which come in the order the file system lists them): (folder, file names).
pub(crate) fn walk(root: &Path) -> Vec<(PathBuf, Vec<String>)> {
    let mut out = Vec::new();
    fn go(dir: &Path, out: &mut Vec<(PathBuf, Vec<String>)>) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let (mut files, mut dirs) = (Vec::new(), Vec::new());
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // `os.walk` follows the type of the entry itself, links to folders count as folders
            let is_dir = e.path().is_dir();
            if is_dir {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }
        out.push((dir.to_path_buf(), files));
        for d in dirs {
            go(&dir.join(d), out);
        }
    }
    go(root, &mut out);
    out
}

/// Runs git in `repo` (quoting of non-ASCII paths off).
pub(crate) fn git(repo: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["-c", "core.quotepath=false"])
        .args(args)
        .output()
        .ctx(|| "cannot run git (is it installed?)".to_string())?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err: String = err.chars().take(300).collect();
        return msg(format!("git {} failed: {err:?}", args.join(" ")));
    }
    Ok(out.stdout)
}

/// Every blob of `Translation` and `Addons` in the history of the project, newest commit first:
/// (blob, commit date, path). Commits that delete a path are left out.
pub(crate) fn history_blobs(repo: &Path) -> Result<Vec<HistoryBlob>> {
    let log = git(
        repo,
        &[
            "log",
            "--root",
            "--raw",
            "-r",
            "--no-abbrev",
            "--no-renames",
            "--format=C %H %cs",
            "--",
            "Translation",
            "Addons",
        ],
    )?;
    let log = String::from_utf8_lossy(&log);
    let mut out = Vec::new();
    let mut date = String::new();
    for line in log.lines() {
        if let Some(rest) = line.strip_prefix("C ") {
            date = rest.split_whitespace().nth(1).unwrap_or("").to_string();
        } else if line.starts_with(':')
            && let Some((meta, path)) = line.split_once('\t')
        {
            let Some(sha) = meta.split_whitespace().nth(3) else {
                continue;
            };
            if sha.bytes().all(|c| c == b'0') {
                continue;
            }
            out.push(HistoryBlob {
                sha: sha.to_string(),
                date: date.clone(),
                path: path.to_string(),
            });
        }
    }
    Ok(out)
}

pub(crate) struct HistoryBlob {
    pub sha: String,
    pub date: String,
    pub path: String,
}

/// The content of a blob.
pub(crate) fn blob(repo: &Path, sha: &str) -> Result<Vec<u8>> {
    git(repo, &["cat-file", "blob", sha])
}

/// Line endings as git stores them (the project's `.gitattributes` check text out with CRLF).
pub(crate) fn lf(data: &[u8]) -> Vec<u8> {
    if data[..data.len().min(8192)].contains(&0) {
        return data.to_vec();
    }
    replace_all(data, b"\r\n", b"\n").0
}

/// The blob id git gives `data` (SHA-1 of `blob <len>\0` and the content).
pub(crate) fn blob_sha(data: &[u8]) -> String {
    let mut buf = format!("blob {}\0", data.len()).into_bytes();
    buf.extend_from_slice(data);
    let digest = sha1(&buf);
    let mut s = String::with_capacity(40);
    for b in digest {
        let _ = write!(s, "{b:02x}");
    }
    s
}

fn sha1(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [
        0x6745_2301,
        0xEFCD_AB89,
        0x98BA_DCFE,
        0x1032_5476,
        0xC3D2_E1F0,
    ];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 80];
        for (i, c) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A82_7999),
                20..=39 => (b ^ c ^ d, 0x6ED9_EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC),
                _ => (b ^ c ^ d, 0xCA62_C1D6),
            };
            let t = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for (i, v) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// The `rgrf` program that reads and writes the archives (the running one by default).
#[derive(Debug, Clone)]
pub struct GrfTool {
    pub exe: PathBuf,
}

impl GrfTool {
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// The program that is running (the `rgrf` command line tool).
    pub fn current() -> Result<Self> {
        let exe = std::env::current_exe().ctx(|| "cannot find the running program".to_string())?;
        Ok(Self { exe })
    }

    pub(crate) fn run(&self, args: &[&std::ffi::OsStr]) -> Result<std::process::Output> {
        Command::new(&self.exe)
            .args(args)
            .stdin(Stdio::null())
            .output()
            .ctx(|| format!("cannot run {}", self.exe.display()))
    }

    fn run_ok(&self, args: &[&std::ffi::OsStr]) -> Result<String> {
        let r = self.run(args)?;
        if !r.status.success() {
            let tail = |b: &[u8]| {
                let s = String::from_utf8_lossy(b);
                let n = s.chars().count();
                s.chars().skip(n.saturating_sub(500)).collect::<String>()
            };
            return msg(format!(
                "failed: {} {}\n{}\n{}",
                self.exe.display(),
                args.iter()
                    .map(|a| a.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                tail(&r.stdout),
                tail(&r.stderr)
            ));
        }
        Ok(String::from_utf8_lossy(&r.stdout).into_owned())
    }

    /// `rgrf build <folder> -o <grf> --quiet`
    pub fn build(&self, folder: &Path, grf: &Path) -> Result<()> {
        self.run_ok(&[
            "build".as_ref(),
            folder.as_os_str(),
            "-o".as_ref(),
            grf.as_os_str(),
            "--quiet".as_ref(),
        ])
        .map(|_| ())
    }

    /// `rgrf merge <layers...> -o <out> --quiet [--replace-only]` (the first archive wins).
    pub fn merge(&self, layers: &[PathBuf], out: &Path, replace_only: bool) -> Result<()> {
        let mut args: Vec<&std::ffi::OsStr> = vec!["merge".as_ref()];
        args.extend(layers.iter().map(|p| p.as_os_str()));
        args.extend(["-o".as_ref(), out.as_os_str(), "--quiet".as_ref()]);
        if replace_only {
            args.push("--replace-only".as_ref());
        }
        self.run_ok(&args).map(|_| ())
    }

    /// The last line `rgrf <diff|verify> ...` prints (the exit code is not checked).
    pub fn last_line(&self, args: &[&std::ffi::OsStr]) -> Result<String> {
        let r = self.run(args)?;
        let out = String::from_utf8_lossy(&r.stdout);
        Ok(out
            .trim()
            .lines()
            .next_back()
            .unwrap_or_default()
            .to_string())
    }

    /// `rgrf extract <grf> <dest> --quiet` (exit codes 0 and 2 are fine).
    pub fn extract(&self, grf: &Path, dest: &Path) -> Result<()> {
        let r = self.run(&[
            "extract".as_ref(),
            grf.as_os_str(),
            dest.as_os_str(),
            "--quiet".as_ref(),
        ])?;
        match r.status.code() {
            Some(0 | 2) => Ok(()),
            _ => {
                let s = String::from_utf8_lossy(&r.stderr);
                let n = s.chars().count();
                msg(format!(
                    "extract failed: {}",
                    s.chars().skip(n.saturating_sub(300)).collect::<String>()
                ))
            }
        }
    }

    /// `rgrf cat <grf> <path> --names cp949`: the content, or `None` when the archive lacks it.
    pub fn cat(&self, grf: &Path, path: &str) -> Result<Option<Vec<u8>>> {
        let r = self.run(&[
            "cat".as_ref(),
            grf.as_os_str(),
            path.as_ref(),
            "--names".as_ref(),
            "cp949".as_ref(),
        ])?;
        Ok((r.status.success() && !r.stdout.is_empty()).then_some(r.stdout))
    }
}

/// `repr()` of a counter dictionary as Python printed it: `{'update': 3, 'merged': 1}`.
pub(crate) fn dict_repr(counts: &[(String, usize)]) -> String {
    let items: Vec<String> = counts.iter().map(|(k, v)| format!("'{k}': {v}")).collect();
    format!("{{{}}}", items.join(", "))
}

pub(crate) fn bump(counts: &mut Vec<(String, usize)>, key: &str) {
    match counts.iter_mut().find(|(k, _)| k == key) {
        Some(c) => c.1 += 1,
        None => counts.push((key.to_string(), 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blob_ids_are_the_ones_git_makes() {
        // `git hash-object` of an empty file and of "hello\n"
        assert_eq!(blob_sha(b""), "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391");
        assert_eq!(
            blob_sha(b"hello\n"),
            "ce013625030ba8dba906f756967f9e9ca394464a"
        );
    }

    #[test]
    fn line_endings_are_git_style_only_for_text() {
        assert_eq!(lf(b"a\r\nb\r\n"), b"a\nb\n");
        assert_eq!(lf(b"a\r\n\0b"), b"a\r\n\0b");
    }
}
