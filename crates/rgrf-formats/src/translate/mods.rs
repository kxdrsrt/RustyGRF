//! Updates your GRF files to the newest translation and add-ons and keeps your own edits (a
//! three-way merge per file).
//!
//! For every file in your unpacked GRF folders (`client-dev/grf/<name>/data`) that matches a file of
//! the project (translation layers or add-ons) by path, the closest HISTORICAL version of it in the
//! project's git history (only versions up to `built_before`) is found and the file is:
//!
//! - `same`: your file already equals the current project file: nothing to do,
//! - `update`: your file is an unmodified older version of the project file: the current one
//!   replaces it,
//! - `merged`: you changed it: yours plus (that older version to the current version), your lines
//!   win on conflict,
//! - `unrelated`: too far from every historical version (not derived from the project): NOT touched,
//! - `binary`: you changed a binary file: kept as is.
//!
//! The "current" version is the one of the SAME source as the historical version that matches best:
//! if your file came from `Addons/Navigation Legacy`, you get the newest Navigation Legacy file, not
//! the translation's (different) navigation file. Results go to `<out>/<name>/data/...` plus
//! `report.json`.
//!
//! Needs `git` and a full clone of ROenglishRE.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use unicode_normalization::UnicodeNormalization;

use super::{
    IoContext, Result, blob, blob_sha, bump, copy_file, dict_repr, fold_name, history_blobs, lf,
    make_dirs, msg, read, replace_all, walk, write,
};

fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// One historical version of a project file.
#[derive(Debug, Clone)]
struct Version {
    sha: String,
    date: String,
    path: String,
    source: String,
}

type Hist = HashMap<String, Vec<Version>>;

/// The data path a repo path stands for, with the source it is part of.
fn translation_key(path: &str) -> Option<String> {
    if let Some(rest) = path
        .strip_prefix("Translation/Renewal/")
        .or_else(|| path.strip_prefix("Translation/Pre-Renewal/"))
        && !rest.is_empty()
    {
        return Some(rest.to_string());
    }
    let rest = path.strip_prefix("Translation/Compatibility/")?;
    let (date, rest) = rest.split_once('/')?;
    if date.is_empty() {
        return None;
    }
    for sub in ["Renewal/", "Pre-Renewal/"] {
        if let Some(r) = rest.strip_prefix(sub)
            && !r.is_empty()
        {
            return Some(r.to_string());
        }
    }
    (!rest.is_empty()).then(|| rest.to_string())
}

/// `[(key, source)]` for a repo path: keys are lower-case data paths, or `*/<name>` for add-on
/// files that have no `data/` prefix (they only say "copy into the right folder").
fn keys_of(path: &str) -> Vec<(String, String)> {
    let path = nfc(path);
    if let Some(k) = translation_key(&path) {
        return vec![(k.to_lowercase(), "translation".to_string())];
    }
    if let Some(rest) = path.strip_prefix("Addons/")
        && let Some((addon, rest)) = rest.split_once('/')
        && !addon.is_empty()
        && !rest.is_empty()
    {
        let low = rest.to_lowercase();
        let source = format!("addon:{addon}");
        if low.starts_with("data/") {
            return vec![(low, source)];
        }
        let name = low.rsplit('/').next().unwrap_or("").to_string();
        return vec![(format!("*/{name}"), source)];
    }
    Vec::new()
}

fn history(repo: &Path) -> Result<Hist> {
    let mut hist: Hist = HashMap::new();
    for b in history_blobs(repo)? {
        for (k, source) in keys_of(&b.path) {
            let v = hist.entry(k).or_default();
            if !v.iter().any(|x| x.sha == b.sha) {
                v.push(Version {
                    sha: b.sha.clone(),
                    date: b.date.clone(),
                    path: b.path.clone(),
                    source,
                });
            }
        }
    }
    Ok(hist)
}

/// `dict.update`: new blobs are added, known ones take the new data and keep their place.
fn update_versions(into: &mut Vec<Version>, from: &[Version]) {
    for v in from {
        match into.iter_mut().find(|x| x.sha == v.sha) {
            Some(x) => *x = v.clone(),
            None => into.push(v.clone()),
        }
    }
}

const FAR: u64 = 1_000_000_000;

/// The number of added and removed lines `git diff` counts between two files.
fn changed_lines(a: &Path, b: &Path) -> u64 {
    let Ok(r) = Command::new("git")
        .args(["diff", "--no-index", "--numstat"])
        .arg(a)
        .arg(b)
        .output()
    else {
        return FAR;
    };
    let out = String::from_utf8_lossy(&r.stdout);
    let mut parts = out.splitn(3, '\t');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(add), Some(rem), Some(_)) => {
            match (add.trim().parse::<u64>(), rem.trim().parse::<u64>()) {
                (Ok(x), Ok(y)) => x + y,
                _ => FAR,
            }
        }
        _ => FAR,
    }
}

/// A value of a report entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Str(String),
    Int(i64),
}

/// One file of the report, fields in the order they are written.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub fields: Vec<(&'static str, Json)>,
}

impl Entry {
    fn new(archive: &str, path: &str) -> Self {
        Self {
            fields: vec![
                ("archive", Json::Str(archive.to_string())),
                ("path", Json::Str(path.to_string())),
            ],
        }
    }

    fn with(mut self, k: &'static str, v: Json) -> Self {
        self.fields.push((k, v));
        self
    }

    fn text(&self, key: &str) -> &str {
        self.fields
            .iter()
            .find_map(|(k, v)| match v {
                Json::Str(s) if *k == key => Some(s.as_str()),
                _ => None,
            })
            .unwrap_or("")
    }

    fn int(&self, key: &str) -> i64 {
        self.fields
            .iter()
            .find_map(|(k, v)| match v {
                Json::Int(n) if *k == key => Some(*n),
                _ => None,
            })
            .unwrap_or(0)
    }

    pub fn archive(&self) -> &str {
        self.text("archive")
    }

    pub fn path(&self) -> &str {
        self.text("path")
    }

    pub fn state(&self) -> &str {
        self.text("state")
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// `json.dump(report, indent=1, ensure_ascii=False)`.
fn report_json(report: &[Entry]) -> String {
    if report.is_empty() {
        return "[]".to_string();
    }
    let items: Vec<String> = report
        .iter()
        .map(|e| {
            let fields: Vec<String> = e
                .fields
                .iter()
                .map(|(k, v)| {
                    let v = match v {
                        Json::Str(s) => json_string(s),
                        Json::Int(n) => n.to_string(),
                    };
                    format!("  {}: {v}", json_string(k))
                })
                .collect();
            format!(" {{\n{}\n }}", fields.join(",\n"))
        })
        .collect();
    format!("[\n{}\n]", items.join(",\n"))
}

#[derive(Debug, Clone)]
pub struct Options {
    pub repo: PathBuf,
    pub client_dev: PathBuf,
    /// The output of `assemble` (its `grf/data` has the current translation files).
    pub asm: PathBuf,
    /// The output folder; it must not exist.
    pub out: PathBuf,
    pub archives: Vec<String>,
    /// The date your GRFs were last built; newer historical versions are ignored.
    pub built_before: String,
    /// The share of differing lines above which a file counts as unrelated to the project.
    pub max_ratio: f64,
    /// Data paths (e.g. `data/luafiles514/lua files/datainfo/petinfo.lub`) that are never touched.
    pub exclude: Vec<String>,
}

impl Options {
    pub fn new(repo: PathBuf, client_dev: PathBuf, asm: PathBuf, out: PathBuf) -> Self {
        Self {
            repo,
            client_dev,
            asm,
            out,
            archives: vec!["core".to_string(), "cdata".to_string()],
            built_before: "9999-12-31".to_string(),
            max_ratio: 0.5,
            exclude: Vec::new(),
        }
    }
}

/// The path of the current version of the project file that `sha` belongs to.
fn current_file(
    o: &Options,
    rel: &str,
    source: &str,
    versions: &[Version],
    sha: &str,
) -> Option<PathBuf> {
    if source == "translation" {
        let mut cur = o.asm.join("grf");
        for part in rel.split('/') {
            let n = fold_name(&cur, part);
            cur = cur.join(n);
        }
        return cur.is_file().then_some(cur);
    }
    // add-on: the file at the path of the matching historical version, as it is in the checkout
    let v = versions.iter().find(|v| v.sha == sha)?;
    let mut p = o.repo.clone();
    for part in v.path.split('/') {
        p.push(part);
    }
    p.is_file().then_some(p)
}

fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(8192)].contains(&0)
}

/// Runs the update; returns the report and what the script printed.
pub fn run(o: &Options) -> Result<(Vec<Entry>, String)> {
    if o.out.exists() {
        return msg(format!("{} exists", o.out.display()));
    }
    make_dirs(&o.out)?;
    let excluded: Vec<String> = o
        .exclude
        .iter()
        .map(|e| e.to_lowercase().replace('\\', "/"))
        .collect();
    let hist = history(&o.repo)?;
    let tmp = o.out.join(".mods-work");
    make_dirs(&tmp)?;
    let (base_f, user_f, best_f, theirs_f) = (
        tmp.join("base"),
        tmp.join("user"),
        tmp.join("best"),
        tmp.join("theirs"),
    );
    let mut report: Vec<Entry> = Vec::new();
    for name in &o.archives {
        let user_data = o.client_dev.join("grf").join(name).join("data");
        for (dir, mut files) in walk(&user_data) {
            files.sort();
            for f in files {
                let path = dir.join(&f);
                let rel_under = path
                    .strip_prefix(&user_data)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                let rel = nfc(&format!("data/{rel_under}"));
                let key = rel.to_lowercase();
                if excluded.contains(&key) {
                    continue;
                }
                let mut versions: Vec<Version> = Vec::new();
                for k in [key.clone(), format!("*/{}", nfc(&f).to_lowercase())] {
                    if let Some(v) = hist.get(&k) {
                        update_versions(&mut versions, v);
                    }
                }
                if versions.is_empty() {
                    continue;
                }
                let ub = read(&path)?;
                let ul = lf(&ub);
                let us = blob_sha(&ul);
                let entry = Entry::new(name, &rel);
                let mut dest = o.out.join(name);
                for part in rel.split('/') {
                    dest.push(part);
                }
                if is_binary(&ul) {
                    // binary: only an exact historical match is a safe update
                    if let Some(v) = versions.iter().find(|v| v.sha == us) {
                        let src = v.source.clone();
                        if let Some(cur) = current_file(o, &rel, &src, &versions, &us)
                            && read(&cur)? != ub
                        {
                            if let Some(d) = dest.parent() {
                                make_dirs(d)?;
                            }
                            copy_file(&cur, &dest)?;
                            report.push(
                                entry
                                    .with("state", Json::Str("update".into()))
                                    .with("source", Json::Str(src)),
                            );
                        }
                    } else {
                        report.push(entry.with("state", Json::Str("binary".into())).with(
                            "note",
                            Json::Str("differs from every historical version; kept as is".into()),
                        ));
                    }
                    continue;
                }
                // the closest historical version: the fewest changed lines, the first on a tie
                let mut best: Option<(u64, String, String, String)> = None;
                for v in &versions {
                    if v.date > o.built_before {
                        continue;
                    }
                    write(&base_f, &blob(&o.repo, &v.sha)?)?;
                    write(&user_f, &ul)?;
                    let n = if v.sha == us {
                        0
                    } else {
                        changed_lines(&base_f, &user_f)
                    };
                    if best.as_ref().is_none_or(|b| n < b.0) {
                        best = Some((n, v.sha.clone(), v.date.clone(), v.source.clone()));
                        copy_file(&base_f, &best_f)?;
                    }
                }
                let Some((n, sha, date, src)) = best else {
                    continue;
                };
                let total = ul.iter().filter(|&&b| b == b'\n').count().max(1);
                if n as f64 > o.max_ratio * total as f64 && n > 20 {
                    report.push(
                        entry
                            .with("state", Json::Str("unrelated".into()))
                            .with("source", Json::Str(src))
                            .with(
                                "note",
                                Json::Str(format!(
                                    "{n} of {total} lines differ from the closest historical version; kept as is"
                                )),
                            ),
                    );
                    continue;
                }
                let Some(cur) = current_file(o, &rel, &src, &versions, &sha) else {
                    continue;
                };
                let tb = read(&cur)?;
                let tl = lf(&tb);
                if blob_sha(&tl) == us {
                    continue; // already current (maybe only line endings differ)
                }
                let (merged, entry) = if n == 0 {
                    (
                        tb,
                        entry
                            .with("state", Json::Str("update".into()))
                            .with("source", Json::Str(src))
                            .with("base_date", Json::Str(date)),
                    )
                } else {
                    write(&user_f, &ul)?;
                    write(&theirs_f, &tl)?;
                    let run = |extra: &[&str]| {
                        Command::new("git")
                            .arg("merge-file")
                            .arg("-p")
                            .args(extra)
                            .arg(&user_f)
                            .arg(&best_f)
                            .arg(&theirs_f)
                            .output()
                            .ctx(|| "cannot run git".to_string())
                    };
                    let probe = run(&[])?;
                    let rc = probe.status.code().unwrap_or(-1);
                    let conflicts = if rc < 128 { rc } else { -1 };
                    let mut merged = run(&["--ours"])?.stdout;
                    let crlf = replace_all(&ub, b"\r\n", b"\n").1;
                    let lfs = ub.iter().filter(|&&b| b == b'\n').count();
                    if crlf * 2 > lfs {
                        merged = replace_all(&merged, b"\n", b"\r\n").0;
                    }
                    (
                        merged,
                        entry
                            .with("state", Json::Str("merged".into()))
                            .with("source", Json::Str(src))
                            .with("base_date", Json::Str(date))
                            .with("your_changed_lines", Json::Int(n as i64))
                            .with("conflicts", Json::Int(i64::from(conflicts))),
                    )
                };
                if let Some(d) = dest.parent() {
                    make_dirs(d)?;
                }
                write(&dest, &merged)?;
                report.push(entry);
            }
        }
    }
    let _ = fs::remove_dir_all(&tmp);
    write(&o.out.join("report.json"), report_json(&report).as_bytes())?;
    let mut counts: Vec<(String, usize)> = Vec::new();
    for e in &report {
        bump(&mut counts, e.state());
    }
    let mut printed = vec![dict_repr(&counts)];
    for e in &report {
        if e.state() == "merged" {
            printed.push(format!(
                "  merged     {:5} {} [{}] base {}, your changed lines {}, conflicts {}",
                e.archive(),
                e.path(),
                e.text("source"),
                e.text("base_date"),
                e.int("your_changed_lines"),
                e.int("conflicts")
            ));
        }
    }
    for e in &report {
        if matches!(e.state(), "binary" | "unrelated") {
            printed.push(format!(
                "  {:10} {:5} {}  {}",
                e.state(),
                e.archive(),
                e.path(),
                e.text("note")
            ));
        }
    }
    Ok((report, printed.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_paths_become_data_paths_with_their_source() {
        assert_eq!(
            keys_of("Translation/Pre-Renewal/data/A/B.lub"),
            [("data/a/b.lub".to_string(), "translation".to_string())]
        );
        assert_eq!(
            keys_of("Translation/Compatibility/2018-06-20/Renewal/data/x.txt"),
            [("data/x.txt".to_string(), "translation".to_string())]
        );
        assert_eq!(
            keys_of("Translation/Compatibility/2018-06-20/data/x.txt")[0].0,
            "data/x.txt"
        );
        assert_eq!(
            keys_of("Addons/Nav/data/Navi.lub"),
            [("data/navi.lub".to_string(), "addon:Nav".to_string())]
        );
        assert_eq!(
            keys_of("Addons/Nav/System/Navi_Data.lub"),
            [("*/navi_data.lub".to_string(), "addon:Nav".to_string())]
        );
        assert!(keys_of("README.md").is_empty());
    }

    #[test]
    fn the_report_is_written_like_python_json_dump() {
        let e = Entry::new("core", "data/a\"b.lub")
            .with("state", Json::Str("merged".into()))
            .with("conflicts", Json::Int(0));
        assert_eq!(
            report_json(&[e]),
            "[\n {\n  \"archive\": \"core\",\n  \"path\": \"data/a\\\"b.lub\",\n  \"state\": \"merged\",\n  \"conflicts\": 0\n }\n]"
        );
        assert_eq!(report_json(&[]), "[]");
    }
}
