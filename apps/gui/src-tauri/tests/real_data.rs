//! The backend on the real kRO `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's golden tests).

use std::path::PathBuf;
use std::time::Instant;

use rustygrf_gui_lib::backend::{Preview, Session};

fn open() -> Option<Session> {
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
    let t = Instant::now();
    let s = Session::open(&p).unwrap();
    eprintln!("opened {} entries in {:?}", s.info().entries, t.elapsed());
    Some(s)
}

/// Index of the entry whose path ends with `suffix` (case-insensitive) among the search hits.
fn find(s: &mut Session, suffix: &str) -> usize {
    assert!(s.search(suffix) >= 1, "{suffix} not found");
    s.page(0, 500)
        .into_iter()
        .find(|r| {
            r.name
                .to_ascii_lowercase()
                .ends_with(&suffix.to_ascii_lowercase())
        })
        .unwrap_or_else(|| panic!("{suffix} not among the matches"))
        .index
}

#[test]
fn search_and_paging_on_200k_entries() {
    let Some(mut s) = open() else { return };
    let total = s.info().entries;
    assert!(total > 200_000);
    let t = Instant::now();
    let n = s.search("*.spr");
    let search = t.elapsed();
    let t = Instant::now();
    let rows = s.page(n - 20, 20);
    eprintln!(
        "search *.spr: {n} hits in {search:?}, page in {:?}",
        t.elapsed()
    );
    assert!(n > 40_000 && rows.len() == 20);
    assert!(rows.iter().all(|r| r.kind == "sprite"));
    assert!(search.as_millis() < 2000, "search took {search:?}");
    assert_eq!(s.search(""), total);
}

#[test]
fn previews_of_real_files() {
    let Some(mut s) = open() else { return };

    let i = find(&mut s, r"data\prontera.gat");
    match s.preview(i, None).unwrap() {
        Preview::Map {
            width,
            height,
            info,
            ..
        } => {
            assert_eq!(
                (width, height),
                (624, 784),
                "312 x 392 cells, enlarged twice"
            );
            assert!(info[0].starts_with("GAT 1.2"));
        }
        other => panic!("{other:?}"),
    }

    // a world: ground, 1304 model instances of 63 models, water and light
    let i = find(&mut s, r"data\prontera.rsw");
    let t = Instant::now();
    match s.preview(i, None).unwrap() {
        Preview::Scene {
            ground,
            models,
            instances,
            textures,
            water,
            info,
            ..
        } => {
            eprintln!("prontera scene in {:?}", t.elapsed());
            let g = ground.unwrap();
            assert_eq!((g.width, g.height), (156, 196));
            assert_eq!((models.len(), instances.len()), (63, 1304));
            assert!(textures.iter().filter(|t| t.png.is_some()).count() > 150);
            assert!(water.is_some());
            assert!(info[1].contains("RSW 2.1"), "{info:?}");
            assert!(instances.iter().all(|i| i.matrix.len() == 16 && i.matrix.iter().all(|v| v.is_finite())));
        }
        other => panic!("{other:?}"),
    }

    let i = find(&mut s, r"data\model\ad\ad01.rsm");
    match s.preview(i, None).unwrap() {
        Preview::Model {
            parts,
            textures,
            triangles,
            ..
        } => {
            assert!(triangles > 0 && !parts.is_empty());
            assert!(
                textures
                    .iter()
                    .any(|t| t.name.contains("ad_j_01.bmp") && t.png.is_some()),
                "the texture is found and decoded"
            );
        }
        other => panic!("{other:?}"),
    }

    // an animation with its sprite in the same archive: a GIF of one action with several frames
    let act = find(&mut s, "poporing.act");
    let t = Instant::now();
    match s.preview(act, Some(8)).unwrap() {
        Preview::Animation {
            frames,
            actions,
            action,
            gif,
            ..
        } => {
            eprintln!("poporing action 8 rendered in {:?}", t.elapsed());
            assert_eq!((actions, action, frames), (72, 8, 10));
            assert!(gif.len() > 1000);
        }
        other => panic!("{other:?}"),
    }

    // a TGA (the webview cannot show them itself) becomes a PNG
    assert!(s.search("*.tga") > 100);
    let tga = s.page(0, 1)[0].index;
    match s.preview(tga, None).unwrap() {
        Preview::Image { mime, data } => {
            assert_eq!((mime.as_str(), data.is_empty()), ("image/png", false))
        }
        other => panic!("{other:?}"),
    }

    // an ADPCM sound is converted to PCM (44-byte header + 16-bit samples)
    let wav = find(&mut s, "se_frog02.wav");
    match s.preview(wav, None).unwrap() {
        Preview::Audio { data, .. } => {
            use base64::Engine;
            let pcm = base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap();
            assert_eq!(&pcm[20..22], &[1, 0], "PCM format tag");
            assert!(pcm.len() > 80_000);
        }
        other => panic!("{other:?}"),
    }

    // a sprite shows its images
    let spr = find(&mut s, "poporing.spr");
    match s.preview(spr, None).unwrap() {
        Preview::Frames { frames, info } => {
            assert!(frames.len() > 10, "{} frames", frames.len());
            assert!(info[0].starts_with("SPR 2.1"));
        }
        other => panic!("{other:?}"),
    }

    // a Lua file: text or, when compiled, a message
    let lub = find(&mut s, r"datainfo\npcidentity.lub");
    assert!(matches!(
        s.preview(lub, None).unwrap(),
        Preview::Text { .. } | Preview::Message { .. }
    ));
}

/// Editing a real 80 MB archive: one file replaced, one added, one removed; the rest is copied.
#[test]
fn edits_a_real_archive() {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return;
    };
    let src = PathBuf::from(base).join("0 GRFs/reference/core-before-translation-2026-11-17.grf");
    assert!(src.exists(), "missing test archive {}", src.display());
    let dir = std::env::temp_dir().join(format!("rgrf-gui-edit-real-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("new.txt");
    std::fs::write(&file, "edited by RustyGRF").unwrap();

    let mut s = Session::open(&src).unwrap();
    let total = s.info().entries;
    let first = s.page(0, 2);
    s.stage_replace(first[0].index, &file).unwrap();
    s.stage_remove(&[first[1].index]).unwrap();
    s.stage_add("data\\rustygrf_test\\new.txt", &file).unwrap();
    let t = Instant::now();
    let out = dir.join("edited.grf");
    let r = rustygrf_gui_lib::backend::save_edited(
        &s.edit_plan().unwrap(),
        &out,
        false,
        &rgrf_core::progress::Control::default(),
    )
    .unwrap();
    eprintln!("edit of {total} entries took {:?}: {r:?}", t.elapsed());
    assert_eq!((r.added, r.replaced, r.removed), (1, 1, 1));
    assert_eq!(r.files, total, "one added, one removed");
    assert!(r.verified);
    let before = std::fs::metadata(&src).unwrap().len();
    assert!(
        r.file_bytes < before + 100_000,
        "unchanged entries keep their streams: {before} -> {}",
        r.file_bytes
    );
    let _ = std::fs::remove_dir_all(dir);
}
