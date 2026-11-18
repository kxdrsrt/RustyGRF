//! `rgrf map`: GAT/GND/RSW files of a map: info, a picture of the walkability grid, and a check
//! that every file a map names (ground, altitude, textures, models, sounds) exists.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rgrf_core::{Archive, GrfPath, NameMode};
use rgrf_formats::gat::{self, Gat};
use rgrf_formats::gif_io::upscale;
use rgrf_formats::gnd::GndSummary;
use rgrf_formats::png_io::rgba_to_png;
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
        other => bail!("'{other}' is not a map file (.gat, .gnd or .rsw)"),
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
struct Vfs {
    archives: Vec<(String, Archive)>,
    keymaps: Vec<HashMap<Vec<u8>, Vec<usize>>>,
    loose: BTreeSet<Vec<u8>>,
}

impl Vfs {
    fn open(input: &Path) -> Result<Self> {
        let mut archives = Vec::new();
        let mut loose = BTreeSet::new();
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
                archives.push((e.file.clone(), Archive::open(p)?));
            }
            if let Some(d) = find_ci(input, "data").filter(|d| d.is_dir()) {
                let mut files: Vec<(String, PathBuf)> = Vec::new();
                walk(&d, input, &mut files);
                for (rel, _) in files {
                    let rel: String = rel.nfc().collect();
                    if let Some(p) = GrfPath::from_display(&rel, NameMode::Cp1252) {
                        loose.insert(p.key());
                    }
                }
            }
        } else {
            archives.push((input.display().to_string(), Archive::open(input)?));
        }
        let keymaps = archives.iter().map(|(_, a)| a.index_by_key()).collect();
        Ok(Self {
            archives,
            keymaps,
            loose,
        })
    }

    /// (archive, entry) of the file that wins for `key`.
    fn find(&self, key: &[u8]) -> Option<(usize, usize)> {
        self.keymaps
            .iter()
            .enumerate()
            .find_map(|(n, m)| m.get(key).and_then(|v| v.last()).map(|&i| (n, i)))
    }

    fn exists(&self, key: &[u8]) -> bool {
        self.find(key).is_some() || self.loose.contains(key)
    }

    fn read(&self, at: (usize, usize)) -> Result<Vec<u8>> {
        Ok(self.archives[at.0].1.read(at.1)?.data)
    }
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
            "Not checked: textures used inside the models (.rsm), effects, and whether the files themselves are valid beyond GAT/GND headers."
        );
    }
    Ok(if problems_total == 0 { 0 } else { 3 })
}
