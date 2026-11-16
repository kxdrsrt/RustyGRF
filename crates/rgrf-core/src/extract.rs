//! Extraction to a directory: safe paths, no silent overwriting, tolerant of damaged entries.

use rayon::prelude::*;
use std::collections::HashSet;
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

/// Turns archive path bytes into a relative path below the destination, or refuses.
pub fn safe_relative_path(name: &GrfPath, mode: NameMode) -> Result<PathBuf> {
    let text = name.display(mode);
    let mut out = PathBuf::new();
    for part in text.split(['\\', '/']) {
        let bad = part.is_empty()
            || part == "."
            || part == ".."
            || part.contains('\0')
            || (cfg!(windows) && part.contains(':'));
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

fn write_file(target: &Path, data: &[u8], opts: &ExtractOptions) -> Result<Outcome> {
    let mut candidate = target.to_path_buf();
    let mut attempt = 0u32;
    loop {
        let mut open = OpenOptions::new();
        open.write(true);
        if opts.overwrite {
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
    fs::create_dir_all(dest)?;
    let root = fs::canonicalize(dest)?;
    let done = AtomicU64::new(0);
    let total = indices.len() as u64;
    let failed = AtomicBool::new(false);

    // Fail on unsafe paths before anything is written.
    let targets: Vec<Option<PathBuf>> = indices
        .iter()
        .map(|&i| {
            let e = a.entry(i)?;
            if e.is_marker() || e.is_encrypted() {
                return Ok(None);
            }
            safe_relative_path(&e.name, opts.names).map(|p| Some(root.join(p)))
        })
        .collect::<Result<_>>()?;

    // Create directories one by one in archive order: on case-insensitive file systems the first
    // spelling wins, so the result must not depend on thread timing.
    let mut made: HashSet<&Path> = HashSet::new();
    for target in targets.iter().flatten() {
        let parent = target.parent().unwrap_or(&root);
        if made.insert(parent) {
            fs::create_dir_all(parent)?;
            // refuse to write through a symlink that leads out of the destination
            if !fs::canonicalize(parent)?.starts_with(&root) {
                return Err(Error::UnsafePath(target.display().to_string()));
            }
        }
    }

    let results: Vec<Result<(usize, ExtractReport)>> = indices
        .par_iter()
        .zip(targets.par_iter())
        .map(|(&i, target)| {
            if failed.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            ctl.check()?;
            let mut r = ExtractReport::default();
            let e = a.entry(i)?;
            let Some(target) = target else {
                if e.is_marker() {
                    r.skipped_markers += 1;
                } else {
                    r.skipped_encrypted += 1;
                }
                return Ok((i, r));
            };
            let d = a.read(i)?;
            match write_file(target, &d.data, opts)? {
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
            Ok((i, r))
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
            Ok((_, p)) => {
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
