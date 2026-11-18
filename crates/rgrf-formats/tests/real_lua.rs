//! Every `.lub`/`.lua` of the real archives must pass the Lua 5.1 check (the client loads them).
//! Needs `RGRF_TESTDATA`, see rgrf-core's golden tests. Compiled files (bytecode) are counted apart.

use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::lua;

const ARCHIVES: [&str; 4] = [
    "client-dev/full/core.grf",
    "client-dev/full/cdata.grf",
    "0 OTHER CLIENTS/client-kro/data.grf",
    "0 GRFs/reference/core-before-translation-2026-11-17.grf",
];

fn base() -> Option<PathBuf> {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        assert!(
            std::env::var_os("RGRF_REQUIRE_TESTDATA").is_none(),
            "RGRF_TESTDATA is not set"
        );
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return None;
    };
    Some(PathBuf::from(base))
}

#[test]
fn every_lua_file_in_the_real_archives_passes() {
    let Some(base) = base() else { return };
    let failures = Mutex::new(Vec::<String>::new());
    for rel in ARCHIVES {
        let path = base.join(rel);
        assert!(path.exists(), "missing test archive {}", path.display());
        let arc = Archive::open(&path).unwrap();
        let mut idx = arc.select(Some("*.lub"), NameMode::Cp1252);
        idx.extend(arc.select(Some("*.lua"), NameMode::Cp1252));
        let counts = Mutex::new((0usize, 0usize, 0usize)); // text ok, bytecode, failed
        idx.par_iter().for_each(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let data = arc.read(i).unwrap().data;
            let mut c = counts.lock().unwrap();
            match lua::check(&data) {
                Ok(()) => c.0 += 1,
                Err(e) if e.message.contains("bytecode") => c.1 += 1,
                Err(e) => {
                    c.2 += 1;
                    failures.lock().unwrap().push(format!("{rel}: {name}: {e}"));
                }
            }
        });
        let c = counts.into_inner().unwrap();
        eprintln!(
            "{rel}: {} Lua files, {} valid text, {} bytecode, {} failed",
            idx.len(),
            c.0,
            c.1,
            c.2
        );
    }
    let mut f = failures.into_inner().unwrap();
    f.sort();
    assert!(
        f.is_empty(),
        "{} files fail:\n{}",
        f.len(),
        f[..f.len().min(15)].join("\n")
    );
}

/// A checker that accepts everything would pass the test above, so break real files and expect
/// the break to be found: remove the last `}` (or, without one, the last `end`).
#[test]
fn damaged_real_lua_files_are_detected() {
    let Some(base) = base() else { return };
    let arc = Archive::open(base.join("client-dev/full/core.grf")).unwrap();
    let idx = arc.select(Some("*.lub"), NameMode::Cp1252);
    let (mut tried, mut found) = (0usize, 0usize);
    for i in idx {
        let data = arc.read(i).unwrap().data;
        let pos = data
            .iter()
            .rposition(|&b| b == b'}')
            .or_else(|| data.windows(3).rposition(|w| w == b"end"));
        let Some(pos) = pos else { continue };
        let mut broken = data.clone();
        broken.remove(pos);
        tried += 1;
        found += usize::from(lua::check(&broken).is_err());
    }
    eprintln!("damaged copies: {tried}, detected: {found}");
    assert!(tried > 300);
    // a few last braces sit in comments or strings, where removing them changes nothing
    assert!(
        found * 100 >= tried * 95,
        "only {found} of {tried} detected"
    );
}

/// Every compiled `.lub` of the real archive is read (Lua 5.1) or recognised (Lua 5.0); the 5.1 ones
/// are decompiled and the result must be valid Lua source. Counts show how much is complete.
#[test]
fn every_compiled_lub_is_read_and_decompiled() {
    use rgrf_formats::luac::parse_any;
    use rgrf_formats::luadec::{decompile, is_complete};
    let Some(base) = base() else { return };
    let path = base.join("0 OTHER CLIENTS/client-kro/data.grf");
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.lub"), NameMode::Cp1252);
    let names = |b: &[u8]| rgrf_core::GrfPath::new(b.to_vec()).display(NameMode::Cp949);
    let (mut compiled, mut v50, mut complete, mut partial, mut functions_total) = (0, 0, 0, 0, 0);
    let (mut bad_syntax, mut bad_parse) = (Vec::new(), Vec::new());
    let mut sample = String::new();
    let mut reasons = std::collections::BTreeMap::<String, usize>::new();
    let mut partial_samples: Vec<String> = Vec::new();
    for &i in &idx {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        if !data.starts_with(b"\x1bLua") {
            continue;
        }
        compiled += 1;
        let chunk = match parse_any(&data) {
            Ok(c) => c,
            Err(e) => {
                bad_parse.push(format!("{name}: {e}"));
                continue;
            }
        };
        v50 += usize::from(chunk.main.orig_code.is_some());
        let text = decompile(&chunk, &names);
        functions_total += text.matches("function(").count();
        for l in text.lines().filter(|l| l.contains("-- not decompiled (")) {
            let why = l
                .split("-- not decompiled (")
                .nth(1)
                .unwrap_or("")
                .split(')')
                .next()
                .unwrap_or("")
                .to_string();
            *reasons.entry(why).or_insert(0usize) += 1;
        }
        if is_complete(&text) {
            complete += 1;
            if sample.is_empty() && text.len() < 600 && text.contains("= {") {
                sample = format!("{name}\n{text}");
            }
            // nothing may be dropped: every plain string constant of a fully decompiled file is in the text
            let mut missing = 0usize;
            let mut missing_first = String::new();
            fn strings(f: &rgrf_formats::luac::Function, out: &mut Vec<Vec<u8>>) {
                for c in &f.constants {
                    if let rgrf_formats::luac::Constant::Str(s) = c {
                        out.push(s.clone());
                    }
                }
                f.protos.iter().for_each(|p| strings(p, out));
            }
            let mut all = Vec::new();
            strings(&chunk.main, &mut all);
            for s in all {
                if s.iter().any(|&b| b < 0x20 || b == b'"' || b == b'\\') {
                    continue;
                }
                if !text.contains(&rgrf_formats::luadec::string_text(&s, &names)) {
                    missing += 1;
                    if missing_first.is_empty() {
                        missing_first = rgrf_formats::luadec::string_text(&s, &names);
                    }
                }
            }
            if missing > 0 {
                bad_syntax.push(format!(
                    "{name}: {missing} string constants are missing in the source, e.g. {missing_first:?}"
                ));
            }
            if let Err(e) = lua::check(text.as_bytes()) {
                bad_syntax.push(format!("{name}: {}", e.message));
            }
        } else {
            partial += 1;
            if partial_samples.len() < 3 {
                let at = text.find("-- not decompiled").unwrap_or(0);
                partial_samples.push(format!(
                    "{name}\n{}",
                    text[at..].chars().take(1800).collect::<String>()
                ));
            }
        }
    }
    eprintln!(
        "{compiled} compiled ({v50} of them Lua 5.0): {complete} fully decompiled, {partial} with control flow left as listing, {functions_total} functions written"
    );
    eprintln!("functions left as listing, by reason: {reasons:?}");
    for p in &partial_samples {
        eprintln!("PARTIAL {p}");
    }
    eprintln!("sample:\n{sample}");
    assert!(
        bad_parse.is_empty(),
        "{} unreadable, e.g. {:?}",
        bad_parse.len(),
        &bad_parse[..bad_parse.len().min(5)]
    );
    assert!(
        bad_syntax.is_empty(),
        "{} decompiled files are not valid Lua, e.g. {:?}",
        bad_syntax.len(),
        &bad_syntax[..bad_syntax.len().min(5)]
    );
}

/// Every compiled `.lub` of the kRO client, Lua 5.0 and 5.1, is read to its last byte and passes
/// the structural check (known opcodes, indices and jumps in range).
#[test]
fn every_compiled_lub_is_structurally_valid() {
    let Some(base) = base() else { return };
    let arc = Archive::open(base.join("0 OTHER CLIENTS/client-kro/data.grf")).unwrap();
    let idx = arc.select(Some("*.lub"), NameMode::Cp1252);
    let counts = Mutex::new(std::collections::BTreeMap::<&'static str, usize>::new());
    let bad: Vec<String> = idx
        .par_iter()
        .filter_map(|&i| {
            let data = arc.read(i).unwrap().data;
            if !data.starts_with(b"\x1bLua") {
                return None;
            }
            match rgrf_formats::luac::check_bytecode(&data) {
                Ok(info) => {
                    *counts.lock().unwrap().entry(info.version).or_default() += 1;
                    None
                }
                Err(e) => Some(format!(
                    "{}: {e}",
                    arc.entries()[i].name.display(NameMode::Cp949)
                )),
            }
        })
        .collect();
    eprintln!("valid compiled files: {:?}", counts.lock().unwrap());
    assert!(
        bad.is_empty(),
        "{} invalid, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
    let c = counts.lock().unwrap();
    assert_eq!(
        (c.get("5.0").copied(), c.get("5.1").copied()),
        (Some(62), Some(442))
    );
}

/// Damaged compiled files must never make the readers, the checks, the listing or the decompiler
/// panic: random byte changes and cut-off copies of real files (5.0 and 5.1).
#[test]
fn damaged_bytecode_never_panics() {
    use rgrf_formats::{luac, luac50, luadec};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let Some(base) = base() else { return };
    let arc = Archive::open(base.join("0 OTHER CLIENTS/client-kro/data.grf")).unwrap();
    let names = |b: &[u8]| String::from_utf8_lossy(b).into_owned();
    let mut seed = 0x9e37_79b9_7f4a_7c15_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let (mut runs, mut panics) = (0usize, Vec::new());
    for (n, &i) in arc
        .select(Some("*.lub"), NameMode::Cp1252)
        .iter()
        .enumerate()
    {
        let data = arc.read(i).unwrap().data;
        if !data.starts_with(b"\x1bLua") || data.len() > 300_000 {
            continue;
        }
        for round in 0..12 {
            let mut d = data.clone();
            if round % 4 == 3 {
                d.truncate((next() as usize) % d.len());
            } else {
                for _ in 0..=(round % 3) {
                    let at = 5 + (next() as usize) % (d.len() - 5);
                    d[at] = (next() & 0xff) as u8;
                }
            }
            runs += 1;
            let r = catch_unwind(AssertUnwindSafe(|| {
                let _ = luac::check_bytecode(&d);
                if let Ok(c) = luac::parse(&d) {
                    let _ = luadec::decompile(&c, &names);
                    let _ = luac::disassemble(&c, &names);
                }
                if let Ok(c) = luac50::parse(&d) {
                    let _ = luac50::disassemble(&c, &names);
                }
            }));
            if r.is_err() {
                panics.push(format!("file #{n} round {round}"));
            }
        }
    }
    eprintln!("{runs} damaged copies, {} panics", panics.len());
    assert!(runs > 3000);
    assert!(
        panics.is_empty(),
        "panics: {:?}",
        &panics[..panics.len().min(5)]
    );
}

/// The syntax check must not panic or overflow the stack on damaged text or on absurd nesting.
#[test]
fn damaged_text_never_panics() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    let deep = format!("x = {}1{}", "(".repeat(100_000), ")".repeat(100_000));
    assert!(
        lua::check(deep.as_bytes()).is_err(),
        "deep nesting is an error, not a crash"
    );
    let deep_tables = format!("x = {}{}", "{".repeat(100_000), "}".repeat(100_000));
    assert!(lua::check(deep_tables.as_bytes()).is_err());
    let deep_fn = "function f() ".repeat(50_000);
    assert!(lua::check(deep_fn.as_bytes()).is_err());

    let Some(base) = base() else { return };
    let arc = Archive::open(base.join("client-dev/full/core.grf")).unwrap();
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let (mut runs, mut panics) = (0usize, 0usize);
    for &i in &arc.select(Some("*.lub"), NameMode::Cp1252) {
        let data = arc.read(i).unwrap().data;
        if data.is_empty() {
            continue;
        }
        for round in 0..10 {
            let mut d = data.clone();
            if round % 5 == 4 {
                d.truncate((next() as usize) % d.len());
            } else {
                for _ in 0..=(round % 3) {
                    let at = (next() as usize) % d.len();
                    d[at] = (next() & 0xff) as u8;
                }
            }
            runs += 1;
            panics += usize::from(
                catch_unwind(AssertUnwindSafe(|| {
                    let _ = lua::check(&d);
                }))
                .is_err(),
            );
        }
    }
    eprintln!("{runs} damaged text copies, {panics} panics");
    assert_eq!(panics, 0);
}
