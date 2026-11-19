//! `diff`, `merge` and `patch` for archives.
//!
//! Paths are compared by [`GrfPath::key`] (ASCII case-insensitive, `/` = `\`), as the client does.
//! Inside one archive a later entry with the same path wins (the same rule `extract` uses).
//! Priority between archives follows `DATA.INI`: the **first** archive given wins.
//! Empty markers (flags 0, no data) are never copied; they are counted in the report.

use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::archive::Archive;
use crate::error::{Error, Result};
use crate::io::ReadAt;
use crate::name::GrfPath;
use crate::pipeline::sha;
use crate::progress::Control;
use crate::repack::{RepackOptions, RepackReport, Sel, Selection, write_selection};
use crate::writer::Format;

/// Last entry per path key; entries are returned in archive order of their last occurrence.
fn last_by_key<R: ReadAt>(a: &Archive<R>) -> (Vec<usize>, usize) {
    let mut at: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut duplicates = 0;
    for (i, e) in a.entries().iter().enumerate() {
        match at.get(&e.name.key()) {
            Some(&pos) => {
                order[pos] = i;
                duplicates += 1;
            }
            None => {
                at.insert(e.name.key(), order.len());
                order.push(i);
            }
        }
    }
    (order, duplicates)
}

/// SHA-256 of what the client would read; of the stored bytes for entries that need a key.
fn fingerprint<R: ReadAt>(a: &Archive<R>, i: usize) -> Result<[u8; 32]> {
    let e = a.entry(i)?;
    if e.needs_key() {
        return Ok(sha(&a.read_raw_aligned(i)?));
    }
    let d = a.read(i)?;
    // a protected entry that cannot be decoded is compared by its stored bytes
    if e.is_protected() && !d.status.is_ok() {
        return Ok(sha(&a.read_raw_aligned(i)?));
    }
    Ok(sha(&d.data))
}

#[derive(Debug, Clone, Default)]
pub struct DiffReport {
    /// Paths only in the first archive.
    pub only_a: Vec<GrfPath>,
    /// Paths only in the second archive.
    pub only_b: Vec<GrfPath>,
    /// Paths in both with different content.
    pub changed: Vec<GrfPath>,
    pub same: usize,
    /// Entries ignored because a later entry of the same archive has the same path.
    pub shadowed_a: usize,
    pub shadowed_b: usize,
}

impl DiffReport {
    pub fn is_identical(&self) -> bool {
        self.only_a.is_empty() && self.only_b.is_empty() && self.changed.is_empty()
    }
}

struct Pair {
    a: Option<usize>,
    b: Option<usize>,
}

fn pair_up<R: ReadAt>(a: &Archive<R>, b: &Archive<R>) -> (Vec<Pair>, usize, usize) {
    let (ia, sa) = last_by_key(a);
    let (ib, sb) = last_by_key(b);
    let mut by_key: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut pairs: Vec<Pair> = Vec::new();
    for &i in &ia {
        by_key.insert(a.entries()[i].name.key(), pairs.len());
        pairs.push(Pair {
            a: Some(i),
            b: None,
        });
    }
    for &i in &ib {
        match by_key.get(&b.entries()[i].name.key()) {
            Some(&p) => pairs[p].b = Some(i),
            None => pairs.push(Pair {
                a: None,
                b: Some(i),
            }),
        }
    }
    (pairs, sa, sb)
}

/// Compares two archives by path and decoded content.
pub fn diff<R: ReadAt>(a: &Archive<R>, b: &Archive<R>, ctl: &Control) -> Result<DiffReport> {
    let (pairs, shadowed_a, shadowed_b) = pair_up(a, b);
    // true = different
    let verdicts: Vec<Result<bool>> = pairs
        .par_iter()
        .map(|p| {
            ctl.check()?;
            let (Some(i), Some(j)) = (p.a, p.b) else {
                return Ok(false);
            };
            let (ea, eb) = (a.entry(i)?, b.entry(j)?);
            if ea.is_marker() || eb.is_marker() {
                return Ok(ea.is_marker() != eb.is_marker());
            }
            if ea.size != eb.size {
                return Ok(true);
            }
            Ok(fingerprint(a, i)? != fingerprint(b, j)?)
        })
        .collect();
    let mut r = DiffReport {
        shadowed_a,
        shadowed_b,
        ..DiffReport::default()
    };
    for (p, v) in pairs.iter().zip(verdicts) {
        let differs = v?;
        match (p.a, p.b) {
            (Some(i), None) => r.only_a.push(a.entries()[i].name.clone()),
            (None, Some(j)) => r.only_b.push(b.entries()[j].name.clone()),
            (Some(i), Some(_)) if differs => r.changed.push(a.entries()[i].name.clone()),
            _ => r.same += 1,
        }
    }
    for v in [&mut r.only_a, &mut r.only_b, &mut r.changed] {
        v.sort_by_key(GrfPath::key);
    }
    Ok(r)
}

#[derive(Debug, Clone, Default)]
pub struct SourceStats {
    /// Files (without markers).
    pub files: usize,
    /// Files that ended up in the output.
    pub used: usize,
    /// Files replaced by an archive with higher priority.
    pub overridden: usize,
    /// Files replaced by a later entry of the same archive.
    pub shadowed: usize,
    pub markers_skipped: usize,
    /// Files not taken because `replace_only` is set and the base archive does not have the path.
    pub ignored_new: usize,
    /// Files left out because their path is in the exclude set of [`merge_excluding`].
    pub excluded: usize,
    /// Delete directives of a patch (Thor, GPF) that this archive carries.
    pub removals: usize,
    /// Paths from lower-priority archives that those directives removed from the output.
    pub removed: usize,
}

#[derive(Debug, Clone)]
pub struct MergeReport {
    pub repack: RepackReport,
    /// In the order the archives were given (highest priority first).
    pub sources: Vec<SourceStats>,
}

fn default_format<R: ReadAt>(sources: &[&Archive<R>]) -> Format {
    if sources.iter().any(|a| a.header().version == 0x300) {
        Format::V300
    } else {
        Format::V200
    }
}

/// Merges archives into one; for every path the first archive that has it wins. Output order:
/// the lowest-priority archive's order, then new paths of the next archive, and so on.
pub fn merge<R: ReadAt>(
    sources: &[&Archive<R>],
    dst: &Path,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<MergeReport> {
    merge_with(sources, dst, opts, false, ctl)
}

/// Like [`merge`]. With `replace_only` the **last** archive is the base: archives before it may
/// replace files the base already has but add no new paths (to update a GRF with a newer
/// translation without pulling in everything else that translation contains).
pub fn merge_with<R: ReadAt>(
    sources: &[&Archive<R>],
    dst: &Path,
    opts: &RepackOptions,
    replace_only: bool,
    ctl: &Control,
) -> Result<MergeReport> {
    merge_excluding(sources, dst, opts, replace_only, &HashSet::new(), ctl)
}

/// Like [`merge_with`], and paths whose [`GrfPath::key`] is in `exclude` are left out of every
/// source (how `edit` removes files).
pub fn merge_excluding<R: ReadAt>(
    sources: &[&Archive<R>],
    dst: &Path,
    opts: &RepackOptions,
    replace_only: bool,
    exclude: &HashSet<Vec<u8>>,
    ctl: &Control,
) -> Result<MergeReport> {
    if sources.is_empty() {
        return Err(Error::InvalidInput(
            "merge needs at least one archive".into(),
        ));
    }
    let mut stats = vec![SourceStats::default(); sources.len()];
    let mut at: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut sel: Vec<Sel> = Vec::new();
    let mut names: Vec<Option<GrfPath>> = Vec::new();
    let mut dead: HashSet<usize> = HashSet::new();
    for (src, a) in sources.iter().enumerate().rev() {
        let (order, dup) = last_by_key(a);
        stats[src].markers_skipped = a
            .entries()
            .iter()
            .filter(|e| e.is_marker() && !e.is_removal())
            .count();
        stats[src].shadowed = dup;
        for idx in order {
            let e = a.entry(idx)?;
            if e.is_removal() {
                // a delete directive acts on the archives of lower priority only
                stats[src].removals += 1;
                if let Some(pos) = at.remove(&e.name.key()) {
                    dead.insert(pos);
                    stats[src].removed += 1;
                }
                continue;
            }
            if e.is_marker() {
                continue;
            }
            stats[src].files += 1;
            if exclude.contains(&e.name.key()) {
                stats[src].excluded += 1;
                continue;
            }
            if replace_only && src + 1 != sources.len() && !at.contains_key(&e.name.key()) {
                stats[src].ignored_new += 1;
                continue;
            }
            match at.get(&e.name.key()) {
                Some(&pos) => {
                    stats[sel[pos].src].overridden += 1;
                    // the spelling of the first (lowest-priority) archive stays
                    if names[pos].is_none() {
                        names[pos] = Some(sources[sel[pos].src].entry(sel[pos].idx)?.name.clone());
                    }
                    sel[pos] = Sel { src, idx };
                }
                None => {
                    at.insert(e.name.key(), sel.len());
                    sel.push(Sel { src, idx });
                    names.push(None);
                }
            }
        }
    }
    if !dead.is_empty() {
        let keep = |i: &usize| !dead.contains(i);
        let kept: Vec<usize> = (0..sel.len()).filter(keep).collect();
        sel = kept.iter().map(|&i| sel[i]).collect();
        names = kept.iter().map(|&i| names[i].clone()).collect();
    }
    for s in &sel {
        stats[s.src].used += 1;
    }
    let format = opts.format.unwrap_or_else(|| default_format(sources));
    let selection = Selection {
        sources,
        sel,
        names,
    };
    let repack = write_selection(&selection, dst, format, opts, ctl)?;
    Ok(MergeReport {
        repack,
        sources: stats,
    })
}

#[derive(Debug, Clone)]
pub struct PatchReport {
    pub repack: RepackReport,
    pub added: usize,
    pub changed: usize,
    /// Paths of `old` that `new` no longer has; a GRF cannot delete, so they stay in the client.
    pub removed: usize,
}

/// Writes an archive with the files that are new or different in `new` compared with `old`
/// (to be listed before `old` in `DATA.INI`).
pub fn patch<R: ReadAt>(
    old: &Archive<R>,
    new: &Archive<R>,
    dst: &Path,
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<PatchReport> {
    let d = diff(old, new, ctl)?;
    let wanted: HashSet<Vec<u8>> = d
        .only_b
        .iter()
        .chain(&d.changed)
        .map(GrfPath::key)
        .collect();
    let (order, _) = last_by_key(new);
    let sel: Vec<Sel> = order
        .into_iter()
        .filter(|&i| {
            new.entries()
                .get(i)
                .is_some_and(|e| !e.is_marker() && wanted.contains(&e.name.key()))
        })
        .map(|idx| Sel { src: 0, idx })
        .collect();
    if sel.is_empty() {
        return Err(Error::InvalidInput(
            "the archives have identical content, nothing to patch".into(),
        ));
    }
    let sources = [new];
    let format = opts.format.unwrap_or_else(|| default_format(&sources));
    let selection = Selection {
        sources: &sources,
        sel,
        names: Vec::new(),
    };
    let repack = write_selection(&selection, dst, format, opts, ctl)?;
    Ok(PatchReport {
        repack,
        added: d.only_b.len(),
        changed: d.changed.len(),
        removed: d.only_a.len(),
    })
}
