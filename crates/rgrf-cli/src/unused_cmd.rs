//! `rgrf client unused`: files in the archives that no map, animation, effect, text, Lua table or
//! program names (see [`rgrf_formats::unused`] for what that can and cannot tell).

use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::map_files::MapSource;
use rgrf_formats::unused::{Client, FileInfo, analyze, names_are_composed};
use serde_json::json;

use crate::map_cmd::Vfs;

struct View<'a> {
    vfs: &'a Vfs,
    files: Vec<FileInfo>,
    at: Vec<(usize, usize)>,
}

impl MapSource for View<'_> {
    fn read(&self, path: &[u8]) -> Option<Vec<u8>> {
        MapSource::read(self.vfs, path)
    }
    fn exists(&self, path: &[u8]) -> bool {
        MapSource::exists(self.vfs, path)
    }
}

impl Client for View<'_> {
    fn files(&self) -> &[FileInfo] {
        &self.files
    }
    fn read_file(&self, i: usize) -> Option<Vec<u8>> {
        self.vfs.read(self.at[i]).ok()
    }
}

pub fn unused(input: &Path, limit: usize, files: bool, names: NameMode, json: bool) -> Result<i32> {
    let vfs = Vfs::open(input)?;
    let mut view = View {
        vfs: &vfs,
        files: Vec::new(),
        at: Vec::new(),
    };
    for (n, (_, a)) in vfs.archives.iter().enumerate() {
        for idxs in vfs.keymaps[n].values() {
            let Some(&i) = idxs.last() else { continue };
            let e = &a.entries()[i];
            if e.is_file() && vfs.find(&e.name.key()) == Some((n, i)) {
                view.files.push(FileInfo {
                    path: e.name.as_bytes().to_vec(),
                    zsize: u64::from(e.zsize),
                });
                view.at.push((n, i));
            }
        }
    }
    if view.files.is_empty() {
        bail!("no files found");
    }
    let started = std::time::Instant::now();
    // the programs and settings next to the archives name files too
    let mut extra = Vec::new();
    if input.is_dir() {
        for dir in [input.to_path_buf(), input.join("System")] {
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for f in rd.flatten() {
                let p = f.path();
                let ext = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_ascii_lowercase)
                    .unwrap_or_default();
                if matches!(
                    ext.as_str(),
                    "exe" | "dll" | "ini" | "xml" | "lua" | "lub" | "txt"
                ) && let Ok(d) = fs::read(&p)
                {
                    extra.push(d);
                }
            }
        }
    }
    let r = analyze(&view, &extra);
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(names);
    let mib = |b: u64| b as f64 / 1_048_576.0;
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "files_looked_at": r.files_looked_at,
                "unnamed_files": r.unnamed,
                "unnamed_stored_bytes": r.unnamed_bytes,
                "unnamed_in_map_folders": r.unnamed_reliable,
                "unnamed_in_map_folders_stored_bytes": r.unnamed_reliable_bytes,
                "names_found": r.names_found,
                "programs_scanned": extra.len(),
                "folders": r.folders.iter().take(limit).map(|f| json!({"folder": show(&f.name), "names_are_composed": names_are_composed(&f.name), "files": f.files, "unnamed": f.unnamed, "unnamed_stored_bytes": f.unnamed_bytes, "examples": f.examples.iter().map(|e| show(e)).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                "list": if files { r.listed.iter().take(limit).map(|(n, z)| json!({"file": show(n), "stored_bytes": z})).collect::<Vec<_>>() } else { vec![] },
            })
        );
        return Ok(0);
    }
    println!(
        "{} names found in text, Lua, animations, effects, maps and {} programs/settings ({:.1}s)",
        r.names_found,
        extra.len(),
        started.elapsed().as_secs_f64()
    );
    println!(
        "{} files in textures, sprites, sounds and models looked at; {} are named by nothing ({:.1} MiB stored)\n",
        r.files_looked_at,
        r.unnamed,
        mib(r.unnamed_bytes)
    );
    println!(
        "Folders of maps (marked *): the names are written in maps and models, so this is reliable: {} files, {:.1} MiB. Other folders (sprites, effects, interface, sounds) are named by tables and program code in pieces: mostly false alarms.\n",
        r.unnamed_reliable,
        mib(r.unnamed_reliable_bytes)
    );
    for f in r.folders.iter().take(limit) {
        let example = show(f.examples.first().map_or(&[][..], |e| e));
        println!(
            "{:>8.1} MiB {} {:>6}/{:<6} {}   e.g. {}",
            mib(f.unnamed_bytes),
            if names_are_composed(&f.name) {
                " "
            } else {
                "*"
            },
            f.unnamed,
            f.files,
            show(&f.name),
            example.rsplit('\\').next().unwrap_or(&example)
        );
    }
    if r.folders.len() > limit {
        println!(
            "... and {} more folders (use --limit)",
            r.folders.len() - limit
        );
    }
    if files {
        println!();
        for (n, z) in r.listed.iter().take(limit) {
            println!("{z:>10}  {}", show(n));
        }
    }
    println!(
        "\nNames put together while the program runs cannot be seen, so these files are only probably unused. Check a folder in the game before leaving its files out."
    );
    Ok(0)
}
