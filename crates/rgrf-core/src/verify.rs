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
    /// Delete directives of a patch (Thor, GPF): nothing to extract.
    Removals,
    RawEntries,
    SharedData,
    OutOfRange,
    Overlap,
    DuplicatePath,
    Alignment,
    Truncated,
    SizeMismatch,
    Corrupt,
    Encrypted,
    KeyBlock,
    /// A Thor patch's `data.integrity` list disagrees with the stored data.
    Integrity,
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
    /// Files stored uncompressed (flags 0 with data); they are counted in `files`.
    pub raw: usize,
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
    let removals = a.entries().iter().filter(|e| e.is_removal()).count();
    rep.markers = a.entries().iter().filter(|e| e.is_marker()).count() - removals;
    rep.files = rep.entries - rep.markers - removals;
    rep.raw = a.entries().iter().filter(|e| e.is_raw()).count();
    if rep.raw > 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::RawEntries,
            entry: None,
            message: format!(
                "{} files are stored uncompressed (flags 0, as in the official archive)",
                rep.raw
            ),
        });
    }
    if removals > 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::Removals,
            entry: None,
            message: format!("{removals} delete directives (patch), nothing to extract"),
        });
    }
    if rep.markers > 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::Markers,
            entry: None,
            message: format!(
                "{} empty marker or folder entries (no file flag), not extracted",
                rep.markers
            ),
        });
    }

    if a.table_info().key_block > 0 {
        let protected = a.entries().iter().filter(|e| e.is_protected()).count();
        let (severity, message) = match a.protected_keys() {
            Ok(n) => (
                Severity::Info,
                format!(
                    "protected archive: key block of {} bytes with {n} archive keys, {protected} protected entries",
                    a.table_info().key_block
                ),
            ),
            // only a problem when entries need it
            Err(e) => (
                if protected > 0 {
                    Severity::Warning
                } else {
                    Severity::Info
                },
                format!(
                    "protected archive: key block of {} bytes cannot be used ({e}), {protected} protected entries",
                    a.table_info().key_block
                ),
            ),
        };
        rep.issues.push(Issue {
            severity,
            kind: IssueKind::KeyBlock,
            entry: None,
            message,
        });
    }

    let unreadable = check_structure(a, &mut rep);
    if opts.decode && a.header().kind == crate::container::Kind::Thor {
        check_thor_integrity(a, &unreadable, &mut rep);
    }

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

/// A Thor patch lists `name=0xCRC32` for its files in `data.integrity`, the CRC32 of the file
/// content (measured on third-party patches). A list that is missing or in another shape is not an
/// error.
fn check_thor_integrity<R: ReadAt>(
    a: &Archive<R>,
    unreadable: &HashMap<usize, ()>,
    rep: &mut VerifyReport,
) {
    let key = crate::name::GrfPath::new(crate::thor::INTEGRITY_FILE.as_bytes()).key();
    let Some(list) = (0..a.len()).find(|&i| a.entries()[i].name.key() == key) else {
        return;
    };
    let Ok(text) = a.read(list).map(|d| d.data) else {
        return;
    };
    let mut expected: std::collections::HashMap<Vec<u8>, u32> = std::collections::HashMap::new();
    for line in text.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let Some(at) = line.iter().rposition(|&b| b == b'=') else {
            continue;
        };
        let hex = line[at + 1..]
            .strip_prefix(b"0x")
            .unwrap_or(&line[at + 1..]);
        if let Some(crc) = std::str::from_utf8(hex)
            .ok()
            .and_then(|h| u32::from_str_radix(h.trim(), 16).ok())
        {
            expected.insert(crate::name::GrfPath::new(&line[..at]).key(), crc);
        }
    }
    let (mut checked, mut bad) = (0usize, 0usize);
    for i in 0..a.len() {
        let e = &a.entries()[i];
        let Some(&want) = expected.get(&e.name.key()) else {
            continue;
        };
        if i == list || e.is_removal() || unreadable.contains_key(&i) {
            continue;
        }
        let Ok(d) = a.read(i) else { continue };
        if !d.status.is_ok() {
            continue;
        }
        checked += 1;
        if crc32fast::hash(&d.data) != want {
            bad += 1;
            rep.issues.push(Issue {
                severity: Severity::Warning,
                kind: IssueKind::Integrity,
                entry: Some(i),
                message: format!("data.integrity says CRC32 {want:08x}, the content has another"),
            });
        }
    }
    if checked > 0 && bad == 0 {
        rep.issues.push(Issue {
            severity: Severity::Info,
            kind: IssueKind::Integrity,
            entry: None,
            message: format!("data.integrity: CRC32 of {checked} files agree"),
        });
    }
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
            "encrypted entry (needs a key), content not checked".to_owned(),
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
    let table_end = t.end();
    let mut unreadable = HashMap::new();
    // (start, end) -> first entry index, to find shared data blocks and overlaps
    let mut spans: Vec<(u64, u64, usize)> = Vec::new();

    for (i, e) in a.entries().iter().enumerate() {
        if e.is_folder() {
            continue;
        }
        // Three forms occur in real archives: rounded up to 8 (official 0x200, GRF Editor with
        // alignment), equal to `zsize` (kRO 0x300, no alignment) and rounded up to 4 (community
        // archives, measured: 4,649 entries in kdom.grf, 31 in woero.grf, 2 in dark2.grf; all
        // decode). DES-scrambled entries are read in blocks of 8, so for them 4 is wrong.
        let rounded = e.zsize.div_ceil(8).saturating_mul(8);
        let rounded4 = e.zsize.div_ceil(4).saturating_mul(4);
        if e.zsize_aligned != e.zsize
            && e.zsize_aligned != rounded
            && (e.zsize_aligned != rounded4 || e.is_des())
        {
            rep.issues.push(Issue {
                severity: Severity::Warning,
                kind: IssueKind::Alignment,
                entry: Some(i),
                message: format!(
                    "aligned size {} is neither {} nor {} rounded up to 8{}",
                    e.zsize_aligned,
                    e.zsize,
                    e.zsize,
                    if e.is_des() { "" } else { " or 4" }
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
