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

    let i = find(&mut s, r"data\prontera.rsw");
    match s.preview(i, None).unwrap() {
        Preview::Info { lines, .. } => {
            assert!(lines[0].contains("RSW 2.1"), "{lines:?}");
            assert!(lines[1].contains("1304 models"), "{lines:?}");
        }
        other => panic!("{other:?}"),
    }

    let i = find(&mut s, r"data\model\ad\ad01.rsm");
    match s.preview(i, None).unwrap() {
        Preview::Info { lines, .. } => {
            assert!(lines.iter().any(|l| l.contains("ad_j_01.bmp")), "{lines:?}")
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
