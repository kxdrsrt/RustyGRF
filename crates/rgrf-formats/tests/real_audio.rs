//! Reads every WAV of the real `data.grf` as an editable sound and edits a few (needs
//! `RGRF_TESTDATA`, see rgrf-core's golden tests).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::audio::{Audio, Op};

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
fn every_wav_of_the_client_is_editable() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.wav"), NameMode::Cp1252);
    let outcome = Mutex::new(BTreeMap::<String, usize>::new());
    idx.par_iter().for_each(|&i| {
        let data = arc.read(i).unwrap().data;
        let key = match Audio::from_wav(&data) {
            Ok(mut a) => {
                let ops = [
                    "gain:-3",
                    "fadeout:50",
                    "resample:22050",
                    "echo:120,0.3,0.3",
                ]
                .map(|t| Op::parse(t).unwrap());
                match a.apply_all(&ops) {
                    Ok(()) => {
                        let back = Audio::from_wav(&a.to_wav()).unwrap();
                        assert_eq!(back.channels.len(), a.channels.len());
                        format!("ok {} ch", a.channels.len())
                    }
                    Err(e) => format!("edit failed: {e}"),
                }
            }
            Err(e) => format!("read failed: {e}"),
        };
        *outcome.lock().unwrap().entry(key).or_default() += 1;
    });
    let outcome = outcome.into_inner().unwrap();
    eprintln!("{} wav files: {outcome:?}", idx.len());
    assert!(
        outcome.keys().all(|k| k.starts_with("ok")),
        "not every sound could be edited: {outcome:?}"
    );
}
