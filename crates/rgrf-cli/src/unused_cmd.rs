//! `rgrf client unused`: files in the archives that no map, animation, effect, text, Lua table or
//! program names (see [`rgrf_formats::unused`] for what that can and cannot tell).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use rayon::prelude::*;
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::act::Act;
use rgrf_formats::map_files::resolve;
use rgrf_formats::unused::Names;
use serde_json::json;

use crate::map_cmd::Vfs;

/// Folders of maps: their files are named in maps and models (reliable); everything else is put
/// together from pieces by the client or its tables (effects, interface, sprites, sounds).
fn composed(folder: &str) -> bool {
    let low = folder.to_lowercase();
    !(low.starts_with("data\\model") || low.starts_with("data\\texture\\"))
        || low.starts_with("data\\texture\\effect")
        || low.starts_with("data\\texture\\유저인터페이스")
        || low.starts_with("data\\texture\\userinterface")
        || low.starts_with("data\\model\\3dmob")
}

/// Folders whose files are looked at (everything else is left alone: tables, maps, scripts).
const AREAS: [&str; 4] = [
    "data\\texture\\",
    "data\\sprite\\",
    "data\\wav\\",
    "data\\model\\",
];

fn lower(b: &[u8]) -> Vec<u8> {
    b.to_ascii_lowercase()
}

fn ext_of(raw: &[u8]) -> Vec<u8> {
    raw.rsplit(|&b| b == b'.')
        .next()
        .map(lower)
        .unwrap_or_default()
}

pub fn unused(input: &Path, limit: usize, files: bool, names: NameMode, json: bool) -> Result<i32> {
    let vfs = Vfs::open(input)?;
    // the files that win, with their stored size
    struct Win {
        at: (usize, usize),
        raw: Vec<u8>,
        ext: Vec<u8>,
        zsize: u64,
    }
    let mut wins = Vec::new();
    for (n, (_, a)) in vfs.archives.iter().enumerate() {
        for idxs in vfs.keymaps[n].values() {
            let Some(&i) = idxs.last() else { continue };
            let e = &a.entries()[i];
            if !e.is_file() || vfs.find(&e.name.key()) != Some((n, i)) {
                continue;
            }
            let raw = e.name.as_bytes().to_vec();
            wins.push(Win {
                at: (n, i),
                ext: ext_of(&raw),
                raw,
                zsize: u64::from(e.zsize),
            });
        }
    }
    if wins.is_empty() {
        bail!("no files found");
    }
    let started = std::time::Instant::now();

    // text, Lua, XML: every name-like piece; animations: their sounds; effects: their textures;
    // maps: everything they name
    let parts: Vec<Names> = wins
        .par_iter()
        .filter(|w| {
            matches!(
                w.ext.as_slice(),
                b"txt"
                    | b"xml"
                    | b"lua"
                    | b"lub"
                    | b"ini"
                    | b"csv"
                    | b"json"
                    | b"act"
                    | b"str"
                    | b"rsw"
            )
        })
        .map(|w| {
            let mut n = Names::new();
            let Ok(data) = vfs.read(w.at) else { return n };
            match w.ext.as_slice() {
                b"act" => {
                    if let Ok(a) = Act::parse(&data) {
                        for s in &a.sounds {
                            n.add_name(s.split(|&b| b == 0).next().unwrap_or(&[]));
                        }
                    }
                }
                b"str" => {
                    if let Ok(t) = rgrf_formats::map_files::effect_textures(&data) {
                        t.iter().for_each(|x| n.add_name(x));
                    }
                }
                b"rsw" => {
                    let base = String::from_utf8_lossy(&w.raw).to_string();
                    if let Ok(list) = resolve(&base, &vfs) {
                        list.iter().for_each(|f| n.add_name(&f.path));
                    }
                }
                _ => n.scan(&data),
            }
            n
        })
        .collect();
    let mut named = Names::new();
    for p in &parts {
        named.merge(p);
    }
    // the programs and settings next to the archives name files too
    let mut programs = 0;
    if input.is_dir() {
        for dir in [input.to_path_buf(), input.join("System")] {
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for f in rd.flatten() {
                let p = f.path();
                let ext = p
                    .extension()
                    .map(|e| e.to_ascii_lowercase())
                    .and_then(|e| e.into_string().ok())
                    .unwrap_or_default();
                if matches!(
                    ext.as_str(),
                    "exe" | "dll" | "ini" | "xml" | "lua" | "lub" | "txt"
                ) && let Ok(d) = fs::read(&p)
                {
                    named.scan(&d);
                    programs += 1;
                }
            }
        }
    }

    // by folder (three levels: data\texture\folder): total and unnamed
    struct Area {
        total: (u64, u64),
        unnamed: (u64, u64),
        examples: Vec<String>,
    }
    let mut areas: BTreeMap<String, Area> = BTreeMap::new();
    let mut listed: Vec<(String, u64)> = Vec::new();
    for w in &wins {
        let low = lower(&w.raw);
        if !AREAS.iter().any(|a| low.starts_with(a.as_bytes())) {
            continue;
        }
        // maps name their own models and textures; the files of a loose list do not count
        let top: Vec<&[u8]> = w.raw.split(|&b| b == b'\\').collect();
        let key = top[..top.len().saturating_sub(1).min(3)]
            .iter()
            .map(|p| GrfPath::new(p.to_vec()).display(names))
            .collect::<Vec<_>>()
            .join("\\");
        let a = areas.entry(key).or_insert(Area {
            total: (0, 0),
            unnamed: (0, 0),
            examples: Vec::new(),
        });
        a.total.0 += 1;
        a.total.1 += w.zsize;
        if !named.names(&w.raw) {
            a.unnamed.0 += 1;
            a.unnamed.1 += w.zsize;
            let shown = GrfPath::new(w.raw.clone()).display(names);
            if a.examples.len() < 3 {
                a.examples.push(shown.clone());
            }
            listed.push((shown, w.zsize));
        }
    }
    let (n_total, n_unnamed, bytes_unnamed): (u64, u64, u64) =
        areas.values().fold((0, 0, 0), |s, a| {
            (s.0 + a.total.0, s.1 + a.unnamed.0, s.2 + a.unnamed.1)
        });
    let mut rows: Vec<(&String, &Area)> = areas.iter().filter(|(_, a)| a.unnamed.0 > 0).collect();
    rows.sort_by(|a, b| b.1.unnamed.1.cmp(&a.1.unnamed.1).then(a.0.cmp(b.0)));
    listed.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mib = |b: u64| b as f64 / 1_048_576.0;
    let (rel_files, rel_bytes) = areas
        .iter()
        .filter(|(k, _)| !composed(k))
        .fold((0, 0), |s, (_, a)| (s.0 + a.unnamed.0, s.1 + a.unnamed.1));
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "files_looked_at": n_total,
                "unnamed_files": n_unnamed,
                "unnamed_stored_bytes": bytes_unnamed,
                "unnamed_in_map_folders": rel_files,
                "unnamed_in_map_folders_stored_bytes": rel_bytes,
                "names_found": named.len(),
                "programs_scanned": programs,
                "folders": rows.iter().take(limit).map(|(k, a)| json!({"folder": k, "names_are_composed": composed(k), "files": a.total.0, "unnamed": a.unnamed.0, "unnamed_stored_bytes": a.unnamed.1, "examples": a.examples})).collect::<Vec<_>>(),
                "list": if files { listed.iter().take(limit).map(|(n, z)| json!({"file": n, "stored_bytes": z})).collect::<Vec<_>>() } else { vec![] },
            })
        );
    } else {
        println!(
            "{} names found in text, Lua, animations, effects, maps and {programs} programs/settings ({:.1}s)",
            named.len(),
            started.elapsed().as_secs_f64()
        );
        println!(
            "{n_total} files in {} looked at; {n_unnamed} are named by nothing ({:.1} MiB stored)\n",
            AREAS.join(", "),
            mib(bytes_unnamed)
        );
        println!(
            "Folders of maps (marked *): the names are written in maps and models, so this is reliable: {rel_files} files, {:.1} MiB. Other folders (sprites, effects, interface, sounds) are named by tables and program code in pieces: mostly false alarms.\n",
            mib(rel_bytes)
        );
        for (k, a) in rows.iter().take(limit) {
            println!(
                "{:>8.1} MiB {} {:>6}/{:<6} {k}   e.g. {}",
                mib(a.unnamed.1),
                if composed(k) { " " } else { "*" },
                a.unnamed.0,
                a.total.0,
                a.examples
                    .first()
                    .map_or("", |e| e.rsplit('\\').next().unwrap_or(e))
            );
        }
        if rows.len() > limit {
            println!("... and {} more folders (use --limit)", rows.len() - limit);
        }
        if files {
            println!();
            for (n, z) in listed.iter().take(limit) {
                println!("{:>10}  {n}", z);
            }
        }
        println!(
            "\nNames put together while the program runs cannot be seen, so these files are only probably unused. Check a folder in the game before leaving its files out."
        );
    }
    Ok(0)
}
