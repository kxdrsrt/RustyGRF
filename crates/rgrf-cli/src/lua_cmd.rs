//! `rgrf lua check`: Lua 5.1 syntax check of `.lub`/`.lua` files, loose or inside a GRF.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rgrf_core::{Archive, DecodeStatus, NameMode};
use rgrf_formats::lua;
use serde_json::json;

struct Outcome {
    name: String,
    result: std::result::Result<(), lua::LuaError>,
}

fn is_lua_name(n: &str) -> bool {
    let n = n.to_ascii_lowercase();
    n.ends_with(".lub") || n.ends_with(".lua")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(&p, out)?;
        } else if p.to_str().is_some_and(is_lua_name) {
            out.push(p);
        }
    }
    Ok(())
}

pub fn check(input: &Path, pattern: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let outcomes: Vec<Outcome> = if input.is_dir() {
        let mut files = Vec::new();
        walk(input, &mut files)?;
        files
            .par_iter()
            .map(|p| {
                let name = p.display().to_string();
                let data = fs::read(p).with_context(|| format!("cannot read {name}"))?;
                Ok(Outcome {
                    name,
                    result: lua::check(&data),
                })
            })
            .collect::<Result<_>>()?
    } else if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("grf"))
    {
        let a = Archive::open(input)?;
        let idx: Vec<usize> = a
            .select(pattern, names)
            .into_iter()
            .filter(|&i| is_lua_name(&a.entries()[i].name.display(names)))
            .collect();
        idx.par_iter()
            .map(|&i| {
                let name = a.entries()[i].name.display(names);
                let d = a.read(i)?;
                if d.status == DecodeStatus::Encrypted {
                    bail!("{name} is encrypted; decryption is not supported yet");
                }
                Ok(Outcome {
                    result: lua::check(&d.data),
                    name,
                })
            })
            .collect::<Result<_>>()?
    } else {
        let data = fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
        vec![Outcome {
            name: input.display().to_string(),
            result: lua::check(&data),
        }]
    };

    let is_bytecode = |o: &Outcome| {
        o.result
            .as_ref()
            .err()
            .is_some_and(|e| e.message.contains("bytecode"))
    };
    let bytecode = outcomes.iter().filter(|o| is_bytecode(o)).count();
    let mut failed: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| o.result.is_err() && !is_bytecode(o))
        .collect();
    failed.sort_by(|a, b| a.name.cmp(&b.name));
    let valid = outcomes.len() - bytecode - failed.len();

    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "checked": outcomes.len(),
                "valid": valid,
                "bytecode": bytecode,
                "failed": failed.iter().map(|o| {
                    let e = o.result.as_ref().unwrap_err();
                    json!({"file": o.name, "line": e.line, "message": e.message})
                }).collect::<Vec<_>>(),
            })
        );
    } else {
        for o in &failed {
            println!("{}: {}", o.name, o.result.as_ref().unwrap_err());
        }
        println!(
            "{} Lua files: {valid} valid, {} with errors, {bytecode} compiled (not checked)",
            outcomes.len(),
            failed.len()
        );
    }
    // exit code 3: the check found errors (same as `verify`)
    Ok(if failed.is_empty() { 0 } else { 3 })
}

/// `rgrf lua decompile`: source (or the bytecode listing) of a compiled Lua 5.1 file.
pub fn decompile(
    input: &Path,
    entry: Option<&str>,
    output: Option<&Path>,
    listing: bool,
    names: NameMode,
) -> Result<i32> {
    use rgrf_core::GrfPath;
    use rgrf_formats::{luac, luadec};
    let (data, what) = crate::spr_cmd::load(input, entry, names)?;
    if !data.starts_with(b"\x1bLua") {
        bail!("{what} is not compiled Lua (it is plain text already)");
    }
    let chunk = luac::parse(&data).with_context(|| format!("{what} cannot be read"))?;
    // names and strings in the client's scripts are Korean text (CP949)
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    let (text, complete) = if listing {
        (luac::disassemble(&chunk, &show), true)
    } else {
        let t = luadec::decompile(&chunk, &show);
        let c = luadec::is_complete(&t);
        (t, c)
    };
    match output {
        Some(p) => {
            if p.exists() {
                bail!("{} exists already; remove it first", p.display());
            }
            fs::write(p, &text).with_context(|| format!("cannot write {}", p.display()))?;
            eprintln!(
                "wrote {}{}",
                p.display(),
                if complete { "" } else { " (partly decompiled)" }
            );
        }
        None => print!("{text}"),
    }
    Ok(if complete { 0 } else { 2 })
}
