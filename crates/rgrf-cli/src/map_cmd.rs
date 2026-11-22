//! `rgrf map`: GAT/GND/RSW files of a map: info, a picture of the walkability grid, a check
//! that every file a map names (ground, altitude, textures, models, sounds) exists, and the
//! extraction of one map with everything it needs.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rgrf_core::{Archive, GrfPath, NameMode};
use rgrf_formats::gat::{self, Gat};
use rgrf_formats::gif_io::upscale;
use rgrf_formats::gnd::GndSummary;
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::rsm::Rsm;
use rgrf_formats::rsw::{self, Object, Rsw};
use rgrf_formats::spr::RgbaImage;
use serde_json::json;
use unicode_normalization::UnicodeNormalization;

use crate::client_cmd::{find_ci, parse_ini, walk};
use crate::spr_cmd::load;

fn ext(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let kind = ext(entry
        .map(str::to_owned)
        .unwrap_or_else(|| input.display().to_string())
        .as_str());
    match kind.as_str() {
        "gat" => {
            let g = Gat::parse(&data).with_context(|| format!("{what} is not a readable GAT"))?;
            let (lo, hi) = g.height_range();
            let counts: Vec<_> = g.type_counts();
            if json {
                println!(
                    "{}",
                    json!({
                        "schema_version": crate::SCHEMA_VERSION,
                        "source": what, "kind": "gat", "version": g.version.to_string(),
                        "width": g.width, "height": g.height,
                        "height_min": lo, "height_max": hi,
                        "types": counts.iter().map(|(k, n)| json!({"type": k, "name": gat::type_name(*k), "cells": n})).collect::<Vec<_>>(),
                    })
                );
            } else {
                println!("{what}");
                println!(
                    "  GAT version {}, {} x {} cells, heights {lo:.1} to {hi:.1}",
                    g.version, g.width, g.height
                );
                for (k, n) in counts {
                    println!("  type {k:>11} ({}): {n} cells", gat::type_name(k));
                }
            }
        }
        "gnd" => {
            let g = GndSummary::parse(&data)
                .with_context(|| format!("{what} is not a readable GND"))?;
            if json {
                println!(
                    "{}",
                    json!({
                        "schema_version": crate::SCHEMA_VERSION,
                        "source": what, "kind": "gnd", "version": g.version.to_string(),
                        "width": g.width, "height": g.height, "cell_size": g.cell_size,
                        "textures": g.texture_names(),
                    })
                );
            } else {
                println!("{what}");
                println!(
                    "  GND version {}, {} x {} ground cells of size {}, {} textures:",
                    g.version,
                    g.width,
                    g.height,
                    g.cell_size,
                    g.textures.len()
                );
                for t in g.texture_names() {
                    println!("    {t}");
                }
            }
        }
        "rsw" => {
            let r = Rsw::parse(&data).with_context(|| format!("{what} is not a readable RSW"))?;
            let (m, l, s, e) = r.object_counts();
            if json {
                println!(
                    "{}",
                    json!({
                        "schema_version": crate::SCHEMA_VERSION,
                        "source": what, "kind": "rsw", "version": r.version.to_string(),
                        "ground": rsw::text(&r.ground), "altitude": rsw::text(&r.altitude),
                        "objects": {"models": m, "lights": l, "sounds": s, "effects": e},
                        "model_files": r.model_files(), "sound_files": r.sound_files(),
                    })
                );
            } else {
                println!("{what}");
                println!(
                    "  RSW version {}, ground {}, altitude {}",
                    r.version,
                    rsw::text(&r.ground),
                    rsw::text(&r.altitude)
                );
                println!(
                    "  objects: {m} models ({} files), {l} lights, {s} sounds ({} files), {e} effects",
                    r.model_files().len(),
                    r.sound_files().len()
                );
                if let Some(w) = &r.water {
                    println!("  water level {}, type {}", w.level, w.kind);
                }
            }
        }
        "rsm" | "rsm2" => {
            let m = Rsm::parse(&data).with_context(|| format!("{what} is not a readable RSM"))?;
            let tex: Vec<String> = m.all_textures().iter().map(|t| latin1(t)).collect();
            let faces: usize = m.meshes.iter().map(|x| x.faces).sum();
            let verts: usize = m.meshes.iter().map(|x| x.vertices).sum();
            if json {
                println!(
                    "{}",
                    json!({
                        "schema_version": crate::SCHEMA_VERSION,
                        "source": what, "kind": "rsm", "version": m.version.to_string(),
                        "meshes": m.meshes.iter().map(|x| json!({"name": latin1(&x.name), "parent": latin1(&x.parent), "vertices": x.vertices, "faces": x.faces})).collect::<Vec<_>>(),
                        "textures": tex, "volume_boxes": m.volume_boxes,
                        "unread_bytes": m.trailing,
                    })
                );
            } else {
                println!("{what}");
                println!(
                    "  RSM version {}, {} meshes, {verts} vertices, {faces} faces, {} volume boxes",
                    m.version,
                    m.meshes.len(),
                    m.volume_boxes
                );
                for x in &m.meshes {
                    println!(
                        "    mesh {} (parent '{}'): {} vertices, {} faces",
                        latin1(&x.name),
                        latin1(&x.parent),
                        x.vertices,
                        x.faces
                    );
                }
                println!("  textures:");
                for t in tex {
                    println!("    {t}");
                }
                if m.trailing != 0 {
                    println!("  warning: {} bytes after the last known part", m.trailing);
                }
            }
        }
        other => bail!("'{other}' is not a map file (.gat, .gnd, .rsw, .rsm or .rsm2)"),
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn gat_set(
    input: &Path,
    entry: Option<&str>,
    (from, to): ((u32, u32), (u32, u32)),
    (kind, height): (Option<i32>, Option<f32>),
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    if kind.is_none() && height.is_none() {
        bail!("nothing to change: give --type and/or --height");
    }
    let (data, what) = load(input, entry, names)?;
    let mut g = Gat::parse(&data).with_context(|| format!("{what} is not a readable GAT"))?;
    let changed = g.set_cells(from, to, kind, height)?;
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    fs::write(output, g.to_bytes())
        .with_context(|| format!("cannot write {}", output.display()))?;
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "operation": "map gat-set",
                "output": output.display().to_string(),
                "details": {"cells_changed": changed},
            })
        );
    } else {
        println!("{changed} cells changed, written to {}", output.display());
    }
    Ok(0)
}

pub fn png(
    input: &Path,
    entry: Option<&str>,
    output: &Path,
    scale: u32,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let g = Gat::parse(&data).with_context(|| format!("{what} is not a readable GAT"))?;
    let img = g
        .to_image()
        .context("map too large for a PNG (more than 65535 cells wide)")?;
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    let s = scale.max(1) as usize;
    let (w, h) = (usize::from(img.width) * s, usize::from(img.height) * s);
    let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
        bail!("{w} x {h} pixels is too large; use a smaller --scale");
    };
    let pixels = if s == 1 {
        img.pixels
    } else {
        upscale(
            &img.pixels,
            usize::from(img.width),
            usize::from(img.height),
            s,
        )
    };
    fs::write(
        output,
        rgba_to_png(&RgbaImage {
            width: w16,
            height: h16,
            pixels,
        })?,
    )
    .with_context(|| format!("cannot write {}", output.display()))?;
    if json {
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "output": output.display().to_string(), "width": w, "height": h})
        );
    } else {
        println!("{what}: {w} x {h} px -> {}", output.display());
    }
    Ok(0)
}

/// The files of a client or of one GRF: first archive wins; loose `data` files count too.
pub(crate) struct Vfs {
    pub(crate) archives: Vec<(String, Archive)>,
    pub(crate) keymaps: Vec<HashMap<Vec<u8>, Vec<usize>>>,
    /// Loose files of the `data` and `System` folders of a client: key -> file.
    loose: BTreeMap<Vec<u8>, PathBuf>,
}

impl Vfs {
    pub(crate) fn open(input: &Path) -> Result<Self> {
        let mut archives = Vec::new();
        let mut loose = BTreeMap::new();
        if input.is_dir() {
            let Some(ini) = find_ci(input, "DATA.INI") else {
                bail!("no DATA.INI in {}", input.display());
            };
            let text: String = fs::read(&ini)?.iter().map(|&c| char::from(c)).collect();
            for e in parse_ini(&text) {
                let Some(p) = find_ci(input, &e.file) else {
                    bail!(
                        "DATA.INI lists {} but it does not exist (see `rgrf client check`)",
                        e.file
                    );
                };
                archives.push((e.file.clone(), crate::crypt_cmd::open_archive(p)?));
            }
            for folder in ["data", "System"] {
                let Some(d) = find_ci(input, folder).filter(|d| d.is_dir()) else {
                    continue;
                };
                let mut files: Vec<(String, PathBuf)> = Vec::new();
                walk(&d, input, &mut files);
                for (rel, path) in files {
                    let rel: String = rel.nfc().collect();
                    if let Some(p) = GrfPath::from_display(&rel, NameMode::Cp1252) {
                        loose.insert(p.key(), path);
                    }
                }
            }
        } else {
            archives.push((
                input.display().to_string(),
                crate::crypt_cmd::open_archive(input)?,
            ));
        }
        let keymaps = archives.iter().map(|(_, a)| a.index_by_key()).collect();
        Ok(Self {
            archives,
            keymaps,
            loose,
        })
    }

    /// (archive, entry) of the file that wins for `key`.
    pub(crate) fn find(&self, key: &[u8]) -> Option<(usize, usize)> {
        self.keymaps
            .iter()
            .enumerate()
            .find_map(|(n, m)| m.get(key).and_then(|v| v.last()).map(|&i| (n, i)))
    }

    pub(crate) fn exists(&self, key: &[u8]) -> bool {
        self.find(key).is_some() || self.loose.contains_key(key)
    }

    pub(crate) fn read(&self, at: (usize, usize)) -> Result<Vec<u8>> {
        Ok(self.archives[at.0].1.read(at.1)?.data)
    }
}

impl rgrf_formats::map_files::MapSource for Vfs {
    fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
        let key = GrfPath::new(path.to_vec()).key();
        match self.find(&key) {
            Some(at) => Vfs::read(self, at).ok(),
            None => fs::read(self.loose.get(&key)?).ok(),
        }
    }

    fn exists(&self, path: &[u8]) -> bool {
        Vfs::exists(self, &GrfPath::new(path.to_vec()).key())
    }

    fn list(&self, prefix: &[u8]) -> Vec<Vec<u8>> {
        let prefix = GrfPath::new(prefix.to_vec()).key();
        let mut out: Vec<Vec<u8>> = self
            .keymaps
            .iter()
            .flat_map(|m| m.keys())
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        out.extend(
            self.loose
                .range(prefix.clone()..)
                .take_while(|(k, _)| k.starts_with(&prefix))
                .map(|(k, _)| k.clone()),
        );
        out.sort();
        out.dedup();
        out
    }
}

/// `rgrf map extract`: one map and every file it needs (GRF Editor's map extractor, plus sounds,
/// water pictures and the minimap) into a folder, or only the list without `-o`.
pub fn extract(
    input: &Path,
    map: &str,
    output: Option<&Path>,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    use rgrf_formats::map_files::resolve;
    let vfs = Vfs::open(input)?;
    let files = resolve(map, &vfs).map_err(|e| anyhow::anyhow!(e))?;
    let mut rows = Vec::new();
    let (mut written, mut bytes, mut missing, mut loose_only, mut failed) = (0, 0u64, 0, 0, 0);
    for f in &files {
        let at = vfs.find(&GrfPath::new(f.path.clone()).key());
        // the name as the archive spells it
        let name = at.map_or_else(
            || GrfPath::new(f.path.clone()),
            |(n, i)| vfs.archives[n].1.entries()[i].name.clone(),
        );
        let mut state = if !f.found {
            missing += 1;
            "missing".to_string()
        } else if at.is_none() {
            loose_only += 1;
            "loose file in the client folder (not copied)".to_string()
        } else {
            "found".to_string()
        };
        if let (Some(out), Some(at)) = (output, at) {
            let res = (|| -> Result<u64> {
                let rel = rgrf_core::extract::safe_relative_path(&name, names)?;
                let dst = out.join(rel);
                if dst.exists() && !overwrite {
                    bail!("{} exists already (use --overwrite)", dst.display());
                }
                let d = vfs.archives[at.0].1.read(at.1)?;
                if !d.status.is_ok() {
                    bail!("cannot be decoded: {:?}", d.status);
                }
                if let Some(p) = dst.parent() {
                    fs::create_dir_all(p)?;
                }
                fs::write(&dst, &d.data)
                    .with_context(|| format!("cannot write {}", dst.display()))?;
                Ok(d.data.len() as u64)
            })();
            match res {
                Ok(n) => {
                    written += 1;
                    bytes += n;
                    state = "written".into();
                }
                Err(e) => {
                    failed += 1;
                    state = e.to_string();
                }
            }
        }
        rows.push((name.display(names), f, state));
    }
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "map": rgrf_formats::map_files::map_base(map),
                "files": rows.iter().map(|(n, f, s)| json!({
                    "path": n, "kind": f.kind.name(), "found": f.found, "state": s,
                    "named_by": f.named_by.as_ref().map(|b| GrfPath::new(b.clone()).display(names)),
                    "problem": f.problem,
                })).collect::<Vec<_>>(),
                "written": written, "bytes": bytes, "missing": missing,
                "loose_only": loose_only, "failed": failed,
                "output": output.map(|o| o.display().to_string()),
            })
        );
    } else {
        let mut last = None;
        for (n, f, s) in &rows {
            if last != Some(f.kind) {
                println!("{}:", f.kind.name());
                last = Some(f.kind);
            }
            let note = f
                .problem
                .as_deref()
                .map(|p| format!(" ({p})"))
                .unwrap_or_default();
            if s == "found" || s == "written" {
                println!("  {n}{note}");
            } else {
                println!("  {n}: {s}{note}");
            }
        }
        println!(
            "{} files: {} found, {missing} missing{}{}",
            rows.len(),
            rows.len() - missing,
            if loose_only > 0 {
                format!(", {loose_only} only as loose files")
            } else {
                String::new()
            },
            match output {
                Some(o) => format!(
                    "; {written} written to {} ({bytes} bytes), {failed} failed",
                    o.display()
                ),
                None => String::new(),
            }
        );
    }
    Ok(if failed > 0 {
        1
    } else if missing > 0 {
        3
    } else {
        0
    })
}

/// `rgrf map flat`: every chosen map through [`rgrf_formats::flat_map::apply`]; the changed files
/// go into a new archive.
pub fn flat(
    input: &Path,
    maps: &[String],
    output: &Path,
    o: &rgrf_formats::flat_map::FlatOptions,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    map_patch(input, maps, output, overwrite, names, json, |base, read| {
        rgrf_formats::flat_map::apply_map(base, read, o)
    })
}

/// GRF Editor's "Downgrade" on maps: RSM2 models become RSM1, world file 2.1, ground file 1.7.
pub fn downgrade(
    input: &Path,
    maps: &[String],
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    map_patch(input, maps, output, overwrite, names, json, |base, read| {
        // without names, only the maps that have RSM2 models
        Ok(
            rgrf_formats::downgrade::apply_map(base, read)?.map(|(files, d)| {
                if maps.is_empty() && d.instances == 0 {
                    Vec::new()
                } else {
                    files
                }
            }),
        )
    })
}

type MapFiles = Result<Result<Vec<(Vec<u8>, Vec<u8>)>, String>>;

/// Runs `per_map` on the maps (by their world file) and writes the files it returns as one small archive.
fn map_patch(
    input: &Path,
    maps: &[String],
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
    per_map: impl Fn(&[u8], &dyn Fn(&[u8]) -> Result<Option<Vec<u8>>>) -> MapFiles + Sync,
) -> Result<i32> {
    use rayon::prelude::*;
    let vfs = Vfs::open(input)?;
    let mut rsws: Vec<(Vec<u8>, (usize, usize))> = Vec::new();
    let mut seen = BTreeSet::new();
    for (n, (_, a)) in vfs.archives.iter().enumerate() {
        for (key, idxs) in &vfs.keymaps[n] {
            let Some(&i) = idxs.last() else { continue };
            if !key.ends_with(b".rsw") || !seen.insert(key.clone()) {
                continue;
            }
            let base = key.rsplit(|&b| b == b'\\').next().unwrap_or(key);
            let base = &base[..base.len() - 4];
            if maps.is_empty()
                || maps.iter().any(|m| {
                    let m = m.to_ascii_lowercase();
                    m.as_bytes() == base || m.trim_end_matches(".rsw").as_bytes() == base
                })
            {
                rsws.push((a.entries()[i].name.as_bytes().to_vec(), (n, i)));
            }
        }
    }
    rsws.sort();
    if rsws.is_empty() {
        bail!("no matching .rsw file found");
    }
    let read = |key: &[u8]| -> Result<Option<Vec<u8>>> {
        vfs.find(key).map(|at| vfs.read(at)).transpose()
    };
    type Done = (Vec<rgrf_core::edit::Packed>, usize, Option<String>);
    let results: Vec<Result<Done>> = rsws
        .par_iter()
        .map(|(rsw_name, _)| {
            let shown = GrfPath::new(rsw_name.clone()).display(names);
            let base = &rsw_name[..rsw_name.len() - 4];
            let changed = match per_map(base, &|p: &[u8]| read(&GrfPath::new(p.to_vec()).key()))? {
                Ok(c) => c,
                Err(why) => return Ok((vec![], 0, Some(format!("{shown}: {why}")))),
            };
            let mut out = Vec::new();
            let mut bytes = 0;
            for (path, d) in changed {
                bytes += d.len();
                out.push(rgrf_core::edit::pack(
                    GrfPath::new(path),
                    &d,
                    rgrf_core::encode::Method::DEFAULT,
                )?);
            }
            Ok((out, bytes, None))
        })
        .collect();
    let mut files = Vec::new();
    let mut problems = Vec::new();
    let (mut changed_maps, mut bytes) = (0, 0);
    for r in results {
        let (f, b, p) = r?;
        changed_maps += usize::from(!f.is_empty());
        bytes += b;
        files.extend(f);
        problems.extend(p);
    }
    let written = if files.is_empty() {
        0
    } else {
        rgrf_core::edit::write_packed(&files, output, overwrite)
            .with_context(|| format!("writing {}", output.display()))?
    };
    if json {
        println!(
            "{}",
            json!({
                "maps": rsws.len(),
                "changed_maps": changed_maps,
                "files": written,
                "bytes": bytes,
                "output": (!files.is_empty()).then(|| output.display().to_string()),
                "problems": problems,
            })
        );
    } else {
        for p in &problems {
            eprintln!("{p}");
        }
        if files.is_empty() {
            println!("{} map(s), nothing changed, nothing written", rsws.len());
        } else {
            println!(
                "{changed_maps} of {} map(s) changed: {} file(s), {bytes} bytes -> {}",
                rsws.len(),
                written,
                output.display()
            );
        }
    }
    Ok(if problems.is_empty() { 0 } else { 3 })
}

fn key_of(prefix: &str, raw: &[u8]) -> Vec<u8> {
    let mut b = prefix.as_bytes().to_vec();
    b.extend(raw);
    GrfPath::new(b).key()
}

fn cstr(raw: &[u8]) -> Vec<u8> {
    raw.iter().take_while(|&&b| b != 0).copied().collect()
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

pub fn check(input: &Path, maps: &[String], names: NameMode, json: bool) -> Result<i32> {
    let vfs = Vfs::open(input)?;
    // The RSW files that win, optionally only the named maps.
    let mut rsws: Vec<(String, (usize, usize))> = Vec::new();
    let mut seen = BTreeSet::new();
    for (n, (_, a)) in vfs.archives.iter().enumerate() {
        for (key, idxs) in &vfs.keymaps[n] {
            let Some(&i) = idxs.last() else { continue };
            let name = a.entries()[i].name.display(names);
            if !name.to_ascii_lowercase().ends_with(".rsw") || !seen.insert(key.clone()) {
                continue;
            }
            let base = name
                .rsplit('\\')
                .next()
                .unwrap_or(&name)
                .to_ascii_lowercase();
            let base = base.trim_end_matches(".rsw");
            if maps.is_empty()
                || maps
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(base) || m.eq_ignore_ascii_case(&name))
            {
                rsws.push((name, (n, i)));
            }
        }
    }
    rsws.sort();
    if rsws.is_empty() {
        bail!("no matching .rsw file found");
    }
    let (mut looked, mut problems_total) = (0usize, 0usize);
    let mut report: Vec<(String, Vec<String>)> = Vec::new();
    // Models are read once; a missing texture is reported once, under the first map that uses it.
    let mut model_textures: HashMap<Vec<u8>, Result<Vec<Vec<u8>>, String>> = HashMap::new();
    let mut reported: BTreeSet<(Vec<u8>, Vec<u8>)> = BTreeSet::new();
    for (name, at) in &rsws {
        let mut problems = Vec::new();
        let data = vfs.read(*at)?;
        let r = match Rsw::parse(&data) {
            Ok(r) => r,
            Err(e) => {
                report.push((name.clone(), vec![format!("RSW cannot be read: {e}")]));
                problems_total += 1;
                continue;
            }
        };
        let mut check = |prefix: &str, raw: &[u8], what: &str, problems: &mut Vec<String>| {
            let t = cstr(raw);
            if t.is_empty() {
                return;
            }
            looked += 1;
            if !vfs.exists(&key_of(prefix, &t)) {
                problems.push(format!("{what} {prefix}{} is missing", latin1(&t)));
            }
        };
        check("data\\", &r.ground, "ground", &mut problems);
        check("data\\", &r.altitude, "altitude", &mut problems);
        let mut done = BTreeSet::new();
        for o in &r.objects {
            match o {
                Object::Model(m) if done.insert(m.model_name.to_vec()) => {
                    check("data\\model\\", &m.model_name, "model", &mut problems)
                }
                Object::Sound(s) if done.insert(s.wave_name.to_vec()) => {
                    check("data\\wav\\", &s.wave_name, "sound", &mut problems)
                }
                _ => {}
            }
        }
        if let Some(gat_at) = vfs.find(&key_of("data\\", &cstr(&r.altitude)))
            && let Err(e) = Gat::parse(&vfs.read(gat_at)?)
        {
            problems.push(format!("altitude file cannot be read: {e}"));
        }
        if let Some(gnd_at) = vfs.find(&key_of("data\\", &cstr(&r.ground))) {
            match GndSummary::parse(&vfs.read(gnd_at)?) {
                Err(e) => problems.push(format!("ground file cannot be read: {e}")),
                Ok(g) => {
                    let mut tex = BTreeSet::new();
                    for t in &g.textures {
                        if tex.insert(t.clone()) {
                            check("data\\texture\\", t, "texture", &mut problems);
                        }
                    }
                }
            }
        }
        // Textures inside the models (the model files that exist; missing ones are reported above).
        for o in &r.objects {
            let Object::Model(m) = o else { continue };
            let model = cstr(&m.model_name);
            let mkey = key_of("data\\model\\", &model);
            if !model_textures.contains_key(&mkey) {
                let parsed = match vfs.find(&mkey) {
                    None => Ok(Vec::new()),
                    Some(at) => match Rsm::parse(&vfs.read(at)?) {
                        Ok(rsm) => Ok(rsm.all_textures()),
                        Err(e) => Err(e.to_string()),
                    },
                };
                if let Ok(t) = &parsed {
                    looked += t.len();
                }
                model_textures.insert(mkey.clone(), parsed);
            }
            match &model_textures[&mkey] {
                Err(e) => {
                    if reported.insert((model.clone(), Vec::new())) {
                        problems.push(format!(
                            "model data\\model\\{} cannot be read: {e}",
                            latin1(&model)
                        ));
                    }
                }
                Ok(textures) => {
                    for t in textures {
                        if !vfs.exists(&key_of("data\\texture\\", t))
                            && reported.insert((model.clone(), t.clone()))
                        {
                            problems.push(format!(
                                "model data\\model\\{} uses the texture data\\texture\\{} which is missing",
                                latin1(&model),
                                latin1(t)
                            ));
                        }
                    }
                }
            }
        }
        if !problems.is_empty() {
            problems_total += problems.len();
            report.push((name.clone(), problems));
        }
    }
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "maps_checked": rsws.len(),
                "files_looked_up": looked,
                "problems": problems_total,
                "maps_with_problems": report.iter().map(|(m, p)| json!({"map": m, "problems": p})).collect::<Vec<_>>(),
            })
        );
    } else {
        for (m, p) in &report {
            for line in p {
                println!("{m}: {line}");
            }
        }
        println!(
            "{} maps checked, {looked} files looked up (ground, altitude, textures, models, sounds), {problems_total} problems",
            rsws.len()
        );
        println!(
            "Not checked: effects, textures of the effects (.str), and whether the files are valid beyond what is read (GAT, GND header, RSM structure)."
        );
    }
    Ok(if problems_total == 0 { 0 } else { 3 })
}
