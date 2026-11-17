//! `repack`: read an archive and write a new one with another compression method, format,
//! alignment and/or deduplication, without changing any file's content.
//!
//! Guarantees: the entry order and names (bytes) stay as in the source; every file decodes to the
//! same bytes (checked after writing unless disabled); encrypted and damaged entries are copied
//! unchanged; the output is written to a temporary file and renamed only after the check passed.

use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::BufWriter;
use std::path::Path;

use crate::archive::Archive;
use crate::codec;
use crate::container::{Entry, flags};
use crate::encode::{Method, encode};
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::pipeline::{self, Expected, Kind, Prepared, sha, status_class};
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
    src: usize,
    offset: u64,
    zsize: u32,
    size: u32,
    flags: u8,
}

fn source_key(src: usize, e: &Entry) -> SourceKey {
    SourceKey {
        src,
        offset: e.offset,
        zsize: e.zsize,
        size: e.size,
        flags: e.flags,
    }
}

/// One output entry: entry `idx` of source archive `src`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Sel {
    pub(crate) src: usize,
    pub(crate) idx: usize,
}

/// The entries of the output archive, in output order, each taken from one of the sources.
/// `repack` selects every entry of one archive; `merge`/`patch` select from several.
pub(crate) struct Selection<'a, R: ReadAt> {
    pub(crate) sources: &'a [&'a Archive<R>],
    pub(crate) sel: Vec<Sel>,
}

impl<R: ReadAt> Selection<'_, R> {
    pub(crate) fn len(&self) -> usize {
        self.sel.len()
    }

    fn archive(&self, i: usize) -> &Archive<R> {
        self.sources[self.sel[i].src]
    }

    pub(crate) fn entry(&self, i: usize) -> Result<&Entry> {
        self.archive(i).entry(self.sel[i].idx)
    }

    fn read(&self, i: usize) -> Result<codec::Decoded> {
        self.archive(i).read(self.sel[i].idx)
    }

    fn read_raw(&self, i: usize) -> Result<Vec<u8>> {
        self.archive(i).read_raw(self.sel[i].idx)
    }

    fn read_raw_aligned(&self, i: usize) -> Result<Vec<u8>> {
        self.archive(i).read_raw_aligned(self.sel[i].idx)
    }
}

#[derive(Debug)]
pub(crate) struct Unit {
    /// Output entry indices, in output order; the first one is the representative.
    pub(crate) members: Vec<usize>,
    /// Hash of the decoded content if known (content dedupe or verification).
    hash: Option<[u8; 32]>,
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

pub(crate) fn plan_units<R: ReadAt>(
    s: &Selection<R>,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<Vec<Unit>> {
    let mut units: Vec<Unit> = Vec::new();
    let mut by_source: HashMap<SourceKey, usize> = HashMap::new();
    for i in 0..s.len() {
        let e = s.entry(i)?;
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
        let u = *by_source
            .entry(source_key(s.sel[i].src, e))
            .or_insert_with(|| {
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
                if s.entry(rep)?.is_encrypted() {
                    return Ok(None);
                }
                let d = s.read(rep)?;
                Ok(d.status.is_ok().then(|| sha(&d.data)))
            })
            .collect();
        let mut merged: Vec<Unit> = Vec::with_capacity(units.len());
        let mut seen: HashMap<([u8; 32], u32, bool), usize> = HashMap::new();
        for (mut u, h) in units.into_iter().zip(hashes) {
            u.hash = h?;
            let (size, raw) = {
                let e = s.entry(u.members[0])?;
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
    s: &Selection<R>,
    unit_id: usize,
    unit: &Unit,
    opts: &RepackOptions,
) -> Result<Prepared> {
    let rep = unit.members[0];
    let e = s.entry(rep)?;
    if e.is_encrypted() {
        let raw = s.read_raw_aligned(rep)?;
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
    let d = s.read(rep)?;
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
        let raw = s.read_raw_aligned(rep)?;
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
        let original = s.read_raw(rep)?;
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

pub fn repack<R: ReadAt>(
    src: &Archive<R>,
    dst: &Path,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<RepackReport> {
    let sources = [src];
    let s = Selection {
        sources: &sources,
        sel: (0..src.len()).map(|idx| Sel { src: 0, idx }).collect(),
    };
    let format = opts.format.unwrap_or(if src.header().version == 0x300 {
        Format::V300
    } else {
        Format::V200
    });
    write_selection(&s, dst, format, opts, ctl)
}

/// Plans and writes the archive for a selection (shared by `repack`, `merge` and `patch`).
pub(crate) fn write_selection<R: ReadAt>(
    s: &Selection<R>,
    dst: &Path,
    format: Format,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<RepackReport> {
    if s.sources.iter().any(|a| a.table_info().key_block > 0) {
        return Err(Error::Unsupported(
            "a source has a protected-archive key block (not supported yet)".into(),
        ));
    }
    let align = opts.align.unwrap_or_else(|| format.aligns_by_default());
    if !opts.dry_run && dst.exists() && !opts.overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }

    let units = plan_units(s, opts, ctl)?;
    let mut markers = 0;
    for i in 0..s.len() {
        markers += usize::from(s.entry(i)?.is_marker());
    }
    let mut report = RepackReport {
        format: format.version(),
        entries: s.len(),
        markers,
        source_file_len: s.sources.iter().map(|a| a.file_len()).sum(),
        dry_run: opts.dry_run,
        ..RepackReport::default()
    };
    report.files = report.entries - report.markers;
    report.blocks = units.len();
    report.shared_files = report.files - units.len();
    for u in &units {
        report.source_stream_bytes += u64::from(s.entry(u.members[0])?.zsize);
    }
    if opts.dry_run {
        return Ok(report);
    }

    let tmp = pipeline::temp_path(dst);
    let result = write_all(s, &units, &tmp, format, align, opts, ctl, &mut report);
    match result {
        Ok(()) => {
            pipeline::commit(&tmp, dst, opts.overwrite)?;
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
    s: &Selection<R>,
    units: &[Unit],
    tmp: &Path,
    format: Format,
    align: bool,
    opts: &RepackOptions,
    ctl: &Control,
    report: &mut RepackReport,
) -> Result<()> {
    let file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let writer = Writer::new(
        BufWriter::with_capacity(1 << 20, file),
        format,
        align,
        opts.max_file_len,
    )?;
    let mut placed: Vec<Option<Placed>> = vec![None; units.len()];
    let mut out_flags: Vec<u8> = vec![flags::FILE; units.len()];
    let mut expected: Vec<Expected> = vec![Expected::Nothing; units.len()];
    let total = units.len() as u64;

    let mut stats = Stats::default();
    let (writer, done) = pipeline::run(
        writer,
        pipeline::batches(units.iter().map(|u| {
            s.entry(u.members[0])
                .map_or(0, |e| u64::from(e.size).max(u64::from(e.zsize)))
        })),
        total,
        ctl,
        &|u| prepare(s, u, &units[u], opts),
        &mut |batch| {
            for p in batch {
                match p.kind {
                    Kind::Encoded => stats.recompressed += 1,
                    Kind::KeptOriginal => stats.kept += 1,
                    Kind::Encrypted => stats.encrypted += 1,
                    Kind::Stored => stats.stored += 1,
                    Kind::Damaged => stats.damaged.push(units[p.unit].members[0]),
                }
                stats.stream_bytes += p.stream.len() as u64;
            }
        },
    )?;
    for (unit, pl, flags, exp) in done {
        placed[unit] = Some(pl);
        out_flags[unit] = flags;
        expected[unit] = exp;
    }

    // file table in output order
    let mut unit_of: Vec<Option<usize>> = vec![None; s.len()];
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
    let mut out_entries: Vec<OutEntry> = Vec::with_capacity(s.len());
    for (i, unit) in unit_of.iter().enumerate() {
        let e = s.entry(i)?;
        out_entries.push(match *unit {
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
        });
    }
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
        verify_output(s, units, &unit_of, &expected, tmp, ctl)?;
        report.verified = true;
    }
    Ok(())
}

/// Opens the written file and checks it against the selection: same entries, same names, every
/// block decodes to the expected content (or has identical stored bytes if encrypted).
pub(crate) fn verify_output<R: ReadAt>(
    s: &Selection<R>,
    units: &[Unit],
    unit_of: &[Option<usize>],
    expected: &[Expected],
    path: &Path,
    ctl: &Control,
) -> Result<()> {
    let fail = |m: String| Err(Error::VerifyFailed(m));
    let out = Archive::open(path)?;
    if out.len() != s.len() {
        return fail(format!(
            "{} entries written, {} expected",
            out.len(),
            s.len()
        ));
    }
    for (i, b) in out.entries().iter().enumerate() {
        let a = s.entry(i)?;
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
