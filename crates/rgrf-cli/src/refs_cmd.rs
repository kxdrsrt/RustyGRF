//! `rgrf client refs`: the files animations (.act) and effects (.str) name, checked against the
//! client's archives before the client finds them missing ("Resource File Loading fail").

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use rayon::prelude::*;
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::act::{Act, V2_1};
use rgrf_formats::spr::Spr;
use rgrf_formats::str_effect;
use serde_json::json;

use crate::map_cmd::Vfs;

/// One thing wrong with a file: a short stable word and the details.
struct Problem {
    kind: &'static str,
    file: String,
    detail: String,
}

/// An archive path as the user chose to see names.
fn shown_path(b: &[u8], names: NameMode) -> String {
    GrfPath::new(b.to_vec()).display(names)
}

fn key(path: &[u8]) -> Vec<u8> {
    GrfPath::new(path.to_vec()).key()
}

fn dir_of(path: &[u8]) -> &[u8] {
    path.iter()
        .rposition(|&b| b == b'\\')
        .map_or(&path[..0], |p| &path[..=p])
}

fn check_act(
    vfs: &Vfs,
    path: &[u8],
    data: &[u8],
    shown: &str,
    names: NameMode,
    out: &mut Vec<Problem>,
) {
    let mut add = |kind, detail: String| {
        out.push(Problem {
            kind,
            file: shown.to_string(),
            detail,
        })
    };
    let act = match Act::parse(data) {
        Ok(a) => a,
        Err(e) => return add("unreadable_act", e.to_string()),
    };
    // the sprite next to it
    let mut spr_path = path.to_vec();
    match spr_path.iter().rposition(|&b| b == b'.') {
        Some(dot) => spr_path.truncate(dot + 1),
        None => return,
    }
    spr_path.extend(b"spr");
    let mut counts = None;
    match vfs.find(&key(&spr_path)) {
        None => {
            if !vfs.exists(&key(&spr_path)) {
                add("missing_sprite", shown_path(&spr_path, names));
            }
        }
        Some(at) => match vfs.read(at).ok().map(|d| Spr::parse(&d)) {
            Some(Ok(s)) => counts = Some((s.indexed.len() as i64, s.rgba.len() as i64)),
            Some(Err(e)) => add(
                "unreadable_sprite",
                format!("{}: {e}", shown_path(&spr_path, names)),
            ),
            None => add("unreadable_sprite", shown_path(&spr_path, names)),
        },
    }
    // images the layers ask for
    if let Some((indexed, rgba)) = counts {
        let mut worst: BTreeMap<i32, i64> = BTreeMap::new();
        for (_, _, _, l) in act.layers() {
            if l.sprite_index < 0 {
                continue;
            }
            let have = if l.sprite_type == 0 { indexed } else { rgba };
            if i64::from(l.sprite_index) >= have {
                let w = worst.entry(l.sprite_type).or_default();
                *w = (*w).max(i64::from(l.sprite_index));
            }
        }
        for (ty, idx) in worst {
            let (kind, have) = if ty == 0 {
                ("palette", indexed)
            } else {
                ("true-colour", rgba)
            };
            add(
                "image_out_of_range",
                format!("a layer uses {kind} image {idx}, the sprite has {have}"),
            );
        }
    }
    // sounds played by a frame ("atk" marks the frame that hits and is no file); before ACT 2.1
    // the field is not a sound number
    let mut seen = std::collections::BTreeSet::new();
    if act.version < V2_1 {
        return;
    }
    for (_, _, f) in act
        .actions
        .iter()
        .enumerate()
        .flat_map(|(a, x)| x.frames.iter().enumerate().map(move |(n, f)| (a, n, f)))
    {
        let Ok(id) = usize::try_from(f.sound_id) else {
            continue;
        };
        let Some(raw) = act.sounds.get(id) else {
            add(
                "sound_out_of_range",
                format!(
                    "a frame plays sound {id}, the table has {}",
                    act.sounds.len()
                ),
            );
            continue;
        };
        // the name as the bytes of the file, not as text: Korean names are not Latin-1
        let name: Vec<u8> = raw.iter().copied().take_while(|&b| b != 0).collect();
        if name.is_empty() || name.eq_ignore_ascii_case(b"atk") || !seen.insert(name.clone()) {
            continue;
        }
        let mut p = b"data\\wav\\".to_vec();
        p.extend(&name);
        if !vfs.exists(&key(&p)) {
            add("missing_sound", shown_path(&p, names));
        }
    }
}

fn check_str(
    vfs: &Vfs,
    path: &[u8],
    data: &[u8],
    shown: &str,
    names: NameMode,
    out: &mut Vec<Problem>,
) {
    let s = match str_effect::parse(data) {
        Ok(s) => s,
        Err(e) => {
            out.push(Problem {
                kind: "unreadable_effect",
                file: shown.to_string(),
                detail: e.to_string(),
            });
            return;
        }
    };
    for name in str_effect::texture_names(&s) {
        // the client looks next to the effect, then in the shared effect folder
        let mut near = dir_of(path).to_vec();
        near.extend(&name);
        let mut shared = b"data\\texture\\effect\\".to_vec();
        shared.extend(&name);
        if !vfs.exists(&key(&near)) && !vfs.exists(&key(&shared)) {
            out.push(Problem {
                kind: "missing_effect_texture",
                file: shown.to_string(),
                detail: shown_path(&shared, names),
            });
        }
    }
}

/// Checks every animation and effect that wins in the client (or the one GRF) and reports
/// what they name that is not there. Exit 0 when nothing is wrong, 3 otherwise.
pub fn check(
    input: &Path,
    only: Option<&str>,
    limit: usize,
    all_files: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let vfs = Vfs::open(input)?;
    let wanted = |ext: &str| only.is_none_or(|o| o.eq_ignore_ascii_case(ext));
    // (archive, entry, path, shown) of the files that win
    let mut work = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (n, (_, a)) in vfs.archives.iter().enumerate() {
        for (key, idxs) in &vfs.keymaps[n] {
            let Some(&i) = idxs.last() else { continue };
            let e = &a.entries()[i];
            let raw = e.name.as_bytes();
            let ext = raw
                .rsplit(|&b| b == b'.')
                .next()
                .map(|x| x.to_ascii_lowercase());
            let kind = match ext.as_deref() {
                Some(b"act") if wanted("act") => 0,
                Some(b"str") if wanted("str") => 1,
                _ => continue,
            };
            if e.is_file() && seen.insert(key.clone()) {
                work.push((n, i, kind, raw.to_vec(), e.name.display(names)));
            }
        }
    }
    if work.is_empty() {
        bail!("no animations or effects found");
    }
    work.sort_by(|a, b| a.4.cmp(&b.4));
    let results: Vec<Vec<Problem>> = work
        .par_iter()
        .map(|(n, i, kind, raw, shown)| {
            let mut out = Vec::new();
            match vfs.read((*n, *i)) {
                Err(e) => out.push(Problem {
                    kind: "unreadable_entry",
                    file: shown.clone(),
                    detail: e.to_string(),
                }),
                Ok(data) if *kind == 0 => check_act(&vfs, raw, &data, shown, names, &mut out),
                Ok(data) => check_str(&vfs, raw, &data, shown, names, &mut out),
            }
            out
        })
        .collect();
    let acts = work.iter().filter(|w| w.2 == 0).count();
    let strs = work.len() - acts;
    let problems: Vec<&Problem> = results.iter().flatten().collect();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &problems {
        *by_kind.entry(p.kind).or_default() += 1;
    }
    let files_with: usize = results.iter().filter(|r| !r.is_empty()).count();
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "animations_checked": acts,
                "effects_checked": strs,
                "files_with_problems": files_with,
                "problems": problems.len(),
                "by_kind": by_kind,
                "list": problems.iter().take(limit).map(|p| json!({"kind": p.kind, "file": p.file, "detail": p.detail})).collect::<Vec<_>>(),
            })
        );
    } else {
        // a missing sprite usually hits a whole folder: count those per folder
        let grouped = |p: &&&Problem| !all_files && p.kind == "missing_sprite";
        let single: Vec<&&Problem> = problems.iter().filter(|p| !grouped(p)).collect();
        for p in single.iter().take(limit) {
            println!("{}: {} ({})", p.file, p.detail, p.kind.replace('_', " "));
        }
        if single.len() > limit {
            println!("... and {} more (use --limit)", single.len() - limit);
        }
        let mut folders: BTreeMap<&str, (usize, &str)> = BTreeMap::new();
        for p in problems.iter().filter(grouped) {
            let dir = p.file.rsplit_once('\\').map_or("", |d| d.0);
            folders.entry(dir).or_insert((0, p.file.as_str())).0 += 1;
        }
        let mut folders: Vec<_> = folders.into_iter().collect();
        folders.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
        for (dir, (n, example)) in folders.iter().take(limit) {
            println!("{dir}: {n} animations without their .spr, e.g. {example}");
        }
        if folders.len() > limit {
            println!(
                "... and {} more folders (use --limit, or --files for every file)",
                folders.len() - limit
            );
        }
        let kinds: Vec<String> = by_kind
            .iter()
            .map(|(k, n)| format!("{n} {}", k.replace('_', " ")))
            .collect();
        println!(
            "{acts} animations and {strs} effects checked: {} problems in {files_with} files{}",
            problems.len(),
            if kinds.is_empty() {
                String::new()
            } else {
                format!(" ({})", kinds.join(", "))
            }
        );
        println!(
            "Not checked: files named by Lua tables, and whether an existing file is valid beyond what is read."
        );
    }
    Ok(if problems.is_empty() { 0 } else { 3 })
}
