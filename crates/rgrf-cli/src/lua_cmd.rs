//! `rgrf lua check`: Lua 5.1 syntax check of `.lub`/`.lua` files, loose or inside a GRF.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rgrf_core::{Archive, DecodeStatus, NameMode};
use rgrf_formats::lua;
use serde_json::json;

#[derive(PartialEq, Clone, Copy)]
enum Kind {
    Text,
    Compiled,
    /// Encrypted in the archive: cannot be read, so it is counted and skipped.
    Encrypted,
}

struct Outcome {
    name: String,
    kind: Kind,
    result: std::result::Result<(), lua::LuaError>,
}

/// Text is checked against the Lua 5.1 grammar, compiled files (5.0 and 5.1) structurally.
fn check_data(name: String, data: &[u8]) -> Outcome {
    if data.starts_with(b"\x1bLua") {
        let result = rgrf_formats::luac::check_bytecode(data)
            .map(|_| ())
            .map_err(|message| lua::LuaError { line: 0, message });
        return Outcome {
            name,
            kind: Kind::Compiled,
            result,
        };
    }
    Outcome {
        name,
        kind: Kind::Text,
        result: lua::check(data),
    }
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
                Ok(check_data(name, &data))
            })
            .collect::<Result<_>>()?
    } else if input.extension().is_some_and(|e| {
        ["grf", "gpf", "thor", "rgz"]
            .iter()
            .any(|x| e.eq_ignore_ascii_case(x))
    }) {
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
                    return Ok(Outcome {
                        name,
                        kind: Kind::Encrypted,
                        result: Ok(()),
                    });
                }
                Ok(check_data(name, &d.data))
            })
            .collect::<Result<_>>()?
    } else {
        let data = fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
        vec![check_data(input.display().to_string(), &data)]
    };

    let count = |k: Kind| outcomes.iter().filter(|o| o.kind == k).count();
    let (compiled, encrypted) = (count(Kind::Compiled), count(Kind::Encrypted));
    let mut failed: Vec<&Outcome> = outcomes.iter().filter(|o| o.result.is_err()).collect();
    failed.sort_by(|a, b| a.name.cmp(&b.name));
    let compiled_ok = outcomes
        .iter()
        .filter(|o| o.kind == Kind::Compiled && o.result.is_ok())
        .count();
    let valid = outcomes
        .iter()
        .filter(|o| o.kind == Kind::Text && o.result.is_ok())
        .count();

    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "checked": outcomes.len(),
                "valid": valid,
                "bytecode": compiled,
                "bytecode_valid": compiled_ok,
                "encrypted": encrypted,
                "failed": failed.iter().map(|o| {
                    let e = o.result.as_ref().unwrap_err();
                    json!({"file": o.name, "line": e.line, "message": e.message, "compiled": o.kind == Kind::Compiled})
                }).collect::<Vec<_>>(),
            })
        );
    } else {
        for o in &failed {
            let e = o.result.as_ref().unwrap_err();
            if o.kind == Kind::Compiled {
                println!("{}: compiled Lua is damaged: {}", o.name, e.message);
            } else {
                println!("{}: {e}", o.name);
            }
        }
        println!(
            "{} Lua files: {valid} valid text, {compiled_ok} of {compiled} compiled valid, {} with errors, {encrypted} encrypted (skipped)",
            outcomes.len(),
            failed.len()
        );
    }
    // exit code 3: the check found errors (same as `verify`)
    Ok(if failed.is_empty() { 0 } else { 3 })
}

/// `rgrf lua decompile`: source (or the bytecode listing) of a compiled Lua 5.0 or 5.1 file.
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
    // names and strings in the client's scripts are Korean text (CP949)
    let show = |b: &[u8]| GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    let chunk = luac::parse_any(&data).with_context(|| format!("{what} cannot be read"))?;
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

/// `rgrf lua compile`: Lua 5.1 source to a `.lub` in the layout of the client's own files.
pub fn compile(
    input: &Path,
    output: &Path,
    strip: bool,
    encoding: rgrf_formats::luacomp::TextEncoding,
    name: Option<&str>,
    overwrite: bool,
    json: bool,
) -> Result<i32> {
    use rgrf_core::GrfPath;
    use rgrf_formats::{luac, luacomp};
    let bytes = fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
    if bytes.starts_with(b"\x1bLua") {
        bail!(
            "{} is compiled Lua already; use `lua decompile` first",
            input.display()
        );
    }
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    let source = match name {
        Some(n) => n.as_bytes().to_vec(),
        None => format!(
            "@{}",
            input
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
        )
        .into_bytes(),
    };
    // the source file is read as UTF-8; a Latin-1 or CP949 file would fail here with a clear error
    let text = String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("{} is not valid UTF-8 text", input.display()))?;
    let cp949 =
        |run: &str| GrfPath::from_display(run, NameMode::Cp949).map(|p| p.as_bytes().to_vec());
    let mut chunk = luacomp::compile_text(&text, &source, encoding, &cp949)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    if strip {
        luacomp::strip(&mut chunk);
    }
    let lub = luac::dump32(&chunk);
    luac::check_bytecode(&lub)
        .map_err(|e| anyhow::anyhow!("internal error, the result is invalid: {e}"))?;
    let tmp = output.with_extension("lub.tmp");
    fs::write(&tmp, &lub).with_context(|| format!("cannot write {}", tmp.display()))?;
    if overwrite && output.exists() {
        fs::remove_file(output)?;
    }
    fs::rename(&tmp, output)?;
    fn count(f: &luac::Function) -> usize {
        1 + f.protos.iter().map(count).sum::<usize>()
    }
    let (functions, instructions) = (count(&chunk.main), chunk.main.code.len());
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "output": output.display().to_string(),
                "bytes": lub.len(),
                "functions": functions,
                "main_instructions": instructions,
                "stripped": strip,
            }))?
        );
    } else {
        println!(
            "Wrote {} ({} bytes, {functions} functions{})",
            output.display(),
            lub.len(),
            if strip { ", stripped" } else { "" }
        );
    }
    Ok(0)
}
