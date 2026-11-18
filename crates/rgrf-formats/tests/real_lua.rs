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
