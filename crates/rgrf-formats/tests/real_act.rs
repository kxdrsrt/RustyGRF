//! Reads every ACT and PAL of the real `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's golden
//! tests). Checks: all parse, rewriting keeps the content, report how many are byte-identical, and
//! every layer points to an image that exists in the SPR next to it.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::act::Act;
use rgrf_formats::pal::Pal;
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
    content_mismatch: Vec<String>,
    byte_identical: usize,
    byte_different: Vec<String>,
    no_spr: usize,
    bad_index: Vec<String>,
    size_differs: usize,
    layers: usize,
}

#[test]
fn every_act_in_data_grf_round_trips() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let acts = arc.select(Some("*.act"), NameMode::Cp1252);
    assert!(acts.len() > 1000, "only {} ACT files found", acts.len());
    let by_name: HashMap<String, usize> = arc
        .select(Some("*.spr"), NameMode::Cp1252)
        .into_iter()
        .map(|i| {
            (
                arc.entries()[i]
                    .name
                    .display(NameMode::Cp1252)
                    .to_ascii_lowercase(),
                i,
            )
        })
        .collect();
    let stats = Mutex::new(Stats::default());

    acts.par_iter().for_each(|&i| {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        let mut st = Stats {
            total: 1,
            ..Stats::default()
        };
        match Act::parse(&data) {
            Err(e) => st.parse_errors.push(format!("{name}: {e}")),
            Ok(act) => {
                *st.versions.entry(act.version.to_string()).or_default() += 1;
                if !act.warnings.is_empty() {
                    st.warned.push(format!("{name}: {}", act.warnings.join("; ")));
                }
                match act.to_bytes(act.version) {
                    Err(e) => st.content_mismatch.push(format!("{name}: write failed: {e}")),
                    Ok(out) => {
                        if out == data {
                            st.byte_identical += 1;
                        } else {
                            let at = out.iter().zip(&data).position(|(a, b)| a != b);
                            st.byte_different.push(format!(
                                "{name} (v{}, len {} -> {}, first difference at {at:?})",
                                act.version,
                                data.len(),
                                out.len()
                            ));
                        }
                        match Act::parse(&out) {
                            Ok(b) if b.actions == act.actions && b.sounds == act.sounds => {}
                            _ => st.content_mismatch.push(name.clone()),
                        }
                    }
                }
                let spr_key = format!("{}spr", &name[..name.len() - 3]).to_ascii_lowercase();
                match by_name.get(&spr_key) {
                    None => st.no_spr += 1,
                    Some(&si) => {
                        if let Ok(spr) = Spr::parse(&arc.read(si).unwrap().data) {
                            for (a, f, l, ly) in act.layers() {
                                st.layers += 1;
                                if ly.sprite_index < 0 {
                                    continue;
                                }
                                let (count, dims) = if ly.sprite_type == 0 {
                                    (
                                        spr.indexed.len(),
                                        spr.indexed
                                            .get(ly.sprite_index as usize)
                                            .map(|i| (i.width, i.height)),
                                    )
                                } else {
                                    (
                                        spr.rgba.len(),
                                        spr.rgba
                                            .get(ly.sprite_index as usize)
                                            .map(|i| (i.width, i.height)),
                                    )
                                };
                                match dims {
                                    None => st.bad_index.push(format!(
                                        "{name}: action {a} frame {f} layer {l}: index {} of {count} (type {})",
                                        ly.sprite_index, ly.sprite_type
                                    )),
                                    Some((w, h)) => {
                                        if act.version >= rgrf_formats::act::V2_5
                                            && (ly.width != i32::from(w) || ly.height != i32::from(h))
                                        {
                                            st.size_differs += 1;
                                        }
                                    }
                                }
                            }
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
        g.content_mismatch.extend(st.content_mismatch);
        g.byte_identical += st.byte_identical;
        g.byte_different.extend(st.byte_different);
        g.no_spr += st.no_spr;
        g.bad_index.extend(st.bad_index);
        g.size_differs += st.size_differs;
        g.layers += st.layers;
    });

    let mut s = stats.into_inner().unwrap();
    s.byte_different.sort();
    s.warned.sort();
    s.bad_index.sort();
    s.parse_errors.sort();
    let head = |v: &[String], n| v[..v.len().min(n)].to_vec();
    eprintln!(
        "ACT files: {}, versions {:?}, layers checked {}, without SPR {}",
        s.total, s.versions, s.layers, s.no_spr
    );
    eprintln!(
        "parse errors: {} e.g. {:?}",
        s.parse_errors.len(),
        head(&s.parse_errors, 3)
    );
    eprintln!(
        "byte-identical after rewrite: {} of {} (different: {}, e.g. {:?})",
        s.byte_identical,
        s.total,
        s.byte_different.len(),
        head(&s.byte_different, 4)
    );
    eprintln!(
        "warnings in {} files, e.g. {:?}",
        s.warned.len(),
        head(&s.warned, 3)
    );
    eprintln!(
        "layers pointing to a missing SPR image: {} e.g. {:?}",
        s.bad_index.len(),
        head(&s.bad_index, 3)
    );
    eprintln!(
        "layers whose stored size differs from the image: {}",
        s.size_differs
    );
    assert!(
        s.content_mismatch.is_empty(),
        "{:?}",
        head(&s.content_mismatch, 5)
    );
    assert!(s.parse_errors.is_empty(), "{:?}", head(&s.parse_errors, 5));
}

#[test]
fn every_pal_in_data_grf_round_trips() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let pals = arc.select(Some("*.pal"), NameMode::Cp1252);
    assert!(pals.len() > 100, "only {} PAL files found", pals.len());
    let bad: Vec<String> = pals
        .par_iter()
        .filter_map(|&i| {
            let data = arc.read(i).unwrap().data;
            match Pal::parse(&data) {
                Ok(p) if p.to_bytes() == data => None,
                _ => Some(arc.entries()[i].name.display(NameMode::Cp1252)),
            }
        })
        .collect();
    eprintln!(
        "PAL files: {}, not 1024 bytes or not identical: {}",
        pals.len(),
        bad.len()
    );
    assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(5)]);
}

/// Draws every 40th ACT (all actions and frames) to be sure the renderer survives real files.
#[test]
fn render_a_sample_of_real_acts() {
    use rgrf_formats::render::{RenderError, bounds_of, render_frame};
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let by_name: HashMap<String, usize> = arc
        .select(Some("*.spr"), NameMode::Cp1252)
        .into_iter()
        .map(|i| {
            (
                arc.entries()[i]
                    .name
                    .display(NameMode::Cp1252)
                    .to_ascii_lowercase(),
                i,
            )
        })
        .collect();
    let acts: Vec<usize> = arc
        .select(Some("*.act"), NameMode::Cp1252)
        .into_iter()
        .step_by(40)
        .collect();
    let counts = Mutex::new((0usize, 0usize, 0usize, 0usize)); // files, frames, empty actions, too large
    acts.par_iter().for_each(|&i| {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let key = format!("{}spr", &name[..name.len() - 3]).to_ascii_lowercase();
        let Some(&si) = by_name.get(&key) else { return };
        let (Ok(act), Ok(spr)) = (
            Act::parse(&arc.read(i).unwrap().data),
            Spr::parse(&arc.read(si).unwrap().data),
        ) else {
            return;
        };
        let (mut frames, mut empty, mut large) = (0, 0, 0);
        for (a, action) in act.actions.iter().enumerate() {
            match bounds_of(&act, &spr, a, 0..action.frames.len()).unwrap() {
                None => empty += 1,
                Some(b) => {
                    for f in 0..action.frames.len() {
                        match render_frame(&act, &spr, a, f, b) {
                            Ok(c) => {
                                assert_eq!(
                                    c.pixels.len(),
                                    c.width as usize * c.height as usize * 4
                                );
                                frames += 1;
                            }
                            Err(RenderError::TooLarge { .. }) => large += 1,
                            Err(e) => panic!("{name}: {e}"),
                        }
                    }
                }
            }
        }
        let mut g = counts.lock().unwrap();
        g.0 += 1;
        g.1 += frames;
        g.2 += empty;
        g.3 += large;
    });
    let (files, frames, empty, large) = counts.into_inner().unwrap();
    eprintln!(
        "rendered {files} ACT files: {frames} frames, {empty} actions without image, {large} frames too large"
    );
    assert!(files > 100 && frames > 1000);
}
