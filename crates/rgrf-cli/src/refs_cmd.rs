//! `rgrf client refs`: the files animations (.act), effects (.str) and Lua tables name, checked
//! against the client's archives before the client finds them missing ("Resource File Loading
//! fail").

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, bail};
use rayon::prelude::*;
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::{refs, refs_tables};
use serde_json::json;

use crate::map_cmd::Vfs;

/// One problem of a file, ready to print.
struct Problem {
    kind: &'static str,
    file: String,
    detail: String,
}

fn shown(p: &refs::Problem, file: &str, names: NameMode) -> Problem {
    let detail = match (p.path.is_empty(), p.note.is_empty()) {
        (false, _) => GrfPath::new(p.path.clone()).display(names),
        (true, _) => p.note.clone(),
    };
    Problem {
        kind: p.kind,
        file: file.to_string(),
        detail: if !p.path.is_empty() && !p.note.is_empty() {
            format!("{detail}: {}", p.note)
        } else {
            detail
        },
    }
}

/// Checks every animation and effect that wins in the client (or the one GRF), and the Lua tables
/// that have a rule (`only` `lua`: only those), and reports what they name that is not there.
/// Exit 0 when nothing is wrong, 3 otherwise.
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
    let tables = wanted("lua").then(|| refs_tables::check_tables(&vfs));
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
    if work.is_empty() && tables.as_ref().is_none_or(|t| t.files == 0) {
        bail!("no animations, effects or Lua tables found");
    }
    work.sort_by(|a, b| a.4.cmp(&b.4));
    let results: Vec<Vec<Problem>> = work
        .par_iter()
        .map(|(n, i, kind, raw, file)| match vfs.read((*n, *i)) {
            Err(e) => vec![Problem {
                kind: "unreadable_entry",
                file: file.clone(),
                detail: e.to_string(),
            }],
            Ok(data) => {
                let found = if *kind == 0 {
                    refs::check_act(&vfs, raw, &data)
                } else {
                    refs::check_str(&vfs, raw, &data)
                };
                found.iter().map(|p| shown(p, file, names)).collect()
            }
        })
        .collect();
    let acts = work.iter().filter(|w| w.2 == 0).count();
    let strs = work.len() - acts;
    let problems: Vec<&Problem> = results.iter().flatten().collect();
    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &problems {
        *by_kind.entry(p.kind).or_default() += 1;
    }
    // the table problems, by rule and then by missing file
    let mut table_problems: Vec<Problem> = Vec::new();
    let mut table_kinds: BTreeMap<&str, usize> = BTreeMap::new();
    if let Some(t) = &tables {
        let mut list: Vec<&refs_tables::TableProblem> = t.problems.iter().collect();
        list.sort_by(|a, b| {
            (a.table, a.problem.kind, &a.problem.path).cmp(&(
                b.table,
                b.problem.kind,
                &b.problem.path,
            ))
        });
        for p in list {
            *table_kinds.entry(p.problem.kind).or_default() += 1;
            let file = GrfPath::new(p.file.clone()).display(names);
            table_problems.push(shown(&p.problem, &file, names));
        }
        for u in &t.unreadable {
            table_problems.push(Problem {
                kind: "unreadable_table",
                file: GrfPath::new(u.clone()).display(names),
                detail: "compiled Lua that cannot be read".to_string(),
            });
        }
    }
    let total_problems = problems.len() + table_problems.len();
    let files_with: usize = results.iter().filter(|r| !r.is_empty()).count();
    let table_files = table_problems
        .iter()
        .map(|p| &p.file)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "animations_checked": acts,
                "effects_checked": strs,
                "files_with_problems": files_with + table_files,
                "tables_checked": tables.as_ref().map(|t| t.files),
                "table_names_checked": tables.as_ref().map(|t| t.names),
                "table_names_skipped_utf8": tables.as_ref().map(|t| t.skipped_utf8),
                "problems": total_problems,
                "by_kind": by_kind,
                "table_problems": table_problems.len(),
                "table_by_kind": table_kinds,
                "list": problems.iter().take(limit).map(|p| json!({"kind": p.kind, "file": p.file, "detail": p.detail})).collect::<Vec<_>>(),
                "table_list": table_problems.iter().take(limit).map(|p| json!({"kind": p.kind, "file": p.file, "detail": p.detail})).collect::<Vec<_>>(),
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
        for p in table_problems.iter().take(limit) {
            println!("{}: {} ({})", p.file, p.detail, p.kind.replace('_', " "));
        }
        if table_problems.len() > limit {
            println!(
                "... and {} more from Lua tables (use --limit)",
                table_problems.len() - limit
            );
        }
        let kinds: Vec<String> = by_kind
            .iter()
            .chain(table_kinds.iter())
            .map(|(k, n)| format!("{n} {}", k.replace('_', " ")))
            .collect();
        let tables_note = tables.as_ref().map_or(String::new(), |t| {
            format!(
                " and {} Lua tables ({} names{})",
                t.files,
                t.names,
                if t.skipped_utf8 > 0 {
                    format!(", {} in UTF-8 scripts left out", t.skipped_utf8)
                } else {
                    String::new()
                }
            )
        });
        println!(
            "{acts} animations and {strs} effects{tables_note} checked: {total_problems} problems in {} files{}",
            files_with + table_files,
            if kinds.is_empty() {
                String::new()
            } else {
                format!(" ({})", kinds.join(", "))
            }
        );
        println!(
            "Not checked: names a table gets from other tables or builds at run time (e.g. weapon sprites), and whether an existing file is valid beyond what is read."
        );
    }
    Ok(if total_problems == 0 { 0 } else { 3 })
}
