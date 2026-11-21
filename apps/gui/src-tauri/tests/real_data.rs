//! The backend on the real kRO `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's golden tests).

use std::path::PathBuf;
use std::time::Instant;

use rustygrf_gui_lib::backend::{Note, Preview, Session};

/// Whether a preview line is the catalogue line `key` for this format version.
fn is_note(n: &Note, key: &str, version: &str) -> bool {
    matches!(n, Note::Key { key: k, args } if *k == key && args["version"] == version)
}

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

/// The gallery: 96 thumbnails of real pictures and sprites (measured: time, size, failures).
#[test]
fn thumbnails_of_real_pictures() {
    let Some(mut s) = open() else { return };
    let mut indices = Vec::new();
    for pattern in ["*.bmp", "*.tga", "*.spr", "*.png", "*.jpg"] {
        let n = s.search(pattern);
        indices.extend(s.page(n / 2, 24).into_iter().map(|r| r.index));
    }
    let t = Instant::now();
    let thumbs = s.thumbnails(&indices, 160);
    let took = t.elapsed();
    let bytes: usize = thumbs
        .iter()
        .filter_map(|t| t.data.as_ref())
        .map(String::len)
        .sum();
    let failed: Vec<_> = thumbs.iter().filter(|t| t.data.is_none()).collect();
    eprintln!(
        "{} thumbnails in {took:?}, {} KB as base64, {} failed {:?}",
        thumbs.len(),
        bytes / 1024,
        failed.len(),
        failed.iter().map(|t| t.error.clone()).collect::<Vec<_>>()
    );
    assert_eq!(thumbs.len(), indices.len());
    assert!(failed.len() * 10 < thumbs.len(), "too many failed");
    assert!(took.as_millis() < 5000, "took {took:?}");
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
            assert!(is_note(&info[0], "pvw-n-gat", "1.2"));
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
            assert!(textures.iter().filter(|t| t.entry.is_some()).count() > 150);
            // the textures come as pictures through the scheme
            let tex = textures.iter().find_map(|t| t.entry).unwrap();
            let (data, name) = s.asset_source(tex).unwrap();
            let (png, mime) =
                rustygrf_gui_lib::backend::asset_response("tex", data, &name).unwrap();
            assert_eq!((&png[1..4], mime), (b"PNG".as_slice(), "image/png"));
            let water = water.unwrap();
            assert_eq!(water.frames.len(), 32, "the whole water cycle is found");
            assert!(water.frame_ms > 0.0);
            assert!(is_note(&info[1], "pvw-n-rsw", "2.1"), "{info:?}");
            assert!(
                instances
                    .iter()
                    .all(|i| i.matrix.len() == 16 && i.matrix.iter().all(|v| v.is_finite()))
            );
        }
        other => panic!("{other:?}"),
    }

    // compiled Lua: 5.1 is decompiled, 5.0 is shown as a bytecode listing
    for (path, expect) in [
        (r"data\lua files\emotionlist.lub", "Lua 5.0"),
        (r"data\luafiles514\lua files\deletefiles.lub", "decompiled"),
    ] {
        let i = find(&mut s, path);
        match s.preview(i, None).unwrap() {
            Preview::Text { text, encoding, .. } => {
                assert!(encoding.contains(expect), "{path}: {encoding}");
                assert!(!text.is_empty());
            }
            other => panic!("{path}: {other:?}"),
        }
    }

    // a map with moving models: node matrices over the loop and a node number per vertex
    let i = find(&mut s, r"data\mjolnir_12.rsw");
    match s.preview(i, None).unwrap() {
        Preview::Scene {
            models, instances, ..
        } => {
            let floats = |b: &str| -> Vec<f32> {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b).unwrap();
                let (words, _) = bytes.as_chunks::<4>();
                words.iter().map(|c| f32::from_le_bytes(*c)).collect()
            };
            let moving: Vec<usize> = (0..models.len())
                .filter(|&m| models[m].anim.is_some())
                .collect();
            assert!(!moving.is_empty(), "mjolnir_12 has moving models");
            for &m in &moving {
                let a = models[m].anim.as_ref().unwrap();
                let mats = floats(&a.matrices);
                assert_eq!(mats.len(), a.node_count * a.samples * 16);
                assert!(mats.iter().all(|v| v.is_finite()));
                for p in &models[m].parts {
                    let nodes = floats(p.nodes.as_ref().unwrap());
                    assert_eq!(nodes.len() * 3, floats(&p.positions).len());
                    assert!(
                        nodes
                            .iter()
                            .all(|&n| n >= 0.0 && (n as usize) < a.node_count)
                    );
                }
            }
            let moving_instances = instances
                .iter()
                .filter(|i| moving.contains(&i.model) && i.speed > 0.0)
                .count();
            eprintln!(
                "mjolnir_12: {} moving models, {moving_instances} moving instances",
                moving.len()
            );
            assert!(moving_instances > 100);
        }
        other => panic!("{other:?}"),
    }

    let i = find(&mut s, r"data\model\ad\ad01.rsm");
    match s.preview(i, None).unwrap() {
        Preview::Model {
            nodes,
            textures,
            triangles,
            ..
        } => {
            assert!(triangles > 0 && !nodes.is_empty());
            assert!(
                textures
                    .iter()
                    .any(|t| t.name.contains("ad_j_01.bmp") && t.entry.is_some()),
                "the texture is found and decoded"
            );
        }
        other => panic!("{other:?}"),
    }

    // the clock tower's hands turn: some node matrix differs between two moments of the loop
    let i = find(&mut s, "data\\model\\프론테라\\시계탑.rsm");
    match s.preview(i, None).unwrap() {
        Preview::Model {
            nodes,
            duration_ms,
            samples,
            ..
        } => {
            assert!(duration_ms > 0.0 && samples > 1);
            let floats = |b: &str| -> Vec<f32> {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b).unwrap();
                let (words, _) = bytes.as_chunks::<4>();
                words.iter().map(|c| f32::from_le_bytes(*c)).collect()
            };
            let moved = nodes.iter().any(|n| {
                let m = floats(&n.matrices);
                assert_eq!(m.len(), samples * 16);
                assert!(m.iter().all(|v| v.is_finite()));
                m[..16] != m[(samples / 4) * 16..(samples / 4) * 16 + 16]
            });
            assert!(moved, "an animated node changes over the loop");
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
        Preview::Image { mime, data, .. } => {
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
            assert!(is_note(&info[0], "pvw-n-spr", "2.1"));
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

#[test]
fn the_folder_tree_holds_every_entry_once_and_opens_completely() {
    use rustygrf_gui_lib::view::ViewSpec;
    use std::collections::{HashMap, HashSet};
    let Some(mut s) = open() else { return };
    let total = s.info().entries;
    let tree = ViewSpec {
        layout: "tree".into(),
        ..ViewSpec::default()
    };
    // closed: only the top folder line(s)
    let closed = s.set_view(0, tree.clone()).unwrap();
    assert_eq!(closed.matches, total);
    let top = s.page_view(0, 0, 50);
    assert!(top.iter().all(|r| r.folder && !r.expanded) && closed.rows == top.len());
    assert_eq!(top.iter().map(|r| r.count).sum::<usize>(), total);

    // everything open: each entry once, under the folder its name says
    let rows = s.tree_set_all(0, true).unwrap();
    let mut lines = Vec::with_capacity(rows);
    let mut at = 0;
    while at < rows {
        let page = s.page_view(0, at, 500);
        assert!(!page.is_empty());
        at += page.len();
        lines.extend(page);
    }
    assert_eq!(lines.len(), rows);
    let files: Vec<_> = lines.iter().filter(|r| !r.folder).collect();
    assert_eq!(files.len(), total, "every entry is shown");
    let unique: HashSet<usize> = files.iter().map(|r| r.index).collect();
    assert_eq!(unique.len(), total, "no entry twice");

    // depth follows the path, and a file is listed after its own folder line
    let mut stack: Vec<String> = Vec::new(); // folder keys by depth
    let mut counted: HashMap<String, usize> = HashMap::new();
    for r in &lines {
        let key = r.name.to_lowercase();
        if r.folder {
            stack.truncate(usize::from(r.depth));
            let parent = stack.last().cloned().unwrap_or_default();
            assert!(
                parent.is_empty() || key.starts_with(&format!("{parent}\\")),
                "{} is not below {parent}",
                r.name
            );
            assert_eq!(
                key.split('\\').count(),
                usize::from(r.depth) + 1,
                "{}",
                r.name
            );
            stack.push(key);
        } else {
            let dir = key.rsplit_once('\\').map_or("", |(d, _)| d).to_string();
            let owner = if usize::from(r.depth) == 0 {
                String::new()
            } else {
                stack[usize::from(r.depth) - 1].clone()
            };
            assert_eq!(dir, owner, "{} is listed under the wrong folder", r.name);
            *counted.entry(owner).or_default() += 1;
        }
    }
    // a folder's count is the number of files in it and below
    for r in lines.iter().filter(|r| r.folder) {
        let key = r.name.to_lowercase();
        let below: usize = counted
            .iter()
            .filter(|(d, _)| **d == key || d.starts_with(&format!("{key}\\")))
            .map(|(_, n)| n)
            .sum();
        assert_eq!(r.count, below, "count of {}", r.name);
    }

    // close all again, then reveal one deep entry: only its way opens
    assert_eq!(s.tree_set_all(0, false).unwrap(), closed.rows);
    let target = find(&mut s, "firepit.spr");
    s.set_view(0, tree).unwrap();
    let at = s.locate(0, Some(target), None).unwrap();
    let line = at.line.expect("revealed");
    assert_eq!(s.page_view(0, line, 1)[0].index, target);
    // exactly the folders on the way are open (data, sprite and the monster folder)
    let mut open_folders = Vec::new();
    let mut from = 0;
    while from < at.rows {
        let page = s.page_view(0, from, 500);
        from += page.len();
        open_folders.extend(page.into_iter().filter(|r| r.folder && r.expanded));
    }
    assert_eq!(
        open_folders.len(),
        3,
        "{:?}",
        open_folders.iter().map(|r| &r.name).collect::<Vec<_>>()
    );
}

#[test]
fn water_waves_of_real_maps_have_plausible_values() {
    let Some(mut s) = open() else { return };
    let mut seen = 0;
    for map in ["pay_fild01", "comodo", "prontera", "alberta"] {
        let i = find(&mut s, &format!("data\\{map}.rsw"));
        let Preview::Scene { water: Some(w), .. } = s.preview(i, None).unwrap() else {
            continue;
        };
        eprintln!(
            "{map}: level {} waves {} / {} / {}",
            w.level, w.wave_height, w.wave_speed, w.wave_pitch
        );
        assert!((0.0..=5.0).contains(&w.wave_height), "{map}");
        assert!((0.0..=50.0).contains(&w.wave_speed), "{map}");
        assert!((0.0..=360.0).contains(&w.wave_pitch), "{map}");
        seen += 1;
    }
    assert!(seen >= 2, "maps with water were checked");
}

#[test]
fn a_map_with_moving_textures_sends_their_transforms() {
    let Some(mut s) = open() else { return };
    let i = find(&mut s, r"data\ba_maison.rsw");
    let Preview::Scene { models, .. } = s.preview(i, None).unwrap() else {
        panic!("not a scene");
    };
    let mut seen = 0;
    for m in &models {
        let Some(a) = &m.anim else { continue };
        for p in m.parts.iter().filter(|p| p.uv_anim.is_some()) {
            use base64::Engine;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(p.uv_anim.as_ref().unwrap())
                .unwrap();
            assert_eq!(bytes.len(), a.samples * 6 * 4, "six numbers per sample");
            seen += 1;
        }
    }
    eprintln!("ba_maison: {seen} parts with moving textures");
    assert!(seen > 0);
}

#[test]
fn a_map_is_extracted_with_everything_it_needs() {
    let Some(mut s) = open() else { return };
    let i = find(&mut s, r"data\prontera.rsw");
    let dest = std::env::temp_dir().join(format!("rgrf-gui-map-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dest);
    let r = s.extract_map(i, &dest, false, &[], None).unwrap();
    assert_eq!(
        (r.map.as_str(), r.files, r.damaged, r.skipped),
        ("prontera", 323, 0, 0)
    );
    assert!(r.missing.is_empty() && r.problems.is_empty(), "{r:?}");
    assert!(dest.join("data/prontera.gat").exists());
    // a second time the files exist: refused, or kept both
    assert!(s.extract_map(i, &dest, false, &[], None).is_err());
    assert_eq!(s.extract_map(i, &dest, true, &[], None).unwrap().files, 323);
    std::fs::remove_dir_all(&dest).unwrap();
    // without models and water: the ground, altitude and world stay, the rest is counted
    let parts = ["models".to_string(), "water".to_string()];
    let r = s.extract_map(i, &dest, false, &parts, None).unwrap();
    assert_eq!(r.files + r.skipped, 323);
    assert!(r.skipped > 100, "{r:?}");
    assert!(dest.join("data/prontera.rsw").exists() && dest.join("data/prontera.gnd").exists());
    assert!(!dest.join("data/model").exists());
    assert!(
        s.extract_map(i, &dest, true, &["lighting".to_string()], None)
            .is_err()
    );
    std::fs::remove_dir_all(&dest).unwrap();
    // without models, and taken out of the world file; ground without shadows and light
    let flat = rustygrf_gui_lib::backend::FlatReq {
        remove_models: true,
        remove_shadow: true,
        remove_light: true,
        ..Default::default()
    };
    let r = s
        .extract_map(i, &dest, false, &["models".to_string()], Some(flat))
        .unwrap();
    assert_eq!(r.changed, 2, "{r:?}");
    let world =
        rgrf_formats::rsw::Rsw::parse(&std::fs::read(dest.join("data/prontera.rsw")).unwrap())
            .unwrap();
    assert_eq!(world.object_counts().0, 0);
    let ground =
        rgrf_formats::gnd_map::parse(&std::fs::read(dest.join("data/prontera.gnd")).unwrap())
            .unwrap();
    assert_eq!(ground.lightmap_count, 1);
    // the changed files exist now: refused, or written next to them
    assert!(
        s.extract_map(i, &dest, false, &["models".to_string()], Some(flat))
            .is_err()
    );
    s.extract_map(i, &dest, true, &["models".to_string()], Some(flat))
        .unwrap();
    assert!(dest.join("data/prontera~1.gnd").exists());
    std::fs::remove_dir_all(dest).unwrap();
}

/// The sounds of a monster animation are found where the client looks (`data\wav\`), and the
/// client's "atk" marker is not mistaken for a file.
#[test]
fn the_sounds_of_an_animation_are_found_and_playable() {
    let Some(s) = open() else { return };
    let sound = s.act_sound("poring_move.wav").unwrap();
    assert_eq!(sound.path, "data\\wav\\poring_move.wav");
    assert!(
        sound.data.is_some_and(|d| d.len() > 100),
        "{:?}",
        sound.problem
    );
    let atk = s.act_sound("atk").unwrap();
    assert!(atk.data.is_none() && atk.problem.unwrap().contains("hits"));
    let missing = s.act_sound("no_such_sound.wav").unwrap();
    assert_eq!(missing.problem.as_deref(), Some("not in this archive"));
    assert!(s.act_sound(&"x".repeat(60)).is_err());
}

/// A headgear is edited with a head and a body behind it, attached by their anchors as in the
/// client; a missing reference or the edited file itself is named, not drawn.
#[test]
fn an_animation_is_shown_with_reference_animations() {
    use rustygrf_gui_lib::backend::ReferenceSpec;
    let Some(mut s) = open() else { return };
    s.set_names("cp949").unwrap();
    let hat = find(&mut s, "남_Crushed_Snow_Man.act");
    let alone = s.act_view(hat, 0, 0).unwrap();
    let spec = |path: &str, under| ReferenceSpec {
        path: path.into(),
        under,
    };
    let refs = [
        spec("data\\sprite\\인간족\\몸통\\남\\초보자_남.act", true),
        spec("data/sprite/인간족/머리통/남/1_남.act", true),
        spec("data\\sprite\\no_such.act", true),
    ];
    let v = s.act_view_with(hat, 0, 0, &refs).unwrap();
    let problems: Vec<_> = v.references.iter().map(|r| r.problem.clone()).collect();
    assert_eq!(
        problems,
        [None, None, Some("is not in this archive".to_string())]
    );
    assert!(v.png.is_some() && v.error.is_none());
    // the body makes the picture taller than the hat alone
    assert!(v.height > alone.height, "{} vs {}", v.height, alone.height);
    let me = s
        .act_view_with(
            hat,
            0,
            0,
            &[spec(
                "data\\sprite\\악세사리\\남\\남_Crushed_Snow_Man.act",
                false,
            )],
        )
        .unwrap();
    assert_eq!(
        me.references[0].problem.as_deref(),
        Some("is the animation being edited")
    );
}

#[test]
fn a_real_map_is_downgraded_in_one_undoable_step() {
    let Some(mut s) = open() else { return };
    let n = s.search("*.rsw");
    let rows = s.page(0, n.min(1100));
    let mut done = None;
    for r in rows {
        match s.stage_map_downgrade(r.index) {
            Ok(d) => {
                done = Some((r.name, d));
                break;
            }
            Err(_) => assert!(s.pending().is_empty(), "a failed downgrade stages nothing"),
        }
    }
    let (map, d) = done.expect("no map with RSM2 models");
    eprintln!("{map}: {} models, {} instances", d.models, d.instances);
    assert!(d.models > 0 && d.instances >= d.models);
    let names: Vec<String> = s.pending().into_iter().map(|p| p.name).collect();
    assert!(
        names
            .iter()
            .any(|n| n.to_ascii_lowercase().ends_with(".rsw"))
    );
    assert!(
        names
            .iter()
            .any(|n| n.to_ascii_lowercase().ends_with(".gnd"))
    );
    assert!(
        names
            .iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".rsm"))
            .count()
            >= d.models
    );
    assert!(s.undo() && s.pending().is_empty());
}

#[test]
fn single_rsm2_models_become_rsm_in_one_undoable_step() {
    let Some(mut s) = open() else { return };
    let n = s.search("*.rsm2");
    let rows = s.page(0, n.min(5));
    assert!(!rows.is_empty(), "no RSM2 model");
    let indices: Vec<usize> = rows.iter().map(|r| r.index).collect();
    let d = s.stage_model_downgrade(&indices).unwrap();
    assert_eq!((d.models, d.failed.len()), (indices.len(), 0));
    let names: Vec<String> = s.pending().into_iter().map(|p| p.name).collect();
    assert_eq!(
        names
            .iter()
            .filter(|n| n.to_ascii_lowercase().ends_with(".rsm"))
            .count(),
        indices.len()
    );
    assert!(s.undo() && s.pending().is_empty());
    // something that is no model stages nothing
    let t = s.search("*.rsw");
    let r = s.page(0, t.min(1));
    assert!(s.stage_model_downgrade(&[r[0].index]).is_err() && s.pending().is_empty());
}
