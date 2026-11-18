//! End-to-end tests of the `rgrf` binary. The ones using real archives need `RGRF_TESTDATA`
//! (the `RO/1 CLIENT-SIDE` folder, see crates/rgrf-core/tests/golden.rs) and skip without it.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn rgrf(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rgrf"))
        .args(args)
        .output()
        .expect("run rgrf")
}

fn testdata(rel: &str) -> Option<String> {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return None;
    };
    let p = PathBuf::from(base).join(rel);
    assert!(p.exists(), "missing {}", p.display());
    Some(p.to_string_lossy().into_owned())
}

fn json(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&o.stdout)))
}

fn tmpdir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("rgrf-cli-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn version_and_help() {
    let o = rgrf(&["--version"]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).starts_with("rgrf "));
    let o = rgrf(&["--help"]);
    let text = String::from_utf8_lossy(&o.stdout).into_owned();
    for cmd in ["info", "ls", "find", "cat", "extract", "verify", "hash"] {
        assert!(text.contains(cmd), "{cmd} missing in help");
    }
}

#[test]
fn missing_and_non_grf_files_fail_with_exit_code_1() {
    let o = rgrf(&["info", "/nonexistent/x.grf"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("cannot open"));
    let not_grf = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
    let o = rgrf(&["ls", not_grf]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("not a GRF"));
    assert!(o.stdout.is_empty());
}

#[test]
fn unknown_name_mode_is_rejected() {
    let o = rgrf(&["--names", "ebcdic", "info", "x"]);
    assert_eq!(o.status.code(), Some(2)); // clap usage error
    assert!(String::from_utf8_lossy(&o.stderr).contains("unknown name mode"));
}

#[test]
fn info_ls_find_cat_hash_on_cdata() {
    let Some(grf) = testdata("0 GRFs/reference/cdata-2024-original.grf") else {
        return;
    };

    let o = rgrf(&["info", &grf, "--codecs", "--json"]);
    assert!(o.status.success());
    let v = json(&o);
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["version"], "0x200");
    assert_eq!(
        (v["entries"].as_u64(), v["markers"].as_u64()),
        (Some(3973), Some(0))
    );
    assert_eq!(v["codecs"]["zlib"], 3788);
    assert_eq!(v["codecs"]["lzma"], 185);

    let o = rgrf(&["ls", &grf, "frostjoke", "--json"]);
    let v = json(&o);
    assert_eq!(v["count"], 1);
    assert_eq!(v["entries"][0]["name"], "data\\ba_frostjoke.txt");
    assert_eq!(v["entries"][0]["size"], 5084);

    let o = rgrf(&["find", &grf, "--ext", "TXT", "--json"]);
    assert!(json(&o)["count"].as_u64().unwrap() > 10);

    // zlib entry and LZMA entry, hashes from tests/golden/cdata.tsv.gz (GRF Editor)
    let o = rgrf(&["cat", &grf, "DATA/BA_FROSTJOKE.TXT"]);
    assert_eq!((o.status.code(), o.stdout.len()), (Some(0), 5084));
    let o = rgrf(&["hash", &grf, "data\\ba_frostjoke.txt"]);
    assert!(
        String::from_utf8_lossy(&o.stdout).starts_with("c61b0d37 23d708c53b5d1a9747a3bf4ece77df94")
    );
    let o = rgrf(&["hash", &grf, "bookitemnametable.txt"]);
    assert!(
        String::from_utf8_lossy(&o.stdout).starts_with("0768bc76 44db51e449811db96976889db9650ad7")
    );

    let o = rgrf(&["cat", &grf, "data\\does_not_exist.txt"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(o.stdout.is_empty());
}

#[test]
fn info_ls_and_verify_on_a_0x300_archive() {
    let Some(grf) = testdata("0 GRFs/data kro latest.grf") else {
        return;
    };
    let v = json(&rgrf(&["info", &grf, "--json"]));
    assert_eq!(v["version"], "0x300");
    assert_eq!(
        (
            v["entries"].as_u64(),
            v["files"].as_u64(),
            v["markers"].as_u64()
        ),
        // the 30 flags-0 entries are PNGs stored uncompressed, not empty markers
        (Some(261_389), Some(261_389), Some(0))
    );
    assert_eq!(v["raw"], 30);
    assert_eq!(v["table"]["key_block"], 0);
    assert!(
        v["file_size"].as_u64().unwrap() < u64::from(u32::MAX),
        "file is just below 4 GiB"
    );
    assert!(v["table"]["offset"].as_u64().unwrap() > u64::from(u32::MAX) / 2);

    let o = rgrf(&["ls", &grf, "06guild_r.gat", "--json"]);
    assert_eq!(json(&o)["count"], 1);

    let o = rgrf(&["verify", &grf, "--json"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    let v = json(&o);
    assert_eq!(
        (v["errors"].as_u64(), v["warnings"].as_u64()),
        (Some(0), Some(0))
    );
}

#[test]
fn verify_exit_codes_clean_and_with_warning() {
    if let Some(grf) = testdata("0 GRFs/reference/cdata-2024-original.grf") {
        let o = rgrf(&["verify", &grf, "--json"]);
        assert_eq!(o.status.code(), Some(0));
        let v = json(&o);
        assert_eq!(
            (v["errors"].as_u64(), v["warnings"].as_u64()),
            (Some(0), Some(0))
        );
    }
    if let Some(grf) = testdata("0 OTHER CLIENTS/client-kro/rdata.grf") {
        let o = rgrf(&["verify", &grf, "--json"]);
        assert_eq!(o.status.code(), Some(2), "encrypted entry gives a warning");
        let v = json(&o);
        assert_eq!(v["warnings"], 1);
        assert_eq!(v["issues"][0]["kind"], "Encrypted");
        assert!(
            v["issues"][0]["entry"]["name"]
                .as_str()
                .unwrap()
                .ends_with("msgstring_kr_s.lub")
        );
    }
}

#[test]
fn extract_pattern_never_overwrites_and_reports() {
    let Some(grf) = testdata("0 GRFs/reference/cdata-2024-original.grf") else {
        return;
    };
    let dir = tmpdir("extract");
    let dest = dir.to_string_lossy().into_owned();
    let o = rgrf(&["extract", &grf, &dest, "frostjoke", "--json"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(json(&o)["written"], 1);
    assert_eq!(
        std::fs::metadata(dir.join("data/ba_frostjoke.txt"))
            .unwrap()
            .len(),
        5084
    );

    let o = rgrf(&["extract", &grf, &dest, "frostjoke"]);
    assert_eq!(
        o.status.code(),
        Some(1),
        "second run must refuse to overwrite"
    );
    assert!(String::from_utf8_lossy(&o.stderr).contains("already exists"));

    let o = rgrf(&[
        "extract",
        &grf,
        &dest,
        "frostjoke",
        "--on-collision",
        "skip",
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert_eq!(json(&o)["skipped_collisions"], 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repack_cdata_to_lzma_and_back_keeps_every_hash() {
    let Some(grf) = testdata("0 GRFs/reference/cdata-2024-original.grf") else {
        return;
    };
    let dir = tmpdir("repack");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("lzma.grf").to_string_lossy().into_owned();

    let o = rgrf(&[
        "repack", &grf, "-o", &out, "--method", "lzma", "--format", "0x300", "--json",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(
        (v["verified"].clone(), v["format"].clone()),
        (true.into(), "0x300".into())
    );
    assert!(
        v["output_file_bytes"].as_u64().unwrap() < 14_000_000,
        "LZMA shrinks cdata to about 13 MB"
    );

    // codecs and hashes seen by a separate run of the CLI
    let info = json(&rgrf(&["info", &out, "--codecs", "--json"]));
    assert_eq!(
        (info["version"].clone(), info["codecs"]["zlib"].clone()),
        ("0x300".into(), 0.into())
    );
    let hashes = |p: &str| {
        let o = rgrf(&["hash", p]);
        let mut lines: Vec<String> = String::from_utf8_lossy(&o.stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        lines.sort();
        lines
    };
    assert_eq!(
        hashes(&grf),
        hashes(&out),
        "same content hashes as the source"
    );

    // the source must never be touched, and existing outputs are not replaced
    let o = rgrf(&["repack", &grf, "-o", &grf]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("must not be the source"));
    let o = rgrf(&["repack", &grf, "-o", &out]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("already exists"));

    let o = rgrf(&["repack", &grf, "-o", &out, "--dry-run", "--json"]);
    assert_eq!(json(&o)["dry_run"], true);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn build_from_extracted_cdata_keeps_every_hash() {
    let Some(grf) = testdata("0 GRFs/reference/cdata-2024-original.grf") else {
        return;
    };
    let dir = tmpdir("build");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("src").to_string_lossy().into_owned();
    let out = dir.join("built.grf").to_string_lossy().into_owned();
    let o = rgrf(&["extract", &grf, &src, "--quiet"]);
    assert_eq!(o.status.code(), Some(0));

    let o = rgrf(&["build", &src, "-o", &out, "--json"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(
        (v["verified"].clone(), v["format"].clone()),
        (true.into(), "0x200".into())
    );
    let hashes = |p: &str| {
        let mut l: Vec<String> = String::from_utf8_lossy(&rgrf(&["hash", p]).stdout)
            .lines()
            .map(str::to_owned)
            .collect();
        l.sort();
        l
    };
    assert_eq!(hashes(&grf), hashes(&out));

    // safety: no replacing, no output inside the folder
    let o = rgrf(&["build", &src, "-o", &out]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("already exists"));
    let inside = format!("{src}/inside.grf");
    let o = rgrf(&["build", &src, "-o", &inside]);
    assert_eq!(o.status.code(), Some(1));
    assert!(!std::path::Path::new(&inside).exists());
    assert_eq!(
        json(&rgrf(&[
            "build",
            &src,
            "-o",
            &out,
            "--dry-run",
            "--overwrite",
            "--json"
        ]))["dry_run"],
        true
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn diff_merge_and_patch_on_small_archives() {
    let dir = tmpdir("merge");
    let put = |root: &str, name: &str, data: &str| {
        let p = dir.join(root).join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    put("base", "data/a.txt", "base a");
    put("base", "data/b.txt", "base b");
    put("custom", "data/b.txt", "custom b");
    put("custom", "data/c.txt", "custom c");
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    for (folder, grf) in [("base", "base.grf"), ("custom", "custom.grf")] {
        let o = rgrf(&["build", &p(folder), "-o", &p(grf), "--quiet"]);
        assert_eq!(o.status.code(), Some(0));
    }

    let o = rgrf(&["diff", &p("base.grf"), &p("base.grf")]);
    assert_eq!(o.status.code(), Some(0));
    let o = rgrf(&["diff", &p("base.grf"), &p("custom.grf"), "--json"]);
    assert_eq!(o.status.code(), Some(2), "differences give exit code 2");
    let v = json(&o);
    assert_eq!(
        (
            v["only_a"].clone(),
            v["only_b"].clone(),
            v["changed"].clone()
        ),
        (
            serde_json::json!(["data\\a.txt"]),
            serde_json::json!(["data\\c.txt"]),
            serde_json::json!(["data\\b.txt"])
        )
    );

    // the first archive wins
    let o = rgrf(&[
        "merge",
        &p("custom.grf"),
        &p("base.grf"),
        "-o",
        &p("merged.grf"),
        "--json",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(
        (v["files"].clone(), v["verified"].clone()),
        (3.into(), true.into())
    );
    assert_eq!(v["sources"][1]["overridden"], 1);
    let cat = rgrf(&["cat", &p("merged.grf"), "data\\b.txt"]);
    assert_eq!(String::from_utf8_lossy(&cat.stdout), "custom b");

    // --replace-only: the last archive is the base, custom.grf may replace but not add (c.txt)
    let o = rgrf(&[
        "merge",
        &p("custom.grf"),
        &p("base.grf"),
        "-o",
        &p("updated.grf"),
        "--replace-only",
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(0));
    let v = json(&o);
    assert_eq!(v["files"], 2);
    assert_eq!(v["sources"][0]["ignored_new"], 1);
    let cat = rgrf(&["cat", &p("updated.grf"), "data\\b.txt"]);
    assert_eq!(String::from_utf8_lossy(&cat.stdout), "custom b");

    // a patch from base to merged holds the new and the changed file
    let o = rgrf(&[
        "patch",
        &p("base.grf"),
        &p("merged.grf"),
        "-o",
        &p("patch.grf"),
        "--json",
    ]);
    assert_eq!(o.status.code(), Some(0));
    let v = json(&o);
    assert_eq!(
        (v["added"].clone(), v["changed"].clone(), v["files"].clone()),
        (1.into(), 1.into(), 2.into())
    );

    // safety: never over an input, never over an existing file
    let o = rgrf(&[
        "merge",
        &p("custom.grf"),
        &p("base.grf"),
        "-o",
        &p("base.grf"),
    ]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("must not be one of the input"));
    let o = rgrf(&["merge", &p("custom.grf"), "-o", &p("merged.grf")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("already exists"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn spr_info_export_build_round_trip() {
    use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr, Version};

    let mut pal = Box::new([0u8; PALETTE_LEN]);
    for i in 0..256 {
        pal[4 * i] = i as u8;
        pal[4 * i + 1] = 255 - i as u8;
        pal[4 * i + 2] = 9;
    }
    let spr = Spr {
        version: Version::V2_1,
        indexed: vec![
            IndexedImage {
                width: 3,
                height: 2,
                pixels: vec![0, 0, 5, 0, 9, 0],
            },
            IndexedImage {
                width: 1,
                height: 1,
                pixels: vec![200],
            },
        ],
        rgba: vec![RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
        }],
        palette: Some(pal),
        warnings: vec![],
    };
    let dir = tmpdir("spr");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("a.spr");
    let original = spr.to_bytes(Version::V2_1).unwrap();
    std::fs::write(&file, &original).unwrap();
    let f = file.to_str().unwrap();

    let o = rgrf(&["--json", "spr", "info", f]);
    assert!(o.status.success());
    let v = json(&o);
    assert_eq!(v["version"], "2.1");
    assert_eq!(v["indexed_images"], 2);
    assert_eq!(v["rgba_images"], 1);

    let frames = dir.join("frames");
    let fr = frames.to_str().unwrap();
    let o = rgrf(&["spr", "export", f, "-o", fr]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read_dir(&frames).unwrap().count(), 3);
    // never overwrites without --overwrite
    assert_eq!(rgrf(&["spr", "export", f, "-o", fr]).status.code(), Some(1));
    assert!(
        rgrf(&["spr", "export", f, "-o", fr, "--overwrite"])
            .status
            .success()
    );

    let out = dir.join("b.spr");
    let o = rgrf(&["spr", "build", fr, "-o", out.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::read(&out).unwrap(), original);
    assert_eq!(
        rgrf(&["spr", "build", fr, "-o", out.to_str().unwrap()])
            .status
            .code(),
        Some(1),
        "refuses to overwrite"
    );

    // not an SPR, and an unknown version string
    std::fs::write(dir.join("x.spr"), b"hello").unwrap();
    assert_eq!(
        rgrf(&["spr", "info", dir.join("x.spr").to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    assert!(
        !rgrf(&["spr", "build", fr, "-o", "/tmp/x", "--version", "9"])
            .status
            .success()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn spr_from_a_real_grf_entry() {
    let Some(data) = testdata("0 OTHER CLIENTS/client-kro/data.grf") else {
        return;
    };
    let o = rgrf(&[
        "--json",
        "spr",
        "info",
        &data,
        r"data\sprite\npc\4_cenere.spr",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(v["version"], "2.1");
    // measured: this sprite has only true-colour frames
    assert_eq!(v["indexed_images"], 0);
    assert_eq!(v["rgba_images"], 24);
}

#[test]
fn act_info_and_render_round_trip() {
    use rgrf_formats::act::{Act, Action, Frame, Layer, V2_5};
    use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, Spr, Version};

    let mut pal = Box::new([0u8; PALETTE_LEN]);
    pal[4..8].copy_from_slice(&[255, 0, 0, 0]);
    let spr = Spr {
        version: Version::V2_1,
        indexed: vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1; 4],
        }],
        rgba: vec![],
        palette: Some(pal),
        warnings: vec![],
    };
    let frame = |dx: f32| Frame {
        layers: vec![Layer {
            sprite_index: 0,
            offset_x: dx,
            ..Layer::default()
        }],
        ..Frame::default()
    };
    let act = Act {
        version: V2_5,
        reserved: [0; 10],
        actions: vec![Action {
            frames: vec![frame(0.0), frame(4.0)],
            interval: 4.0,
        }],
        sounds: vec![],
        trailing: vec![],
        warnings: vec![],
    };
    let dir = tmpdir("act");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.spr"), spr.to_bytes(Version::V2_1).unwrap()).unwrap();
    std::fs::write(dir.join("a.act"), act.to_bytes(V2_5).unwrap()).unwrap();
    let a = dir.join("a.act");
    let a = a.to_str().unwrap();

    let o = rgrf(&["--json", "act", "info", a]);
    assert!(o.status.success());
    let v = json(&o);
    assert_eq!(v["version"], "2.5");
    assert_eq!(v["frames"], 2);
    assert_eq!(v["layers"], 2);

    // one frame to a file, then every frame of the action into a folder (same canvas)
    let one = dir.join("one.png");
    let o = rgrf(&[
        "act",
        "render",
        a,
        "--frame",
        "1",
        "-o",
        one.to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(one.exists());
    assert_eq!(
        rgrf(&[
            "act",
            "render",
            a,
            "--frame",
            "1",
            "-o",
            one.to_str().unwrap()
        ])
        .status
        .code(),
        Some(1),
        "refuses to overwrite"
    );
    let all = dir.join("all");
    let o = rgrf(&["--json", "act", "render", a, "-o", all.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(v["width"], 6, "frames share one canvas: 2 px + 4 px travel");
    assert_eq!(std::fs::read_dir(&all).unwrap().count(), 2);

    // errors: missing action, missing sprite, not an ACT
    assert_eq!(
        rgrf(&["act", "render", a, "--action", "9", "-o", "/tmp/x"])
            .status
            .code(),
        Some(1)
    );
    std::fs::remove_file(dir.join("a.spr")).unwrap();
    assert_eq!(
        rgrf(&["act", "render", a, "-o", dir.join("z").to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    std::fs::write(dir.join("x.act"), b"hello").unwrap();
    assert_eq!(
        rgrf(&["act", "info", dir.join("x.act").to_str().unwrap()])
            .status
            .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn act_from_a_real_grf_entry() {
    let Some(data) = testdata("0 OTHER CLIENTS/client-kro/data.grf") else {
        return;
    };
    let act = r"data\sprite\npc\poring.act";
    let o = rgrf(&["--json", "act", "info", &data, act]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(json(&o)["frames"].as_u64().unwrap() > 0);
    let dir = tmpdir("actreal");
    std::fs::create_dir_all(&dir).unwrap();
    let o = rgrf(&[
        "act",
        "render",
        &data,
        act,
        "--frame",
        "0",
        "-o",
        dir.join("p.png").to_str().unwrap(),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(dir.join("p.png").metadata().unwrap().len() > 100);
    let _ = std::fs::remove_dir_all(&dir);
}
