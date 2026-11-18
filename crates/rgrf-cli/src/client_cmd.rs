//! `rgrf client check`: reads a client folder like the client does and reports what could go wrong
//! before you start it: `DATA.INI` and its archives, files that hide other files, loose files the
//! client ignores, conflicting server addresses, missing folders, broken Lua.
//!
//! Rules used (measured on the 2018-06-20 client, see docs/03-roadmap.md): in `DATA.INI` the lowest
//! index wins, and archives win over loose files in the `data` folder.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rgrf_core::codec::Codec;
use rgrf_core::{Archive, GrfPath, NameMode};
use rgrf_formats::lua;
use serde_json::json;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warning",
            Self::Error => "error",
        }
    }
}

struct Finding {
    level: Level,
    message: String,
}

struct Report {
    findings: Vec<Finding>,
}

impl Report {
    fn add(&mut self, level: Level, message: impl Into<String>) {
        self.findings.push(Finding {
            level,
            message: message.into(),
        });
    }
}

/// Entry of `DATA.INI`.
struct IniEntry {
    index: u32,
    file: String,
}

fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            e.file_name()
                .to_str()
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                .then(|| e.path())
        })
}

fn parse_ini(text: &str) -> Vec<IniEntry> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with([';', '[']) || line.starts_with("//") {
            continue;
        }
        if let Some((k, v)) = line.split_once('=')
            && let Ok(index) = k.trim().parse::<u32>()
        {
            out.push(IniEntry {
                index,
                file: v.trim().to_owned(),
            });
        }
    }
    out.sort_by_key(|e| e.index);
    out
}

fn tag(text: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&format!("</{name}>"))? + start;
    Some(text[start..end].trim().to_owned())
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// "Name address:port; Name address:port" of the `<connection>` blocks that have an address.
fn connections(xml: &str) -> String {
    let mut out = Vec::new();
    for block in xml.split("<connection>").skip(1) {
        let block = block.split("</connection>").next().unwrap_or(block);
        let (Some(addr), Some(port)) = (tag(block, "address"), tag(block, "port")) else {
            continue;
        };
        let name = tag(block, "balloon").unwrap_or_else(|| "?".into());
        out.push(format!("{name} {addr}:{port}"));
    }
    if out.is_empty() {
        "no connection".into()
    } else {
        out.join("; ")
    }
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, base, out);
        } else if let Ok(rel) = p.strip_prefix(base) {
            let rel: String = rel.to_string_lossy().replace('/', "\\").nfc().collect();
            out.push((rel, p));
        }
    }
}

struct Loaded {
    file: String,
    arc: Archive,
}

pub fn check(dir: &Path, names: NameMode, json: bool, check_lua: bool) -> Result<i32> {
    if !dir.is_dir() {
        bail!("{} is not a folder", dir.display());
    }
    let mut rep = Report {
        findings: Vec::new(),
    };
    let Some(ini_path) = find_ci(dir, "DATA.INI") else {
        bail!(
            "no DATA.INI in {} (is this a client folder?)",
            dir.display()
        );
    };
    let ini_text: String = fs::read(&ini_path)
        .map(|b| b.iter().map(|&c| char::from(c)).collect())
        .with_context(|| format!("cannot read {}", ini_path.display()))?;
    let ini = parse_ini(&ini_text);
    if ini.is_empty() {
        rep.add(
            Level::Error,
            "DATA.INI lists no archives (expected lines like 1=data.grf under [Data])",
        );
    }
    for w in ini.windows(2) {
        if w[0].index == w[1].index {
            rep.add(
                Level::Warn,
                format!(
                    "DATA.INI: index {} is used twice ({} and {}); which one wins is not measured",
                    w[0].index, w[0].file, w[1].file
                ),
            );
        }
    }
    if ini.first().is_some_and(|e| e.index == 0) {
        rep.add(Level::Info, "DATA.INI starts at index 0; whether the client reads index 0 is not measured (the tests used 1 and up)");
    }

    // Open the archives in priority order.
    let mut loaded: Vec<Loaded> = Vec::new();
    for e in &ini {
        let Some(path) = find_ci(dir, &e.file) else {
            rep.add(
                Level::Error,
                format!(
                    "DATA.INI {}={} but the file does not exist",
                    e.index, e.file
                ),
            );
            continue;
        };
        match Archive::open(&path) {
            Ok(arc) => loaded.push(Loaded {
                file: e.file.clone(),
                arc,
            }),
            Err(err) => rep.add(Level::Error, format!("{} cannot be opened: {err}", e.file)),
        }
    }
    // Archives lying around but not listed.
    if let Ok(rd) = fs::read_dir(dir) {
        let listed: Vec<String> = ini.iter().map(|e| e.file.to_ascii_lowercase()).collect();
        let mut extra: Vec<String> = rd
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| {
                let l = n.to_ascii_lowercase();
                (l.ends_with(".grf") || l.ends_with(".gpf")) && !listed.contains(&l)
            })
            .collect();
        extra.sort();
        for n in extra {
            rep.add(
                Level::Info,
                format!("{n} is in the folder but not in DATA.INI, so the client does not read it"),
            );
        }
    }

    // Archive table and codec counts.
    let mut table = Vec::new();
    let mut any_lzma = false;
    for (n, l) in loaded.iter().enumerate() {
        let a = &l.arc;
        let (mut zlib, mut lzma) = (0usize, 0usize);
        for i in 0..a.len() {
            match a.sniff_codec(i) {
                Ok(Some(Codec::Zlib)) => zlib += 1,
                Ok(Some(Codec::Lzma)) => lzma += 1,
                _ => {}
            }
        }
        any_lzma |= lzma > 0;
        table.push(json!({
            "priority": n + 1,
            "file": l.file,
            "format": format!("0x{:03x}", a.header().version),
            "entries": a.len(),
            "zlib": zlib,
            "lzma": lzma,
        }));
    }
    if any_lzma {
        match find_ci(dir, "cps.dll") {
            None => rep.add(Level::Error, "archives hold LZMA entries but there is no cps.dll; the client cannot read them"),
            Some(p) => rep.add(
                Level::Info,
                format!("archives hold LZMA entries; cps.dll is present ({} bytes), whether it can read LZMA is not checked", p.metadata().map(|m| m.len()).unwrap_or(0)),
            ),
        }
    }

    // Which file wins where: the first archive in DATA.INI order.
    let mut winner: HashMap<Vec<u8>, (usize, usize)> = HashMap::new();
    let keymaps: Vec<HashMap<Vec<u8>, Vec<usize>>> =
        loaded.iter().map(|l| l.arc.index_by_key()).collect();
    for (n, l) in loaded.iter().enumerate() {
        let (mut hidden, mut hidden_diff, mut own) = (0usize, 0usize, 0usize);
        for (key, idxs) in keymaps[n].iter() {
            let Some(&i) = idxs.last() else { continue };
            if !l.arc.entries()[i].is_file() {
                continue;
            }
            own += 1;
            match winner.get(key) {
                Some(&(wn, wi)) => {
                    hidden += 1;
                    if loaded[wn].arc.entries()[wi].size != l.arc.entries()[i].size {
                        hidden_diff += 1;
                    }
                }
                None => {
                    winner.insert(key.clone(), (n, i));
                }
            }
        }
        if hidden > 0 {
            rep.add(
                Level::Info,
                format!(
                    "{}: {hidden} of {own} files are hidden by archives with a lower index ({hidden_diff} of them with a different size)",
                    l.file
                ),
            );
        }
    }

    // Loose files in the data folder.
    let mut loose: Vec<(String, PathBuf)> = Vec::new();
    if let Some(d) = find_ci(dir, "data")
        && d.is_dir()
    {
        walk(&d, dir, &mut loose);
    }
    let mut ignored = Vec::new();
    let mut used = 0usize;
    let mut unmapped = 0usize;
    for (rel, _) in &loose {
        match GrfPath::from_display(rel, NameMode::Cp1252) {
            None => unmapped += 1,
            Some(p) => match winner.get(&p.key()) {
                Some(&(n, _)) => ignored.push((rel.clone(), loaded[n].file.clone())),
                None => used += 1,
            },
        }
    }
    if !loose.is_empty() {
        rep.add(
            Level::Info,
            format!("data folder: {} loose files, {used} used, {} ignored because an archive has the same path{}", loose.len(), ignored.len(),
                if unmapped > 0 { format!(", {unmapped} with names that cannot be compared") } else { String::new() }),
        );
    }
    if !ignored.is_empty() {
        let shown: Vec<String> = ignored
            .iter()
            .take(10)
            .map(|(f, a)| format!("{f} (hidden by {a})"))
            .collect();
        rep.add(
            Level::Warn,
            format!(
                "{} loose files are ignored, the archive version wins (measured on the 2018-06-20 client; with 'read data folder first' it would be the other way round): {}{}",
                ignored.len(),
                shown.join("; "),
                if ignored.len() > 10 { "; ..." } else { "" }
            ),
        );
    }

    // Servers: every copy of clientinfo/sclientinfo lists its connections; the first copy counts.
    for file in ["sclientinfo.xml", "clientinfo.xml"] {
        let key = GrfPath::from_display(&format!("data\\{file}"), NameMode::Cp1252)
            .map(|p| p.key())
            .unwrap_or_default();
        let mut copies: Vec<(String, String)> = Vec::new(); // (source, "name address:port; ...")
        for (n, l) in loaded.iter().enumerate() {
            if let Some(&i) = keymaps[n].get(&key).and_then(|v| v.last()) {
                let d = l.arc.read(i)?;
                copies.push((l.file.clone(), connections(&latin1(&d.data))));
            }
        }
        for (rel, p) in &loose {
            if rel.eq_ignore_ascii_case(&format!("data\\{file}")) {
                copies.push((
                    "data folder (loose)".into(),
                    connections(&latin1(&fs::read(p)?)),
                ));
            }
        }
        if let Some((src, list)) = copies.first() {
            rep.add(Level::Info, format!("{file} (used: {src}): {list}"));
            let others: Vec<_> = copies.iter().skip(1).filter(|(_, a)| a != list).collect();
            if !others.is_empty() {
                let shown: Vec<String> = others.iter().map(|(s, a)| format!("{s}: {a}")).collect();
                rep.add(
                    Level::Warn,
                    format!("{file}: other copies list other servers ({}); harmless now, but they win if the order in DATA.INI changes", shown.join(" | ")),
                );
            }
        }
    }

    // Folders the executable asks for.
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.filter_map(|e| e.ok()) {
            let p = e.path();
            if !p.extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")) {
                continue;
            }
            let Ok(bytes) = fs::read(&p) else { continue };
            let wants = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
            if wants(b"SystemEN") && find_ci(dir, "SystemEN").is_none() {
                rep.add(
                    Level::Warn,
                    format!("{} mentions SystemEN but there is no SystemEN folder next to it (needed by the translation, otherwise Lua errors at start)", e.file_name().to_string_lossy()),
                );
            }
        }
    }

    // Lua syntax of the files the client will really load.
    if check_lua {
        let is_lua = |n: &str| {
            let n = n.to_ascii_lowercase();
            n.ends_with(".lub") || n.ends_with(".lua")
        };
        let mut checked = 0usize;
        let mut bytecode = 0usize;
        let mut errors = Vec::new();
        for &(wn, wi) in winner.values() {
            let l = &loaded[wn];
            let name = l.arc.entries()[wi].name.display(names);
            if !is_lua(&name) {
                continue;
            }
            let d = l.arc.read(wi)?;
            match lua::check(&d.data) {
                Ok(()) => checked += 1,
                Err(e) if e.message.contains("bytecode") => bytecode += 1,
                Err(e) => errors.push(format!("{name} (in {}): {e}", l.file)),
            }
        }
        errors.sort();
        for (rel, p) in &loose {
            if is_lua(rel)
                && GrfPath::from_display(rel, NameMode::Cp1252)
                    .is_some_and(|k| !winner.contains_key(&k.key()))
            {
                match lua::check(&fs::read(p)?) {
                    Ok(()) => checked += 1,
                    Err(e) if e.message.contains("bytecode") => bytecode += 1,
                    Err(e) => errors.push(format!("{rel} (loose): {e}")),
                }
            }
        }
        rep.add(Level::Info, format!("Lua: {checked} files valid, {bytecode} compiled (not checked), {} with syntax errors", errors.len()));
        for e in errors {
            rep.add(Level::Error, format!("Lua syntax: {e}"));
        }
    }

    let worst = rep
        .findings
        .iter()
        .map(|f| f.level)
        .max()
        .unwrap_or(Level::Info);
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "folder": dir.display().to_string(),
                "archives": table,
                "findings": rep.findings.iter().map(|f| json!({"level": f.level.name(), "message": f.message})).collect::<Vec<_>>(),
            })
        );
    } else {
        println!("Client folder: {}", dir.display());
        println!("DATA.INI (the lowest index wins):");
        for t in &table {
            println!(
                "  {}={}  format {}, {} entries ({} zlib, {} LZMA)",
                t["priority"],
                t["file"].as_str().unwrap_or(""),
                t["format"].as_str().unwrap_or(""),
                t["entries"],
                t["zlib"],
                t["lzma"]
            );
        }
        for lvl in [Level::Error, Level::Warn, Level::Info] {
            for f in rep.findings.iter().filter(|f| f.level == lvl) {
                println!("{}: {}", lvl.name(), f.message);
            }
        }
        println!(
            "Result: {}",
            match worst {
                Level::Error => "errors found",
                Level::Warn => "warnings only",
                Level::Info => "nothing to report",
            }
        );
    }
    // 3: errors found, 2: warnings only (same codes as `verify`)
    Ok(match worst {
        Level::Error => 3,
        Level::Warn => 2,
        Level::Info => 0,
    })
}
