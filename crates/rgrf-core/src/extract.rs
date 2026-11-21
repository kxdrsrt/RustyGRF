//! Extraction to a directory: safe paths, no silent overwriting, tolerant of damaged entries.

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::archive::Archive;
use crate::codec::DecodeStatus;
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::{GrfPath, NameMode};
use crate::progress::Control;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnCollision {
    /// Stop with an error when a target already exists (also case-insensitive collisions on the disk).
    #[default]
    Abort,
    Skip,
    /// Write to `name~1.ext`, `name~2.ext`, ...
    Rename,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractOptions {
    pub names: NameMode,
    pub on_collision: OnCollision,
    /// Replace existing files instead of treating them as collisions.
    pub overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Damaged {
    pub index: usize,
    pub status: DecodeStatus,
}

#[derive(Debug, Clone, Default)]
pub struct ExtractReport {
    pub selected: usize,
    pub written: usize,
    pub bytes: u64,
    pub skipped_markers: usize,
    pub skipped_encrypted: usize,
    pub skipped_collisions: usize,
    pub renamed: usize,
    /// Entries written with recovered data only (see [`DecodeStatus`]).
    pub damaged: Vec<Damaged>,
}

/// Names Windows cannot store as written: reserved characters, a trailing dot or space (Win32
/// silently drops them), and device names such as `nul` (data written there disappears).
pub(crate) fn invalid_on_windows(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or_default().trim_end();
    let numbered = match stem.as_bytes() {
        [a, b, c, b'1'..=b'9'] => {
            let prefix = [
                a.to_ascii_lowercase(),
                b.to_ascii_lowercase(),
                c.to_ascii_lowercase(),
            ];
            prefix == *b"com" || prefix == *b"lpt"
        }
        _ => false,
    };
    let device = numbered
        || matches!(
            stem.to_ascii_lowercase().as_str(),
            "con" | "prn" | "aux" | "nul" | "conin$" | "conout$"
        );
    part.chars()
        .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c < ' ')
        || part.ends_with(['.', ' '])
        || device
}

/// Turns archive path bytes into a relative path below the destination, or refuses.
pub fn safe_relative_path(name: &GrfPath, mode: NameMode) -> Result<PathBuf> {
    let text = name.display(mode);
    let mut out = PathBuf::new();
    for part in text.split(['\\', '/']) {
        let bad = part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\0')
            || (cfg!(windows) && invalid_on_windows(part));
        if bad {
            return Err(Error::UnsafePath(name.display(NameMode::Raw)));
        }
        out.push(part);
    }
    // a component such as "C:" or a root would make `join` replace the destination
    if out.components().any(|c| !matches!(c, Component::Normal(_))) {
        return Err(Error::UnsafePath(name.display(NameMode::Raw)));
    }
    Ok(out)
}

/// The first of `name~1.ext`, `name~2.ext`, ... that does not exist (what [`OnCollision::Rename`]
/// writes to).
pub fn free_name(path: &Path) -> PathBuf {
    (1..)
        .map(|n| renamed(path, n))
        .find(|p| !p.exists())
        .expect("some number is free")
}

fn renamed(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    path.with_file_name(format!("{stem}~{n}{ext}"))
}

enum Outcome {
    Written { bytes: u64, renamed: bool },
    SkippedCollision,
}

/// The deepest existing ancestor of `dir` must already lie inside `root`, otherwise
/// `create_dir_all` could create directories outside of it through a symlink.
fn ensure_inside(root: &Path, dir: &Path) -> Result<()> {
    let mut probe = dir;
    while fs::symlink_metadata(probe).is_err() {
        match probe.parent() {
            Some(p) => probe = p,
            None => return Err(Error::UnsafePath(dir.display().to_string())),
        }
    }
    if fs::canonicalize(probe)?.starts_with(root) {
        Ok(())
    } else {
        Err(Error::UnsafePath(dir.display().to_string()))
    }
}

fn write_file(target: &Path, data: &[u8], opts: &ExtractOptions) -> Result<Outcome> {
    let mut candidate = target.to_path_buf();
    let mut attempt = 0u32;
    loop {
        let mut open = OpenOptions::new();
        open.write(true);
        if opts.overwrite {
            // never write through a symlink that already sits at the target
            if fs::symlink_metadata(&candidate).is_ok_and(|m| m.file_type().is_symlink()) {
                return Err(Error::UnsafePath(candidate.display().to_string()));
            }
            open.create(true).truncate(true);
        } else {
            open.create_new(true);
        }
        match open.open(&candidate) {
            Ok(mut f) => {
                f.write_all(data)?;
                return Ok(Outcome::Written {
                    bytes: data.len() as u64,
                    renamed: attempt > 0,
                });
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => match opts.on_collision {
                OnCollision::Abort => {
                    return Err(Error::TargetExists(candidate.display().to_string()));
                }
                OnCollision::Skip => return Ok(Outcome::SkippedCollision),
                OnCollision::Rename => {
                    attempt += 1;
                    candidate = renamed(target, attempt);
                }
            },
            Err(e) => return Err(e.into()),
        }
    }
}

/// Extracts the given entries (indices) below `dest`. Existing files are never replaced unless
/// `overwrite` is set. Encrypted entries and markers are skipped and counted.
pub fn extract<R: ReadAt>(
    a: &Archive<R>,
    indices: &[usize],
    dest: &Path,
    opts: &ExtractOptions,
    ctl: &Control,
) -> Result<ExtractReport> {
    extract_with(a, indices, dest, opts, ctl, None)
}

/// Turns the content of an entry (given its path and bytes) into what is written, or `None` to
/// write it as it is.
pub type Convert = fn(&[u8], &[u8]) -> Option<Vec<u8>>;

/// Like [`extract`]; `convert` can change the content of entries on the way out.
pub fn extract_with<R: ReadAt>(
    a: &Archive<R>,
    indices: &[usize],
    dest: &Path,
    opts: &ExtractOptions,
    ctl: &Control,
    convert: Option<Convert>,
) -> Result<ExtractReport> {
    fs::create_dir_all(dest)?;
    let root = fs::canonicalize(dest)?;
    let done = AtomicU64::new(0);
    let total = indices.len() as u64;
    let failed = AtomicBool::new(false);

    // Fail on unsafe paths before anything is written.
    let mut targets: Vec<Option<PathBuf>> = indices
        .iter()
        .map(|&i| {
            let e = a.entry(i)?;
            if e.is_marker() || e.needs_key() {
                return Ok(None);
            }
            safe_relative_path(&e.name, opts.names).map(Some)
        })
        .collect::<Result<_>>()?;

    // The first spelling of every folder and file name wins, as on case-insensitive file systems
    // (the client does not care about case either). Doing it ourselves gives the same tree on
    // Linux, where `data/` and `DATA/` would otherwise become two folders.
    let mut spelling: HashMap<String, std::ffi::OsString> = HashMap::new();
    for rel in targets.iter_mut().flatten() {
        let mut key = String::new();
        let mut unified = PathBuf::new();
        for part in rel.components() {
            let part = part.as_os_str();
            key.push('/');
            key.push_str(&part.to_string_lossy().to_lowercase());
            unified.push(
                spelling
                    .entry(key.clone())
                    .or_insert_with(|| part.to_owned()),
            );
        }
        *rel = unified;
    }
    let targets: Vec<Option<PathBuf>> = targets
        .into_iter()
        .map(|t| t.map(|rel| root.join(rel)))
        .collect();

    // Create directories one by one in archive order: on case-insensitive file systems the first
    // spelling wins, so the result must not depend on thread timing.
    let mut made: HashSet<&Path> = HashSet::new();
    for target in targets.iter().flatten() {
        let parent = target.parent().unwrap_or(&root);
        if made.insert(parent) {
            ensure_inside(&root, parent)?;
            fs::create_dir_all(parent)?;
            // refuse to write through a symlink that leads out of the destination
            if !fs::canonicalize(parent)?.starts_with(&root) {
                return Err(Error::UnsafePath(target.display().to_string()));
            }
        }
    }

    // Entries that end up in the same file (duplicate or case-colliding paths) are written one
    // after the other in archive order, so which one wins never depends on thread timing.
    let mut group_of: HashMap<String, usize> = HashMap::new();
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for (pos, target) in targets.iter().enumerate() {
        let Some(target) = target else {
            groups.push(vec![pos]);
            continue;
        };
        let g = *group_of
            .entry(target.to_string_lossy().to_lowercase())
            .or_insert_with(|| {
                groups.push(Vec::new());
                groups.len() - 1
            });
        groups[g].push(pos);
    }

    let work = |pos: usize, r: &mut ExtractReport| -> Result<()> {
        if failed.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        ctl.check()?;
        let i = indices[pos];
        let e = a.entry(i)?;
        let Some(target) = &targets[pos] else {
            if e.is_marker() {
                r.skipped_markers += 1;
            } else {
                r.skipped_encrypted += 1;
            }
            return Ok(());
        };
        let d = a.read(i)?;
        let converted = convert.and_then(|f| f(e.name.as_bytes(), &d.data));
        match write_file(target, converted.as_deref().unwrap_or(&d.data), opts)? {
            Outcome::Written { bytes, renamed } => {
                r.written += 1;
                r.bytes += bytes;
                r.renamed += usize::from(renamed);
            }
            Outcome::SkippedCollision => r.skipped_collisions += 1,
        }
        if !d.status.is_ok() {
            r.damaged.push(Damaged {
                index: i,
                status: d.status,
            });
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n.is_multiple_of(256) || n == total {
            ctl.report(n, total);
        }
        Ok(())
    };

    let results: Vec<Result<ExtractReport>> = groups
        .par_iter()
        .map(|group| {
            let mut r = ExtractReport::default();
            for &pos in group {
                work(pos, &mut r)?;
            }
            Ok(r)
        })
        .map(|r| {
            if r.is_err() {
                failed.store(true, Ordering::Relaxed);
            }
            r
        })
        .collect();

    let mut report = ExtractReport {
        selected: indices.len(),
        ..ExtractReport::default()
    };
    let mut first_err = None;
    for r in results {
        match r {
            Ok(p) => {
                report.written += p.written;
                report.bytes += p.bytes;
                report.skipped_markers += p.skipped_markers;
                report.skipped_encrypted += p.skipped_encrypted;
                report.skipped_collisions += p.skipped_collisions;
                report.renamed += p.renamed;
                report.damaged.extend(p.damaged);
            }
            Err(e) => {
                if matches!(first_err, None | Some(Error::Cancelled)) {
                    first_err = Some(e);
                }
            }
        }
    }
    report.damaged.sort_by_key(|d| d.index);
    first_err.map_or(Ok(report), Err)
}
