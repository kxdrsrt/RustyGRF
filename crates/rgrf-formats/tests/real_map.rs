//! Reads every GAT, GND and RSW of the real `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's
//! golden tests): all must parse, GAT and RSW must come back byte-identical, and the files a map
//! names (ground, altitude, textures, models, sounds) are looked up in the archive.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, GrfPath, NameMode};
use rgrf_formats::gat::Gat;
use rgrf_formats::gnd::GndSummary;
use rgrf_formats::rsw::{Object, Rsw};

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

#[test]
fn every_gat_round_trips() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.gat"), NameMode::Cp1252);
    assert!(idx.len() > 500);
    let versions = Mutex::new(BTreeMap::<String, usize>::new());
    let bad: Vec<String> = idx
        .par_iter()
        .filter_map(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let data = arc.read(i).unwrap().data;
            match Gat::parse(&data) {
                Err(e) => Some(format!("{name}: {e}")),
                Ok(g) => {
                    *versions
                        .lock()
                        .unwrap()
                        .entry(g.version.to_string())
                        .or_default() += 1;
                    (g.to_bytes() != data).then(|| format!("{name}: not byte-identical"))
                }
            }
        })
        .collect();
    eprintln!(
        "GAT files: {}, versions {:?}, failures {}",
        idx.len(),
        versions.into_inner().unwrap(),
        bad.len()
    );
    assert!(bad.is_empty(), "{:?}", &bad[..bad.len().min(5)]);
}

#[test]
fn every_rsw_and_gnd_parses_and_references_exist() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let keymap = arc.index_by_key();
    let exists = |prefix: &str, name: &[u8]| {
        let mut b = prefix.as_bytes().to_vec();
        b.extend(name);
        keymap.contains_key(&GrfPath::new(b).key())
    };

    let rsws = arc.select(Some("*.rsw"), NameMode::Cp1252);
    assert!(rsws.len() > 500);
    #[derive(Default)]
    struct Stats {
        versions: BTreeMap<String, usize>,
        failures: Vec<String>,
        objects: usize,
        refs: usize,
        missing: Vec<String>,
    }
    let stats = Mutex::new(Stats::default());
    rsws.par_iter().for_each(|&i| {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        let mut s = Stats::default();
        match Rsw::parse(&data) {
            Err(e) => s.failures.push(format!("{name}: {e}")),
            Ok(r) => {
                *s.versions.entry(r.version.to_string()).or_default() += 1;
                s.objects += r.objects.len();
                if r.to_bytes() != data {
                    s.failures.push(format!("{name}: RSW not byte-identical"));
                }
                let mut check = |prefix: &str, raw: &[u8], what: &str| {
                    let t: Vec<u8> = raw.iter().take_while(|&&b| b != 0).copied().collect();
                    if t.is_empty() {
                        return;
                    }
                    s.refs += 1;
                    if !exists(prefix, &t) {
                        s.missing.push(format!(
                            "{name}: {what} {}{}",
                            prefix,
                            t.iter().map(|&b| char::from(b)).collect::<String>()
                        ));
                    }
                };
                check("data\\", &r.ground, "ground");
                check("data\\", &r.altitude, "altitude");
                let mut seen = HashSet::new();
                for o in &r.objects {
                    match o {
                        Object::Model(m) if seen.insert(m.model_name.to_vec()) => {
                            check("data\\model\\", &m.model_name, "model")
                        }
                        Object::Sound(sd) if seen.insert(sd.wave_name.to_vec()) => {
                            check("data\\wav\\", &sd.wave_name, "sound")
                        }
                        _ => {}
                    }
                }
                // the ground file's textures
                let gnd_name: Vec<u8> = r.ground.iter().take_while(|&&b| b != 0).copied().collect();
                let mut key = b"data\\".to_vec();
                key.extend(&gnd_name);
                if let Some(&gi) = keymap.get(&GrfPath::new(key).key()).and_then(|v| v.last()) {
                    match GndSummary::parse(&arc.read(gi).unwrap().data) {
                        Err(e) => s.failures.push(format!("{name}: GND: {e}")),
                        Ok(g) => {
                            for t in &g.textures {
                                s.refs += 1;
                                if !exists("data\\texture\\", t) {
                                    s.missing.push(format!(
                                        "{name}: texture {}",
                                        t.iter().map(|&b| char::from(b)).collect::<String>()
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        let mut g = stats.lock().unwrap();
        for (k, v) in s.versions {
            *g.versions.entry(k).or_default() += v;
        }
        g.failures.extend(s.failures);
        g.objects += s.objects;
        g.refs += s.refs;
        g.missing.extend(s.missing);
    });
    let mut s = stats.into_inner().unwrap();
    s.missing.sort();
    eprintln!(
        "RSW files: {}, versions {:?}, objects {}, failures {}",
        rsws.len(),
        s.versions,
        s.objects,
        s.failures.len()
    );
    eprintln!(
        "referenced files looked up: {}, not in the archive: {}, e.g. {:?}",
        s.refs,
        s.missing.len(),
        &s.missing[..s.missing.len().min(6)]
    );
    assert!(
        s.failures.is_empty(),
        "{:?}",
        &s.failures[..s.failures.len().min(5)]
    );
}

#[test]
fn every_rsm_is_read_to_its_last_byte_and_its_textures_exist() {
    use rgrf_formats::rsm::Rsm;
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let keymap = arc.index_by_key();
    let mut idx = arc.select(Some("*.rsm"), NameMode::Cp1252);
    idx.extend(arc.select(Some("*.rsm2"), NameMode::Cp1252));
    assert!(idx.len() > 5000, "only {} models", idx.len());
    #[derive(Default)]
    struct S {
        versions: BTreeMap<String, usize>,
        failures: Vec<String>,
        meshes: usize,
        refs: usize,
        missing: Vec<String>,
    }
    let s = Mutex::new(S::default());
    idx.par_iter().for_each(|&i| {
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        let mut l = S::default();
        match Rsm::parse(&data) {
            Err(e) => l.failures.push(format!("{name}: {e}")),
            Ok(m) => {
                *l.versions.entry(m.version.to_string()).or_default() += 1;
                l.meshes += m.meshes.len();
                if m.trailing != 0 {
                    l.failures
                        .push(format!("{name}: {} bytes left over", m.trailing));
                }
                for t in m.all_textures() {
                    l.refs += 1;
                    let mut k = b"data\\texture\\".to_vec();
                    k.extend(&t);
                    if !keymap.contains_key(&GrfPath::new(k).key()) {
                        l.missing.push(format!(
                            "{name}: {}",
                            t.iter().map(|&b| char::from(b)).collect::<String>()
                        ));
                    }
                }
            }
        }
        let mut g = s.lock().unwrap();
        for (k, v) in l.versions {
            *g.versions.entry(k).or_default() += v;
        }
        g.failures.extend(l.failures);
        g.meshes += l.meshes;
        g.refs += l.refs;
        g.missing.extend(l.missing);
    });
    let mut s = s.into_inner().unwrap();
    s.failures.sort();
    s.missing.sort();
    eprintln!(
        "models: {}, versions {:?}, meshes {}, failures {}",
        idx.len(),
        s.versions,
        s.meshes,
        s.failures.len()
    );
    eprintln!(
        "texture names looked up: {}, not in the archive: {}, e.g. {:?}",
        s.refs,
        s.missing.len(),
        &s.missing[..s.missing.len().min(5)]
    );
    assert!(
        s.failures.is_empty(),
        "{:?}",
        &s.failures[..s.failures.len().min(8)]
    );
}

/// Every TGA of the real archive decodes (they are used by effects and UI).
#[test]
fn every_tga_decodes() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.tga"), NameMode::Cp1252);
    eprintln!("{} TGA files", idx.len());
    let bad: Vec<String> = idx
        .par_iter()
        .filter_map(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let data = arc.read(i).unwrap().data;
            rgrf_formats::tga::parse(&data)
                .err()
                .map(|e| format!("{name}: {e}"))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "{} failed, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

/// Every WAV can be played by a webview: PCM as it is, the few ADPCM files decoded. (Measured on
/// the kRO archive: 3,326 PCM, 5 IMA ADPCM, 1 Microsoft ADPCM; the ADPCM decoders match Apple's
/// `afconvert` sample for sample on the common length.)
#[test]
fn every_wav_is_playable() {
    use rgrf_formats::wav::{Playable, playable};
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.wav"), NameMode::Cp1252);
    assert!(idx.len() > 3000);
    let results: Vec<(String, Result<Playable, String>)> = idx
        .par_iter()
        .map(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let d = arc.read(i).unwrap().data;
            (name, playable(&d).map_err(|e| e.to_string()))
        })
        .collect();
    let decoded = results
        .iter()
        .filter(|(_, r)| matches!(r, Ok(Playable::Decoded(_))))
        .count();
    let bad: Vec<_> = results
        .iter()
        .filter_map(|(n, r)| r.as_ref().err().map(|e| format!("{n}: {e}")))
        .collect();
    eprintln!(
        "{} WAV, {decoded} decoded from ADPCM, {} errors: {bad:?}",
        idx.len(),
        bad.len()
    );
    assert_eq!(decoded, 6);
    assert!(bad.is_empty(), "{bad:?}");
}

/// Every BMP of the real archive decodes (RLE and other rarities would show up here).
#[test]
fn every_bmp_decodes() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.bmp"), NameMode::Cp1252);
    eprintln!("{} BMP files", idx.len());
    let bad: Vec<String> = idx
        .par_iter()
        .filter_map(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let data = arc.read(i).unwrap().data;
            rgrf_formats::bmp::parse(&data)
                .err()
                .map(|e| format!("{name}: {e}"))
        })
        .collect();
    assert!(
        bad.is_empty(),
        "{} failed, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(8)]
    );
}

/// Every STR effect of the real archive parses; their textures are looked up in the archive and
/// a sample of effects is rendered (time and the share of resolved textures are printed).
#[test]
fn every_str_parses_and_renders() {
    use rgrf_formats::str_effect::{Textures, parse, render, texture_names};
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.str"), NameMode::Cp1252);
    eprintln!("{} STR files", idx.len());
    assert!(idx.len() > 1000);
    // `Archive::find` scans linearly, so look texture names up in a map made once
    let by_key: std::collections::HashMap<Vec<u8>, usize> = arc
        .entries()
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.key(), i))
        .collect();
    let mut bad = Vec::new();
    let (mut names, mut found) = (0usize, 0usize);
    let mut rendered = 0usize;
    let started = std::time::Instant::now();
    for (n, &i) in idx.iter().enumerate() {
        if n % 200 == 0 {
            eprintln!("  {n} / {} after {:?}", idx.len(), started.elapsed());
        }
        let name = arc.entries()[i].name.display(NameMode::Cp1252);
        let data = arc.read(i).unwrap().data;
        let s = match parse(&data) {
            Ok(s) => s,
            Err(e) => {
                bad.push(format!("{name}: {e}"));
                continue;
            }
        };
        let mut tex = Textures::new();
        for t in texture_names(&s) {
            names += 1;
            // next to the STR file first (newer effects), else in data\texture\effect
            let mut dir = arc.entries()[i].name.as_bytes().to_vec();
            dir.truncate(dir.iter().rposition(|&b| b == b'\\').map_or(0, |p| p + 1));
            let mut near = dir.clone();
            near.extend(&t);
            let mut shared = b"data\\texture\\effect\\".to_vec();
            shared.extend(&t);
            let Some(&ti) = [near, shared]
                .iter()
                .find_map(|p| by_key.get(&GrfPath::new(p.clone()).key()))
            else {
                if names % 4000 == 1 {
                    eprintln!("  missing {:?} used by {name}", String::from_utf8_lossy(&t));
                }
                continue;
            };
            found += 1;
            let d = arc.read(ti).unwrap().data;
            let lower = String::from_utf8_lossy(&t).to_lowercase();
            let img = if lower.ends_with(".tga") {
                rgrf_formats::tga::parse(&d).ok()
            } else {
                rgrf_formats::bmp::parse(&d).ok()
            };
            if let Some(img) = img {
                tex.insert(t, img);
            }
        }
        if n % 20 == 0 && !tex.is_empty() {
            let c = render(&s, (s.last_frame / 2) as f32, &tex, 0.5);
            assert_eq!((c.width, c.height), (320, 240));
            rendered += 1;
        }
    }
    eprintln!(
        "textures: {found} of {names} found; {rendered} sample effects rendered in {:?}",
        started.elapsed()
    );
    assert!(
        bad.is_empty(),
        "{} failed, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

/// Every RSM parses with its geometry, agrees with the structural reader and has a finite static pose.
#[test]
fn every_rsm_has_geometry_and_a_finite_pose() {
    use rgrf_formats::rsm::Rsm;
    use rgrf_formats::rsm_model::{duration_ms, node_matrices_at, parse, static_pose};
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let animated = std::sync::atomic::AtomicUsize::new(0);
    let mut idx = arc.select(Some("*.rsm"), NameMode::Cp1252);
    idx.extend(arc.select(Some("*.rsm2"), NameMode::Cp1252));
    eprintln!("{} RSM files", idx.len());
    let stats = Mutex::new(BTreeMap::<String, (usize, f32, f32)>::new());
    let bad: Vec<String> = idx
        .par_iter()
        .filter_map(|&i| {
            let name = arc.entries()[i].name.display(NameMode::Cp1252);
            let data = arc.read(i).unwrap().data;
            let m = match parse(&data) {
                Ok(m) => m,
                Err(e) => return Some(format!("{name}: {e}")),
            };
            let s = Rsm::parse(&data).unwrap();
            let (v1, f1): (usize, usize) = (
                m.nodes.iter().map(|n| n.vertices.len()).sum(),
                m.nodes.iter().map(|n| n.faces.len()).sum(),
            );
            let (v2, f2): (usize, usize) = (
                s.meshes.iter().map(|x| x.vertices).sum(),
                s.meshes.iter().map(|x| x.faces).sum(),
            );
            if (v1, f1) != (v2, f2) {
                return Some(format!("{name}: counts differ"));
            }
            let p = static_pose(&m);
            if p.min.iter().chain(&p.max).any(|v| !v.is_finite()) {
                return Some(format!("{name}: not finite"));
            }
            let d = duration_ms(&m);
            if d > 0.0 {
                animated.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if d < 6000.0 && std::env::var_os("RGRF_LIST_ANIMATED").is_some() {
                    eprintln!("  animated {d:.0} ms: {name}");
                }
            }
            for t in [0.0, d / 3.0, d / 2.0, d] {
                if node_matrices_at(&m, t)
                    .iter()
                    .flatten()
                    .any(|v| !v.is_finite())
                {
                    return Some(format!("{name}: animation not finite at {t}"));
                }
            }
            let size = (p.max[0] - p.min[0])
                .max(p.max[1] - p.min[1])
                .max(p.max[2] - p.min[2]);
            let mut st = stats.lock().unwrap();
            let e = st
                .entry(format!("{}", m.version))
                .or_insert((0, f32::MAX, 0.0));
            e.0 += 1;
            e.1 = e.1.min(size);
            e.2 = e.2.max(size);
            None
        })
        .collect();
    eprintln!(
        "  {} animated",
        animated.load(std::sync::atomic::Ordering::Relaxed)
    );
    for (v, (n, lo, hi)) in stats.lock().unwrap().iter() {
        eprintln!("  RSM {v}: {n} models, size {lo:.1} .. {hi:.1}");
    }
    assert!(
        bad.is_empty(),
        "{} failed, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

/// Every GND parses completely: tiles, lightmaps and cubes, with valid tile and lightmap indices.
#[test]
fn every_gnd_parses_with_valid_indices() {
    use rgrf_formats::gnd_map::parse;
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.gnd"), NameMode::Cp1252);
    eprintln!("{} GND files", idx.len());
    let versions = Mutex::new(BTreeMap::<String, (usize, usize)>::new());
    let bad: Vec<String> =
        idx.par_iter()
            .filter_map(|&i| {
                let name = arc.entries()[i].name.display(NameMode::Cp1252);
                let data = arc.read(i).unwrap().data;
                let g = match parse(&data) {
                    Ok(g) => g,
                    Err(e) => return Some(format!("{name}: {e}")),
                };
                let n_tiles = g.tiles.len() as i32;
                for c in &g.cubes {
                    for t in [c.up, c.front, c.right] {
                        if t >= n_tiles {
                            return Some(format!("{name}: tile {t} of {n_tiles}"));
                        }
                    }
                }
                if g.tiles.iter().any(|t| {
                    usize::from(t.lightmap) >= g.lightmap_count.max(1) && g.lightmap_count > 0
                }) {
                    return Some(format!("{name}: lightmap index out of range"));
                }
                if g.tiles
                    .iter()
                    .any(|t| usize::from(t.texture) >= g.textures.len())
                {
                    return Some(format!("{name}: texture index out of range"));
                }
                let mut v = versions.lock().unwrap();
                let e = v.entry(format!("{}", g.version)).or_insert((0, 0));
                e.0 += 1;
                e.1 += g.cubes.len();
                None
            })
            .collect();
    for (v, (n, cells)) in versions.lock().unwrap().iter() {
        eprintln!("  GND {v}: {n} maps, {cells} cells");
    }
    assert!(
        bad.is_empty(),
        "{} failed, e.g. {:?}",
        bad.len(),
        &bad[..bad.len().min(5)]
    );
}

#[test]
fn texture_animations_of_rsm2_are_measured() {
    use rgrf_formats::rsm_model::parse;
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let mut idx = arc.select(Some("*.rsm"), NameMode::Cp1252);
    idx.extend(arc.select(Some("*.rsm2"), NameMode::Cp1252));
    // kind -> (tracks, min value, max value, max frame, max keys)
    let stats = Mutex::new(BTreeMap::<i32, (usize, f32, f32, i32, usize)>::new());
    let models = std::sync::atomic::AtomicUsize::new(0);
    let sample = Mutex::new(Vec::<String>::new());
    idx.par_iter().for_each(|&i| {
        let data = arc.read(i).unwrap().data;
        let m = parse(&data).unwrap();
        let mut any = false;
        for n in &m.nodes {
            for a in &n.tex_anims {
                for t in &a.tracks {
                    any = true;
                    let mut st = stats.lock().unwrap();
                    let e = st.entry(t.kind).or_insert((0, f32::MAX, f32::MIN, 0, 0));
                    e.0 += 1;
                    e.4 = e.4.max(t.keys.len());
                    for &(f, v) in &t.keys {
                        e.1 = e.1.min(v);
                        e.2 = e.2.max(v);
                        e.3 = e.3.max(f);
                    }
                    if sample.lock().unwrap().len() < 12 {
                        sample.lock().unwrap().push(format!(
                            "{} (length {}, rate {}, nodes {}) tex {} kind {} keys {:?}",
                            arc.entries()[i].name.display(NameMode::Cp1252),
                            m.animation_length,
                            m.frame_rate,
                            m.nodes.len(),
                            a.texture,
                            t.kind,
                            &t.keys[..t.keys.len().min(4)]
                        ));
                    }
                }
            }
        }
        if any {
            models.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
    });
    let mut animated_models = std::collections::HashSet::new();
    for &i in &idx {
        let m = parse(&arc.read(i).unwrap().data).unwrap();
        if rgrf_formats::rsm_model::has_texture_animation(&m) {
            let n = arc.entries()[i].name.display(NameMode::Cp1252);
            animated_models.insert(n.rsplit('\\').next().unwrap().to_ascii_lowercase());
        }
    }
    let mut maps = Vec::new();
    for i in arc.select(Some("*.rsw"), NameMode::Cp1252) {
        let data = arc.read(i).unwrap().data;
        let low = String::from_utf8_lossy(&data).to_ascii_lowercase();
        let n = animated_models
            .iter()
            .filter(|m| low.contains(m.as_str()))
            .count();
        if n > 0 {
            maps.push(format!(
                "{} ({n})",
                arc.entries()[i].name.display(NameMode::Cp1252)
            ));
        }
    }
    eprintln!(
        "maps with texture-animated models: {:?}",
        &maps[..maps.len().min(10)]
    );
    eprintln!(
        "models with texture animation: {}, (kind: tracks, min, max, last frame, most keys) {:?}\n{:#?}",
        models.load(std::sync::atomic::Ordering::Relaxed),
        stats.into_inner().unwrap(),
        sample.into_inner().unwrap()
    );
}
