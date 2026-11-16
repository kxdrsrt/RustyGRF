//! Structural and content checks of an archive.

use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::archive::Archive;
use crate::codec::DecodeStatus;
use crate::error::Result;
use crate::io::ReadAt;
use crate::progress::Control;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Worth knowing, nothing to fix (markers, shared data blocks).
    Info,
    /// Damaged or unchecked, but the client copes (truncated stream, encrypted entry, duplicate path).
    Warning,
    /// The archive is structurally wrong or an entry cannot be read.
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueKind {
    Markers,
    SharedData,
    OutOfRange,
    Overlap,
    DuplicatePath,
    Alignment,
    Truncated,
    SizeMismatch,
    Corrupt,
    Encrypted,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub kind: IssueKind,
    /// Entry index, `None` for archive-wide findings.
    pub entry: Option<usize>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct VerifyReport {
    pub entries: usize,
    pub files: usize,
    pub markers: usize,
    /// Entries whose content was decoded.
    pub decoded: usize,
    pub issues: Vec<Issue>,
}

impl VerifyReport {
    pub fn count(&self, s: Severity) -> usize {
        self.issues.iter().filter(|i| i.severity == s).count()
    }

    /// 0 clean (infos only), 2 warnings, 3 errors (matches the CLI exit codes).
    pub fn exit_code(&self) -> i32 {
        if self.count(Severity::Error) > 0 {
            3
        } else if self.count(Severity::Warning) > 0 {
            2
        } else {
            0
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VerifyOptions {
    /// Decode every entry (slow, reads the whole archive). Without it only the table is checked.
    pub decode: bool,
}

impl Default for VerifyOptions {
    fn default() -> Self {
        Self { decode: true }
    }
}

pub fn verify<R: ReadAt>(
    a: &Archive<R>,
    opts: VerifyOptions,
    ctl: &Control,
) -> Result<VerifyReport> {
    let mut rep = VerifyReport {
        entries: a.len(),
        ..VerifyReport::default()
    };
    rep.markers = a.entries().iter().filter(|e| e.is_marker()).count();
    rep.files = rep.entries - rep.markers;
    if rep.markers > 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::Markers,
            entry: None,
            message: format!(
                "{} folder/empty marker entries (flags 0, size 0), not extracted",
                rep.markers
            ),
        });
    }

    let unreadable = check_structure(a, &mut rep);

    if opts.decode {
        let total = a.len() as u64;
        let done = AtomicU64::new(0);
        let found: Vec<Result<Option<Issue>>> = (0..a.len())
            .into_par_iter()
            .map(|i| {
                ctl.check()?;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                if n.is_multiple_of(512) || n == total {
                    ctl.report(n, total);
                }
                if unreadable.contains_key(&i) {
                    return Ok(None);
                }
                let d = a.read(i)?;
                Ok(status_issue(i, &d.status))
            })
            .collect();
        for r in found {
            if let Some(issue) = r? {
                rep.issues.push(issue);
            }
        }
        rep.decoded = a.len() - unreadable.len();
    }
    rep.issues
        .sort_by_key(|i| (std::cmp::Reverse(i.severity), i.entry.unwrap_or(usize::MAX)));
    Ok(rep)
}

fn status_issue(index: usize, s: &DecodeStatus) -> Option<Issue> {
    let (severity, kind, message) = match s {
        DecodeStatus::Ok => return None,
        DecodeStatus::Truncated { got, expected } => (
            Severity::Warning,
            IssueKind::Truncated,
            format!("stream ends early: {got} of {expected} bytes recoverable"),
        ),
        DecodeStatus::SizeMismatch { got, expected } => (
            Severity::Warning,
            IssueKind::SizeMismatch,
            format!("decoded {got} bytes, table says {expected}"),
        ),
        DecodeStatus::Corrupt(m) => (
            Severity::Error,
            IssueKind::Corrupt,
            format!("damaged stream: {m}"),
        ),
        DecodeStatus::Encrypted => (
            Severity::Warning,
            IssueKind::Encrypted,
            "encrypted entry, content not checked (decryption not supported yet)".to_owned(),
        ),
    };
    Some(Issue {
        severity,
        kind,
        entry: Some(index),
        message,
    })
}

/// Returns the entries that cannot be read (out of range) so they are not decoded.
fn check_structure<R: ReadAt>(a: &Archive<R>, rep: &mut VerifyReport) -> HashMap<usize, ()> {
    let file_len = a.file_len();
    let t = a.table_info();
    let table_end = t.abs_offset + 8 + u64::from(t.zsize);
    let mut unreadable = HashMap::new();
    // (start, end) -> first entry index, to find shared data blocks and overlaps
    let mut spans: Vec<(u64, u64, usize)> = Vec::new();

    for (i, e) in a.entries().iter().enumerate() {
        if e.zsize_aligned != e.zsize.div_ceil(8).saturating_mul(8) {
            rep.issues.push(Issue {
                severity: Severity::Warning,
                kind: IssueKind::Alignment,
                entry: Some(i),
                message: format!(
                    "aligned size {} is not {} rounded up to 8",
                    e.zsize_aligned, e.zsize
                ),
            });
        }
        if e.zsize == 0 {
            continue;
        }
        let (start, end) = (e.abs_offset(), e.abs_offset() + u64::from(e.zsize));
        if end > file_len {
            unreadable.insert(i, ());
            rep.issues.push(Issue {
                severity: Severity::Error,
                kind: IssueKind::OutOfRange,
                entry: Some(i),
                message: format!(
                    "data at {start}..{end} lies beyond the end of the file ({file_len})"
                ),
            });
            continue;
        }
        if start < table_end && end > t.abs_offset {
            rep.issues.push(Issue {
                severity: Severity::Error,
                kind: IssueKind::Overlap,
                entry: Some(i),
                message: "data overlaps the file table".into(),
            });
        }
        spans.push((start, end, i));
    }

    spans.sort_unstable();
    let (mut shared, mut prev) = (0usize, None::<(u64, u64, usize)>);
    for s in spans {
        match prev {
            Some((ps, pe, _)) if (s.0, s.1) == (ps, pe) => shared += 1,
            Some((_, pe, pi)) if s.0 < pe => {
                rep.issues.push(Issue {
                    severity: Severity::Error,
                    kind: IssueKind::Overlap,
                    entry: Some(s.2),
                    message: format!("data partially overlaps entry {pi}"),
                });
                prev = Some((s.0, s.1.max(pe), pi));
            }
            _ => prev = Some(s),
        }
    }
    if shared > 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::SharedData,
            entry: None,
            message: format!(
                "{shared} entries share their data block with another entry (compacted archive)"
            ),
        });
    }

    let mut seen: HashMap<Vec<u8>, usize> = HashMap::with_capacity(a.len());
    for (i, e) in a.entries().iter().enumerate() {
        if let Some(&first) = seen.get(&e.name.key()) {
            rep.issues.push(Issue {
                severity: Severity::Warning,
                kind: IssueKind::DuplicatePath,
                entry: Some(i),
                message: format!("same path as entry {first} (compared case-insensitively)"),
            });
        } else {
            seen.insert(e.name.key(), i);
        }
    }
    unreadable
}

impl From<Severity> for &'static str {
    fn from(s: Severity) -> Self {
        match s {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}
