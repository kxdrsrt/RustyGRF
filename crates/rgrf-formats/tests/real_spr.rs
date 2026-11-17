//! Reads every SPR of the real `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's golden tests).
//!
//! Checks: all files parse, writing them again keeps every pixel and the palette, and reports how
//! many files come back byte-identical (printed, not asserted: other tools compress differently).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::spr::Spr;

fn data_grf() -> Option<PathBuf> {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        assert!(
            std::env::var_os("RGRF_REQUIRE_TESTDATA").is_none(),
            "RGRF_TESTDATA is not set"
        );
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return None;
    };
    let p = PathBuf::from(base).join("0 OTHER CLIENTS/client-kro/data.grf");
    assert!(p.exists(), "missing test archive {}", p.display());
    Some(p)
}

#[derive(Default)]
struct Stats {
    total: usize,
    versions: BTreeMap<String, usize>,
    parse_errors: Vec<String>,
    warned: Vec<String>,
    pixel_mismatch: Vec<String>,
    byte_identical: usize,
    byte_different: Vec<String>,
    rgba_files: usize,
    indexed_frames: usize,
}

#[test]
fn every_spr_in_data_grf_round_trips() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let sprs = arc.select(Some("*.spr"), NameMode::Cp1252);
    assert!(sprs.len() > 1000, "only {} SPR files found", sprs.len());
    let stats = Mutex::new(Stats::default());

    sprs.par_iter().for_each(|&i| {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        let mut st = Stats {
            total: 1,
            ..Stats::default()
        };
        match Spr::parse(&data) {
            Err(e) => st.parse_errors.push(format!("{name}: {e}")),
            Ok(spr) => {
                *st.versions.entry(spr.version.to_string()).or_default() += 1;
                st.indexed_frames += spr.indexed.len();
                st.rgba_files += usize::from(!spr.rgba.is_empty());
                if !spr.warnings.is_empty() {
                    st.warned
                        .push(format!("{name}: {}", spr.warnings.join("; ")));
                }
                match spr.to_bytes(spr.version) {
                    Err(e) => st.pixel_mismatch.push(format!("{name}: write failed: {e}")),
                    Ok(out) => {
                        if out == data {
                            st.byte_identical += 1;
                        } else {
                            let at = out.iter().zip(&data).position(|(a, b)| a != b);
                            st.byte_different.push(format!(
                                "{name} (len {} -> {}, first difference at {:?})",
                                data.len(),
                                out.len(),
                                at
                            ));
                        }
                        match Spr::parse(&out) {
                            Ok(b)
                                if b.indexed == spr.indexed
                                    && b.rgba == spr.rgba
                                    && b.palette == spr.palette => {}
                            _ => st.pixel_mismatch.push(name),
                        }
                    }
                }
            }
        }
        let mut g = stats.lock().unwrap();
        g.total += st.total;
        for (k, v) in st.versions {
            *g.versions.entry(k).or_default() += v;
        }
        g.parse_errors.extend(st.parse_errors);
        g.warned.extend(st.warned);
        g.pixel_mismatch.extend(st.pixel_mismatch);
        g.byte_identical += st.byte_identical;
        g.byte_different.extend(st.byte_different);
        g.rgba_files += st.rgba_files;
        g.indexed_frames += st.indexed_frames;
    });

    let mut s = stats.into_inner().unwrap();
    s.byte_different.sort();
    s.warned.sort();
    eprintln!(
        "SPR files: {}, versions {:?}, indexed frames {}, files with RGBA images {}",
        s.total, s.versions, s.indexed_frames, s.rgba_files
    );
    eprintln!(
        "byte-identical after rewrite: {} of {} (different: {}, e.g. {:?})",
        s.byte_identical,
        s.total,
        s.byte_different.len(),
        &s.byte_different[..s.byte_different.len().min(6)]
    );
    eprintln!(
        "warnings in {} files, e.g. {:?}",
        s.warned.len(),
        &s.warned[..s.warned.len().min(3)]
    );
    assert!(
        s.parse_errors.is_empty(),
        "{:?}",
        &s.parse_errors[..s.parse_errors.len().min(5)]
    );
    assert!(
        s.pixel_mismatch.is_empty(),
        "{:?}",
        &s.pixel_mismatch[..s.pixel_mismatch.len().min(5)]
    );
}
