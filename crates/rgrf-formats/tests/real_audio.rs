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

/// The background music of the client is MP3 outside the archives: every file reads, a trimmed
/// copy is written as MP3 again with about the same bit rate and reads back at the new length.
#[cfg(feature = "lossy-audio")]
#[test]
fn the_background_music_can_be_trimmed_and_written_as_mp3_again() {
    use rgrf_formats::audio::Format;
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return;
    };
    let dir = PathBuf::from(base).join("client-dev/full/BGM");
    let Ok(rd) = std::fs::read_dir(&dir) else {
        eprintln!("SKIPPED: no {}", dir.display());
        return;
    };
    let mut files: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mp3")))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let results: Vec<String> = files
        .par_iter()
        .enumerate()
        .filter_map(|(n, p)| {
            let bytes = std::fs::read(p).unwrap();
            let (mut a, src) = match Audio::decode(&bytes) {
                Ok(x) => x,
                Err(e) => return Some(format!("{}: {e}", p.display())),
            };
            assert_eq!(src.format, Format::Mp3);
            // every 12th file goes through the encoder too (it is slow)
            if n % 12 != 0 {
                return None;
            }
            let before = a.duration_ms();
            a.apply(&Op::parse("trim:1000-6000").unwrap()).unwrap();
            let out = a.encode(&src).unwrap();
            let (b, again) = Audio::decode(&out).unwrap();
            assert_eq!(again.format, Format::Mp3);
            let len = b.duration_ms();
            assert!(
                (4900..=5200).contains(&len),
                "{}: {len} ms (was {before})",
                p.display()
            );
            let (k0, k1) = (src.kbps.unwrap_or(0) as f64, again.kbps.unwrap_or(0) as f64);
            assert!(k1 > k0 * 0.5 && k1 < k0 * 1.6 + 20.0, "{k0} -> {k1} kbit/s");
            None
        })
        .collect();
    assert!(results.is_empty(), "{results:#?}");
    eprintln!("{} MP3 files read", files.len());
}
