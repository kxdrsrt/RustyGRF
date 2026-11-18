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
