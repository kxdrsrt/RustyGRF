//! Downgrades every map of the real `data.grf` that has RSM2 models and checks the result with
//! the readers of the crate (needs `RGRF_TESTDATA`, see rgrf-core's golden tests).

use std::collections::HashSet;
use std::path::PathBuf;

use rgrf_core::{Archive, GrfPath};
use rgrf_formats::downgrade::apply_map;
use rgrf_formats::{gnd_map, rsm, rsw::Rsw};

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
fn maps_with_rsm2_models_downgrade_to_readable_files() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let keys = arc.index_by_key();
    let read = |p: &[u8]| -> Result<Option<Vec<u8>>, rgrf_core::Error> {
        match keys.get(&GrfPath::new(p.to_vec()).key()) {
            Some(i) => Ok(Some(arc.read(*i.last().unwrap())?.data)),
            None => Ok(None),
        }
    };
    let mut maps = 0;
    let (mut models, mut instances) = (0, 0);
    let mut seen = HashSet::new();
    for e in arc.entries() {
        let n = e.name.as_bytes();
        if !n.to_ascii_lowercase().ends_with(b".rsw") {
            continue;
        }
        let rsw = Rsw::parse(&arc.read(arc.find(&e.name).unwrap()).unwrap().data).unwrap();
        if !rsw
            .model_files()
            .iter()
            .any(|m| m.to_ascii_lowercase().ends_with(".rsm2"))
        {
            continue;
        }
        let base = &n[..n.len() - 4];
        let (files, d) = apply_map(base, read).unwrap().unwrap();
        maps += 1;
        assert!(d.failed.is_empty(), "{:?}", d.failed);
        // the world file is 2.1 and names no RSM2 model that was converted
        let w = Rsw::parse(&d.rsw).unwrap();
        assert_eq!((w.version.major, w.version.minor), (2, 1));
        assert!(w.water.is_some());
        let left: Vec<String> = w
            .model_files()
            .into_iter()
            .filter(|m| m.to_ascii_lowercase().ends_with(".rsm2"))
            .collect();
        assert_eq!(left.len(), d.missing.len(), "{left:?} vs {:?}", d.missing);
        let g = gnd_map::parse(&d.gnd).unwrap();
        assert_eq!((g.version.major, g.version.minor), (1, 7));
        assert!(g.tail.is_empty());
        instances += d.instances;
        for (name, bytes) in &d.models {
            if !seen.insert(name.clone()) {
                continue;
            }
            models += 1;
            let r = rsm::Rsm::parse(bytes).unwrap();
            assert_eq!((r.version.major, r.version.minor), (1, 4));
            assert_eq!(r.trailing, 0, "{}", String::from_utf8_lossy(name));
        }
        assert!(files.len() >= 2);
    }
    eprintln!("{maps} maps, {models} models, {instances} instances");
    assert!(maps > 0, "no map with RSM2 models");
}
