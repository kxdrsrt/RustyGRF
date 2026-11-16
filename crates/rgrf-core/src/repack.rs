//! `repack`: read an archive and write a new one with another compression method, format,
//! alignment and/or deduplication, without changing any file's content.
//!
//! Guarantees: the entry order and names (bytes) stay as in the source; every file decodes to the
//! same bytes (checked after writing unless disabled); encrypted and damaged entries are copied
//! unchanged; the output is written to a temporary file and renamed only after the check passed.

use rayon::prelude::*;
use sha2::{Digest as _, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::archive::Archive;
use crate::codec::{self, DecodeStatus};
use crate::container::{Entry, flags};
use crate::encode::{Method, encode};
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::progress::Control;
use crate::writer::{Format, OutEntry, Placed, Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dedupe {
    /// Every entry gets its own data block.
    None,
    /// Entries that share a data block in the source keep sharing it (what a compacted archive has).
    #[default]
    Source,
    /// Entries with identical decoded content share one block, also across different source blocks.
    Content,
}

#[derive(Debug, Clone)]
pub struct RepackOptions {
    pub method: Method,
    /// `None` keeps the version of the source.
    pub format: Option<Format>,
    /// `None` uses the format default (see [`Format::aligns_by_default`]).
    pub align: Option<bool>,
    pub dedupe: Dedupe,
    /// Always use the freshly encoded stream. By default the original stream is kept when it uses
    /// the same codec and is not larger.
    pub recompress_all: bool,
    /// Decode the written archive and compare every entry with the source (strongly recommended).
    pub verify: bool,
    pub overwrite: bool,
    /// Only plan: count blocks and bytes, write nothing.
    pub dry_run: bool,
    /// Override of the format's size limit (tests).
    pub max_file_len: Option<u64>,
}

impl Default for RepackOptions {
    fn default() -> Self {
        Self {
            method: Method::DEFAULT,
            format: None,
            align: None,
            dedupe: Dedupe::default(),
            recompress_all: false,
            verify: true,
            overwrite: false,
            dry_run: false,
            max_file_len: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RepackReport {
    pub format: u32,
    pub entries: usize,
    pub markers: usize,
    pub files: usize,
    /// Distinct data blocks written (files that share a block count once).
    pub blocks: usize,
    /// Files that share their block with an earlier file.
    pub shared_files: usize,
    /// Blocks copied unchanged because they are encrypted.
    pub encrypted_blocks: usize,
    /// Blocks copied unchanged because the source stores them uncompressed (flags 0 with data).
    pub stored_blocks: usize,
    /// Blocks copied unchanged because they are damaged (indices of their first entry).
    pub damaged: Vec<usize>,
    /// Blocks whose original stream was kept (same codec, not larger).
    pub kept_original: usize,
    pub recompressed: usize,
    pub source_file_len: u64,
    /// Sum of the stored sizes of the source blocks that are written (without sharing).
    pub source_stream_bytes: u64,
    pub output_file_len: u64,
    pub output_stream_bytes: u64,
    pub verified: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SourceKey {
    offset: u64,
    zsize: u32,
    size: u32,
    flags: u8,
}

fn source_key(e: &Entry) -> SourceKey {
    SourceKey {
        offset: e.offset,
        zsize: e.zsize,
        size: e.size,
        flags: e.flags,
    }
}

#[derive(Debug)]
pub(crate) struct Unit {
    /// Entry indices, in source order; the first one is the representative.
    pub(crate) members: Vec<usize>,
    /// Hash of the decoded content if known (content dedupe or verification).
    hash: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expected {
    /// Decodes to data with this hash and status class.
    Data([u8; 32], u8),
    /// Encrypted: the stored bytes (with padding) must be identical.
    Raw([u8; 32]),
    Nothing,
}

/// How a block ends up in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Encoded,
    KeptOriginal,
    Encrypted,
    Damaged,
    Stored,
}

pub(crate) struct Prepared {
    pub(crate) unit: usize,
    kind: Kind,
    stream: Vec<u8>,
    /// `Some((zsize, aligned))` for blocks copied unchanged.
    raw: Option<(u32, u32)>,
    flags: u8,
    pub(crate) expected: Expected,
}

#[derive(Default)]
struct Stats {
    kept: usize,
    recompressed: usize,
    encrypted: usize,
    stored: usize,
    stream_bytes: u64,
    damaged: Vec<usize>,
}

/// What the writer thread reports per written block: unit, position, flags, expectation.
type Written = (usize, Placed, u8, Expected);

fn sha(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

fn status_class(s: &DecodeStatus) -> u8 {
    match s {
        DecodeStatus::Ok => 0,
        DecodeStatus::Truncated { .. } => 1,
        DecodeStatus::SizeMismatch { .. } => 2,
        DecodeStatus::Corrupt(_) => 3,
        DecodeStatus::Encrypted => 4,
    }
}

pub(crate) fn plan_units<R: ReadAt>(
    src: &Archive<R>,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<Vec<Unit>> {
    let mut units: Vec<Unit> = Vec::new();
    let mut by_source: HashMap<SourceKey, usize> = HashMap::new();
    for (i, e) in src.entries().iter().enumerate() {
        if e.is_marker() {
            continue;
        }
        if opts.dedupe == Dedupe::None {
            units.push(Unit {
                members: vec![i],
                hash: None,
            });
            continue;
        }
        let u = *by_source.entry(source_key(e)).or_insert_with(|| {
            units.push(Unit {
                members: Vec::new(),
                hash: None,
            });
            units.len() - 1
        });
        units[u].members.push(i);
    }

    if opts.dedupe == Dedupe::Content {
        // hash every distinct source block once, then merge blocks with identical content
        let hashes: Vec<Result<Option<[u8; 32]>>> = units
            .par_iter()
            .map(|u| {
                ctl.check()?;
                let rep = u.members[0];
                if src.entry(rep)?.is_encrypted() {
                    return Ok(None);
                }
                let d = src.read(rep)?;
                Ok(d.status.is_ok().then(|| sha(&d.data)))
            })
            .collect();
        let mut merged: Vec<Unit> = Vec::with_capacity(units.len());
        let mut seen: HashMap<([u8; 32], u32, bool), usize> = HashMap::new();
        for (mut u, h) in units.into_iter().zip(hashes) {
            u.hash = h?;
            let (size, raw) = {
                let e = src.entry(u.members[0])?;
                (e.size, e.is_raw())
            };
            match u.hash {
                Some(h) => match seen.get(&(h, size, raw)) {
                    Some(&into) => merged[into].members.extend(u.members),
                    None => {
                        seen.insert((h, size, raw), merged.len());
                        merged.push(u);
                    }
                },
                None => merged.push(u),
            }
        }
        for u in &mut merged {
            u.members.sort_unstable();
        }
        return Ok(merged);
    }
    Ok(units)
}

pub(crate) fn prepare<R: ReadAt>(
    src: &Archive<R>,
    unit_id: usize,
    unit: &Unit,
    opts: &RepackOptions,
) -> Result<Prepared> {
    let rep = unit.members[0];
    let e = src.entry(rep)?;
    if e.is_encrypted() {
        let raw = src.read_raw_aligned(rep)?;
        let expected = if opts.verify {
            Expected::Raw(sha(&raw))
        } else {
            Expected::Nothing
        };
        return Ok(Prepared {
            unit: unit_id,
            kind: Kind::Encrypted,
            stream: raw,
            raw: Some((e.zsize, e.zsize_aligned)),
            flags: e.flags,
            expected,
        });
    }
    let d = src.read(rep)?;
    let class = status_class(&d.status);
    let expected = |data: &[u8]| {
        if !opts.verify {
            Expected::Nothing
        } else {
            Expected::Data(unit.hash.unwrap_or_else(|| sha(data)), class)
        }
    };
    if !d.status.is_ok() || e.is_raw() {
        // Damaged: cannot be re-encoded faithfully, keep the stored bytes (and what the client
        // would see). Stored uncompressed (flags 0): keep as the official archive has it.
        let raw = src.read_raw_aligned(rep)?;
        return Ok(Prepared {
            unit: unit_id,
            kind: if d.status.is_ok() {
                Kind::Stored
            } else {
                Kind::Damaged
            },
            expected: expected(&d.data),
            stream: raw,
            raw: Some((e.zsize, e.zsize_aligned)),
            flags: e.flags,
        });
    }
    let mut stream = encode(opts.method, &d.data);
    let mut kept = false;
    if !opts.recompress_all {
        let original = src.read_raw(rep)?;
        if codec::sniff(&original) == Some(opts.method.codec()) && original.len() <= stream.len() {
            stream = original;
            kept = true;
        }
    }
    Ok(Prepared {
        unit: unit_id,
        kind: if kept {
            Kind::KeptOriginal
        } else {
            Kind::Encoded
        },
        expected: expected(&d.data),
        stream,
        raw: None,
        flags: flags::FILE,
    })
}

/// How much decoded data one batch may hold in memory (all worker threads together).
const BATCH_BYTES: u64 = 256 << 20;
const BATCH_UNITS: usize = 4096;

fn batches<R: ReadAt>(src: &Archive<R>, units: &[Unit]) -> Vec<std::ops::Range<usize>> {
    let (mut out, mut start, mut bytes) = (Vec::new(), 0usize, 0u64);
    for (i, u) in units.iter().enumerate() {
        let size = src
            .entry(u.members[0])
            .map_or(0, |e| u64::from(e.size).max(u64::from(e.zsize)));
        if i > start && (bytes + size > BATCH_BYTES || i - start >= BATCH_UNITS) {
            out.push(start..i);
            start = i;
            bytes = 0;
        }
        bytes += size;
    }
    if start < units.len() {
        out.push(start..units.len());
    }
    out
}

fn temp_path(dst: &Path) -> PathBuf {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dst.with_file_name(format!(".{name}.rgrf-tmp"))
}

pub fn repack<R: ReadAt>(
    src: &Archive<R>,
    dst: &Path,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<RepackReport> {
    if src.table_info().key_block > 0 {
        return Err(Error::Unsupported(
            "the source has a protected-archive key block (not supported yet)".into(),
        ));
    }
    let format = opts.format.unwrap_or(if src.header().version == 0x300 {
        Format::V300
    } else {
        Format::V200
    });
    let align = opts.align.unwrap_or_else(|| format.aligns_by_default());
    if !opts.dry_run && dst.exists() && !opts.overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }

    let entries = src.entries();
    let units = plan_units(src, opts, ctl)?;
    let mut report = RepackReport {
        format: format.version(),
        entries: entries.len(),
        markers: entries.iter().filter(|e| e.is_marker()).count(),
        source_file_len: src.file_len(),
        dry_run: opts.dry_run,
        ..RepackReport::default()
    };
    report.files = report.entries - report.markers;
    report.blocks = units.len();
    report.shared_files = report.files - units.len();
    report.source_stream_bytes = units
        .iter()
        .map(|u| u64::from(entries[u.members[0]].zsize))
        .sum();
    if opts.dry_run {
        return Ok(report);
    }

    let tmp = temp_path(dst);
    let result = write_all(src, &units, &tmp, format, align, opts, ctl, &mut report);
    match result {
        Ok(()) => {
            if opts.overwrite && dst.exists() {
                fs::remove_file(dst)?;
            }
            fs::rename(&tmp, dst)?;
            Ok(report)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn write_all<R: ReadAt>(
    src: &Archive<R>,
    units: &[Unit],
    tmp: &Path,
    format: Format,
    align: bool,
    opts: &RepackOptions,
    ctl: &Control,
    report: &mut RepackReport,
) -> Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let mut writer = Writer::new(
        BufWriter::with_capacity(1 << 20, file),
        format,
        align,
        opts.max_file_len,
    )?;
    let mut placed: Vec<Option<Placed>> = vec![None; units.len()];
    let mut out_flags: Vec<u8> = vec![flags::FILE; units.len()];
    let mut expected: Vec<Expected> = vec![Expected::Nothing; units.len()];
    let ranges = batches(src, units);
    let total = units.len() as u64;

    let (tx, rx) = mpsc::sync_channel::<Vec<Prepared>>(1);
    let mut stats = Stats::default();

    let mut produced: Result<()> = Ok(());
    let written: Result<(Writer<BufWriter<File>>, Vec<Written>)> = std::thread::scope(|scope| {
        let handle = scope.spawn(move || {
            let mut done = Vec::new();
            for batch in rx {
                for p in batch {
                    let pl = match p.raw {
                        Some((zsize, aligned)) => writer.put_raw(&p.stream, zsize, aligned)?,
                        None => writer.put_stream(&p.stream)?,
                    };
                    done.push((p.unit, pl, p.flags, p.expected));
                }
            }
            Ok((writer, done))
        });
        for range in ranges {
            if let Err(e) = ctl.check() {
                produced = Err(e);
                break;
            }
            let batch: Result<Vec<Prepared>> = range
                .clone()
                .into_par_iter()
                .map(|u| prepare(src, u, &units[u], opts))
                .collect();
            match batch {
                Ok(batch) => {
                    for p in &batch {
                        match p.kind {
                            Kind::Encoded => stats.recompressed += 1,
                            Kind::KeptOriginal => stats.kept += 1,
                            Kind::Encrypted => stats.encrypted += 1,
                            Kind::Stored => stats.stored += 1,
                            Kind::Damaged => stats.damaged.push(units[p.unit].members[0]),
                        }
                        stats.stream_bytes += p.stream.len() as u64;
                    }
                    if tx.send(batch).is_err() {
                        break; // the writer stopped; its error is returned below
                    }
                    ctl.report(range.end as u64, total);
                }
                Err(e) => {
                    produced = Err(e);
                    break;
                }
            }
        }
        drop(tx);
        handle.join().expect("writer thread panicked")
    });
    let (writer, done) = match written {
        Ok(w) => w,
        Err(e) => return Err(produced.err().unwrap_or(e)),
    };
    produced?;
    for (unit, pl, flags, exp) in done {
        placed[unit] = Some(pl);
        out_flags[unit] = flags;
        expected[unit] = exp;
    }

    // file table in source order
    let mut unit_of: Vec<Option<usize>> = vec![None; src.len()];
    for (u, unit) in units.iter().enumerate() {
        for &m in &unit.members {
            unit_of[m] = Some(u);
        }
    }
    let marker = Placed {
        offset: 0,
        zsize: 0,
        aligned: 0,
    };
    let out_entries: Vec<OutEntry> = src
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| match unit_of[i] {
            Some(u) => OutEntry {
                name: e.name.clone(),
                placed: placed[u].expect("every unit was written"),
                size: e.size,
                flags: out_flags[u],
            },
            None => OutEntry {
                name: e.name.clone(),
                placed: marker,
                size: 0,
                flags: 0,
            },
        })
        .collect();
    let (len, mut out) = writer.finish(&out_entries)?;
    std::io::Write::flush(&mut out)?;
    drop(out);

    report.kept_original = stats.kept;
    report.recompressed = stats.recompressed;
    report.encrypted_blocks = stats.encrypted;
    report.stored_blocks = stats.stored;
    report.output_stream_bytes = stats.stream_bytes;
    report.damaged = stats.damaged;
    report.output_file_len = len;

    if opts.verify {
        verify_output(src, units, &unit_of, &expected, tmp, ctl)?;
        report.verified = true;
    }
    Ok(())
}

/// Opens the written file and checks it against the source: same entries, same names, every block
/// decodes to the expected content (or has identical stored bytes if encrypted).
pub(crate) fn verify_output<R: ReadAt>(
    src: &Archive<R>,
    units: &[Unit],
    unit_of: &[Option<usize>],
    expected: &[Expected],
    path: &Path,
    ctl: &Control,
) -> Result<()> {
    let fail = |m: String| Err(Error::VerifyFailed(m));
    let out = Archive::open(path)?;
    if out.len() != src.len() {
        return fail(format!(
            "{} entries written, {} expected",
            out.len(),
            src.len()
        ));
    }
    for (i, (a, b)) in src.entries().iter().zip(out.entries()).enumerate() {
        if a.name != b.name
            || a.size != b.size && !a.is_marker()
            || a.is_marker() != b.is_marker()
            || a.is_raw() != b.is_raw()
        {
            return fail(format!("entry {i} differs in name, size or kind"));
        }
        if let Some(u) = unit_of[i] {
            let rep = units[u].members[0];
            if rep != i
                && (b.offset != out.entries()[rep].offset || b.zsize != out.entries()[rep].zsize)
            {
                return fail(format!("entry {i} should share the block of entry {rep}"));
            }
        }
    }
    let problems: Vec<Result<Option<String>>> = (0..units.len())
        .into_par_iter()
        .map(|u| {
            ctl.check()?;
            let rep = units[u].members[0];
            Ok(match &expected[u] {
                Expected::Nothing => None,
                Expected::Raw(h) => (sha(&out.read_raw_aligned(rep)?) != *h)
                    .then(|| format!("entry {rep}: stored bytes of an encrypted entry changed")),
                Expected::Data(h, class) => {
                    let d = out.read(rep)?;
                    (status_class(&d.status) != *class || sha(&d.data) != *h).then(|| {
                        format!("entry {rep}: content differs after repack ({:?})", d.status)
                    })
                }
            })
        })
        .collect();
    for p in problems {
        if let Some(msg) = p? {
            return fail(msg);
        }
    }
    Ok(())
}
