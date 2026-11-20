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
        // the one DES-scrambled entry (flags 0x03, msgstring_kr_s.lub) is decrypted and checked
        let o = rgrf(&["verify", &grf, "--json"]);
        assert_eq!(o.status.code(), Some(0));
        let v = json(&o);
        assert_eq!(
            (v["errors"].as_u64(), v["warnings"].as_u64()),
            (Some(0), Some(0))
        );
        let o = rgrf(&["info", &grf, "--json"]);
        let v = json(&o);
        assert_eq!(
            (v["encrypted"].as_u64(), v["des_scrambled"].as_u64()),
            (Some(0), Some(1))
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
fn edit_replaces_adds_and_removes_without_touching_the_original() {
    let dir = tmpdir("archive-edit");
    let put = |name: &str, data: &str| {
        let p = dir.join("src").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    };
    put("data/a.txt", "old a");
    put("data/b.txt", "old b");
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    assert_eq!(
        rgrf(&["build", &p("src"), "-o", &p("base.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );
    std::fs::write(dir.join("new.txt"), "new a").unwrap();
    std::fs::write(dir.join("extra.txt"), "extra").unwrap();
    let before = std::fs::read(p("base.grf")).unwrap();

    let o = rgrf(&[
        "edit",
        &p("base.grf"),
        "-o",
        &p("out.grf"),
        "--quiet",
        "--json",
        "--put",
        &format!("/data/a.txt={}", p("new.txt")),
        "--put",
        &format!("data\\c.txt={}", p("extra.txt")),
        "--remove",
        "DATA/B.TXT",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(
        (
            v["added"].clone(),
            v["replaced"].clone(),
            v["removed"].clone(),
            v["files"].clone(),
            v["verified"].clone()
        ),
        (1.into(), 1.into(), 1.into(), 2.into(), true.into())
    );
    assert_eq!(
        std::fs::read(p("base.grf")).unwrap(),
        before,
        "the original stays unchanged"
    );
    let cat = |n: &str| String::from_utf8(rgrf(&["cat", &p("out.grf"), n]).stdout).unwrap();
    assert_eq!(cat("data/a.txt"), "new a");
    assert_eq!(cat("data/c.txt"), "extra");
    assert!(!rgrf(&["cat", &p("out.grf"), "data/b.txt"]).status.success());

    // nothing to do, existing output and the source as output are refused
    assert_eq!(
        rgrf(&["edit", &p("base.grf"), "-o", &p("x.grf")])
            .status
            .code(),
        Some(1)
    );
    let put_arg = format!("data/a.txt={}", p("new.txt"));
    assert_eq!(
        rgrf(&[
            "edit",
            &p("base.grf"),
            "-o",
            &p("out.grf"),
            "--put",
            &put_arg
        ])
        .status
        .code(),
        Some(1)
    );
    assert_eq!(
        rgrf(&[
            "edit",
            &p("base.grf"),
            "-o",
            &p("base.grf"),
            "--overwrite",
            "--put",
            &put_arg
        ])
        .status
        .code(),
        Some(1)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// A 24-bit BMP (bottom-up) with the colour of each pixel from `f(x, y)`.
fn bmp24(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let stride = (w * 3).div_ceil(4) * 4;
    let mut v = b"BM".to_vec();
    v.extend((54 + stride * h).to_le_bytes());
    v.extend([0; 4]);
    v.extend(54u32.to_le_bytes());
    v.extend(40u32.to_le_bytes());
    v.extend(w.to_le_bytes());
    v.extend(h.to_le_bytes());
    v.extend(1u16.to_le_bytes());
    v.extend(24u16.to_le_bytes());
    v.extend([0; 24]);
    for y in (0..h).rev() {
        for x in 0..w {
            let c = f(x, y);
            v.extend([c[2], c[1], c[0]]);
        }
        v.resize(v.len() + (stride - w * 3) as usize, 0);
    }
    v
}

#[test]
fn client_unused_lists_files_no_text_names() {
    let dir = tmpdir("client-unused");
    let src = dir.join("src/data/texture/town");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(dir.join("src/data/luafiles514")).unwrap();
    for n in ["used.bmp", "lonely.bmp"] {
        std::fs::write(src.join(n), vec![7u8; 100]).unwrap();
    }
    std::fs::write(
        dir.join("src/data/luafiles514/t.lua"),
        "pic = \"data\\\\texture\\\\town\\\\Used.BMP\"",
    )
    .unwrap();
    let grf = dir.join("t.grf").to_string_lossy().into_owned();
    let s = dir.join("src").to_string_lossy().into_owned();
    assert_eq!(
        rgrf(&["build", &s, "-o", &grf, "--quiet"]).status.code(),
        Some(0)
    );
    let o = rgrf(&["client", "unused", &grf, "--json"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(v["files_looked_at"].as_u64(), Some(2));
    assert_eq!(v["unnamed_files"].as_u64(), Some(1));
    assert_eq!(
        v["folders"][0]["examples"][0],
        "data\\texture\\town\\lonely.bmp"
    );
}

#[test]
fn client_optimize_copies_the_folder_and_reduces_every_archive() {
    let dir = tmpdir("client-optimize");
    let src = dir.join("src/data");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("flat.bmp"),
        bmp24(128, 128, |x, y| [(x % 7) as u8 * 30, (y % 5) as u8 * 40, 9]),
    )
    .unwrap();
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    std::fs::create_dir_all(dir.join("client/System")).unwrap();
    std::fs::write(dir.join("client/System/a.txt"), "loose").unwrap();
    std::fs::write(dir.join("client/DATA.INI"), "1=base.grf\n").unwrap();
    assert_eq!(
        rgrf(&["build", &p("src"), "-o", &p("client/base.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );
    let (client, small) = (p("client"), p("small"));
    let run = |extra: &[&str]| {
        let mut a = vec!["client", "optimize", &client[..], "-o", &small[..]];
        a.extend(extra);
        rgrf(&a)
    };
    let o = run(&["--quiet"]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert_eq!(
        std::fs::read(dir.join("small/System/a.txt")).unwrap(),
        b"loose"
    );
    assert_eq!(
        std::fs::read(dir.join("small/DATA.INI")).unwrap(),
        b"1=base.grf\n"
    );
    let info = json(&rgrf(&[
        "image",
        "info",
        &p("small/base.grf"),
        "data/flat.bmp",
        "--json",
    ]));
    assert_eq!(info["bits"].as_u64(), Some(8));
    assert!(!dir.join("client/small").exists());
    assert_eq!(
        run(&["--quiet"]).status.code(),
        Some(1),
        "an existing output is kept"
    );
    let inside = rgrf(&[
        "client",
        "optimize",
        &p("client"),
        "-o",
        &p("client/new"),
        "--quiet",
    ]);
    assert_eq!(inside.status.code(), Some(1));
    assert!(!dir.join("client/new").exists());
}

#[test]
fn image_reduce_shrinks_true_colour_bmps_and_keeps_magenta() {
    let dir = tmpdir("image-reduce");
    let src = dir.join("src/data");
    std::fs::create_dir_all(&src).unwrap();
    // 200 colours scattered at random and a magenta corner: lossless; a smooth two-channel
    // gradient: lossy (about 45 dB); 2 x 2: not smaller
    let flat = bmp24(128, 128, |x, y| {
        let mut r = (x * 128 + y + 1) as u64;
        for _ in 0..3 {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
        }
        let r = (r % 200) as u8;
        if x < 8 && y < 8 {
            [255, 0, 255]
        } else {
            [r, r / 2, 255 - r]
        }
    });
    std::fs::write(src.join("flat.bmp"), &flat).unwrap();
    let grad = bmp24(64, 64, |x, y| [(x * 2) as u8, (y * 2) as u8, 0]);
    std::fs::write(src.join("grad.bmp"), &grad).unwrap();
    std::fs::write(
        src.join("tiny.bmp"),
        bmp24(2, 2, |x, y| [x as u8, y as u8, 0]),
    )
    .unwrap();
    std::fs::write(src.join("note.txt"), "not a picture").unwrap();
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    assert_eq!(
        rgrf(&["build", &p("src"), "-o", &p("base.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );

    let dry = rgrf(&[
        "image",
        "reduce",
        &p("base.grf"),
        "-o",
        &p("out.grf"),
        "--dry-run",
        "--json",
    ]);
    assert_eq!(
        dry.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    let v = json(&dry);
    assert_eq!(
        (
            v["matched"].as_u64(),
            v["reduced"].as_u64(),
            v["written"].as_bool()
        ),
        (Some(3), Some(2), Some(false))
    );
    assert!(!dir.join("out.grf").exists());

    let o = rgrf(&[
        "image",
        "reduce",
        &p("base.grf"),
        "-o",
        &p("out.grf"),
        "--min-psnr",
        "99",
        "--json",
        "--quiet",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(
        (v["reduced"].as_u64(), v["lossless"].as_u64()),
        (Some(1), Some(1))
    );
    assert_eq!(v["kept"]["below_min_psnr"].as_u64(), Some(1));
    assert_eq!(v["kept"]["not_smaller"].as_u64(), Some(1));
    assert!(v["stored_bytes_after"].as_u64() < v["stored_bytes_before"].as_u64());

    // the flat picture is 8-bit now, with the same colours and the magenta corner intact
    let info = |grf: &str| {
        json(&rgrf(&[
            "image",
            "info",
            &p(grf),
            "data/flat.bmp",
            "--json",
        ]))
    };
    let (a, b) = (info("base.grf"), info("out.grf"));
    assert_eq!(
        (a["bits"].as_u64(), b["bits"].as_u64()),
        (Some(24), Some(8))
    );
    assert_eq!(a["colors"], b["colors"]);
    assert_eq!(b["transparent_key_pixels"].as_u64(), Some(64));
    let same = rgrf(&["cat", &p("out.grf"), "data/note.txt"]);
    assert_eq!(same.stdout, b"not a picture");
    let again = rgrf(&[
        "image",
        "reduce",
        &p("base.grf"),
        "-o",
        &p("out.grf"),
        "--quiet",
    ]);
    assert_eq!(
        again.status.code(),
        Some(1),
        "an existing output is never replaced"
    );

    // convert: PNG out, and an 8-bit BMP with its PSNR
    let o = rgrf(&[
        "image",
        "convert",
        &p("base.grf"),
        "data/grad.bmp",
        "-o",
        &p("grad.png"),
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    assert!(
        std::fs::read(dir.join("grad.png"))
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    let o = rgrf(&[
        "image",
        "convert",
        &p("grad.png"),
        "-o",
        &p("grad8.bmp"),
        "--bits",
        "8",
        "--json",
    ]);
    let v = json(&o);
    assert!(v["psnr_db"].as_f64().is_some_and(|p| p > 30.0), "{v}");
    assert_eq!(std::fs::read(dir.join("grad8.bmp")).unwrap()[28], 8);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn client_refs_finds_what_animations_name_and_do_not_have() {
    use rgrf_formats::act::{Act, Action, Frame, Layer, V2_5, sound_from_name};
    use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, Spr, Version};

    let spr = Spr {
        version: Version::V2_1,
        indexed: vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1; 4],
        }],
        rgba: vec![],
        palette: Some(Box::new([0u8; PALETTE_LEN])),
        warnings: vec![],
    };
    let act = |image: i32, sounds: &[&str], plays: &[i32]| {
        let frame = |sound_id: i32| Frame {
            layers: vec![Layer {
                sprite_index: image,
                ..Layer::default()
            }],
            sound_id,
            ..Frame::default()
        };
        Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![Action {
                frames: plays.iter().map(|&p| frame(p)).collect(),
                interval: 4.0,
            }],
            sounds: sounds.iter().map(|n| sound_from_name(n).unwrap()).collect(),
            trailing: vec![],
            warnings: vec![],
        }
        .to_bytes(V2_5)
        .unwrap()
    };
    let dir = tmpdir("refs");
    let sprite = dir.join("src/data/sprite");
    std::fs::create_dir_all(&sprite).unwrap();
    std::fs::create_dir_all(dir.join("src/data/wav")).unwrap();
    let spr_bytes = spr.to_bytes(Version::V2_1).unwrap();
    for n in ["good", "badimage", "sound"] {
        std::fs::write(sprite.join(format!("{n}.spr")), &spr_bytes).unwrap();
    }
    std::fs::write(sprite.join("good.act"), act(0, &[], &[-1])).unwrap();
    std::fs::write(sprite.join("nospr.act"), act(0, &[], &[-1])).unwrap();
    std::fs::write(sprite.join("badimage.act"), act(3, &[], &[-1])).unwrap();
    // sound 0 has no file, sound 1 is the hit marker, sound 2 is there, 5 is outside the table
    std::fs::write(
        sprite.join("sound.act"),
        act(0, &["gone.wav", "atk", "here.wav"], &[0, 1, 2, 5]),
    )
    .unwrap();
    std::fs::write(dir.join("src/data/wav/here.wav"), b"RIFF").unwrap();
    let p = |n: &str| dir.join(n).to_string_lossy().into_owned();
    assert_eq!(
        rgrf(&["build", &p("src"), "-o", &p("a.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );

    let o = rgrf(&["client", "refs", &p("a.grf"), "--json"]);
    assert_eq!(
        o.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let v = json(&o);
    assert_eq!(v["animations_checked"], 4);
    let by = &v["by_kind"];
    assert_eq!(
        (
            by["missing_sprite"].as_u64(),
            by["image_out_of_range"].as_u64(),
            by["missing_sound"].as_u64(),
            by["sound_out_of_range"].as_u64()
        ),
        (Some(1), Some(1), Some(1), Some(1)),
        "{v}"
    );
    assert_eq!(v["problems"], 4);
    // a folder without the problem files is clean
    std::fs::remove_file(sprite.join("nospr.act")).unwrap();
    std::fs::remove_file(sprite.join("badimage.act")).unwrap();
    std::fs::remove_file(sprite.join("sound.act")).unwrap();
    assert_eq!(
        rgrf(&["build", &p("src"), "-o", &p("b.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );
    assert_eq!(
        rgrf(&["client", "refs", &p("b.grf"), "--quiet"])
            .status
            .code(),
        Some(0)
    );
    let _ = std::fs::remove_dir_all(&dir);
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

#[test]
fn spr_and_act_edit_commands() {
    use rgrf_formats::act::{Act, Action, Frame, Layer, V2_5};
    use rgrf_formats::png_io::rgba_to_png;
    use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr, Version};

    let mut pal = Box::new([0u8; PALETTE_LEN]);
    pal[4..8].copy_from_slice(&[255, 0, 0, 0]); // 1 red
    pal[8..12].copy_from_slice(&[0, 255, 0, 0]); // 2 green
    let one = |v: u8| IndexedImage {
        width: 1,
        height: 1,
        pixels: vec![v],
    };
    let spr = Spr {
        version: Version::V2_1,
        indexed: vec![one(1), one(2), one(1)],
        rgba: vec![],
        palette: Some(pal),
        warnings: vec![],
    };
    let layer = |i: i32| Layer {
        sprite_index: i,
        ..Layer::default()
    };
    let act = Act {
        version: V2_5,
        reserved: [0; 10],
        actions: vec![
            Action {
                frames: vec![Frame {
                    layers: vec![layer(0), layer(1), layer(2)],
                    ..Frame::default()
                }],
                interval: 4.0,
            },
            Action::default(),
        ],
        sounds: vec![],
        trailing: vec![],
        warnings: vec![],
    };
    let dir = tmpdir("edit");
    std::fs::create_dir_all(&dir).unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();
    std::fs::write(p("a.spr"), spr.to_bytes(Version::V2_1).unwrap()).unwrap();
    std::fs::write(p("a.act"), act.to_bytes(V2_5).unwrap()).unwrap();
    // a nearly green true-colour PNG
    std::fs::write(
        p("g.png"),
        rgba_to_png(&RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![10, 250, 10, 255],
        })
        .unwrap(),
    )
    .unwrap();
    let read_spr = |n: &str| Spr::parse(&std::fs::read(p(n)).unwrap()).unwrap();
    let read_act = |n: &str| Act::parse(&std::fs::read(p(n)).unwrap()).unwrap();

    // true-colour PNG into a palette image needs --nearest
    let o = rgrf(&[
        "spr",
        "replace",
        &p("a.spr"),
        "--frame",
        "0",
        "--png",
        &p("g.png"),
        "-o",
        &p("r.spr"),
    ]);
    assert_eq!(o.status.code(), Some(1));
    let o = rgrf(&[
        "spr",
        "replace",
        &p("a.spr"),
        "--frame",
        "0",
        "--png",
        &p("g.png"),
        "--nearest",
        "-o",
        &p("r.spr"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(read_spr("r.spr").indexed[0].pixels, vec![2]);
    // never overwrites, never touches the input
    assert_eq!(
        rgrf(&[
            "spr",
            "replace",
            &p("a.spr"),
            "--frame",
            "0",
            "--png",
            &p("g.png"),
            "--nearest",
            "-o",
            &p("r.spr")
        ])
        .status
        .code(),
        Some(1)
    );
    assert_eq!(read_spr("a.spr").indexed[0].pixels, vec![1]);

    let o = rgrf(&[
        "spr",
        "add",
        &p("a.spr"),
        "--png",
        &p("g.png"),
        "--nearest",
        "-o",
        &p("add.spr"),
    ]);
    assert!(o.status.success());
    assert!(String::from_utf8_lossy(&o.stdout).contains("index 3"));
    assert_eq!(read_spr("add.spr").indexed.len(), 4);

    // removing image 1 empties the layer using it and moves the next one down
    let o = rgrf(&[
        "spr",
        "remove",
        &p("a.spr"),
        "--frame",
        "1",
        "--act",
        &p("a.act"),
        "--act-output",
        &p("rm.act"),
        "-o",
        &p("rm.spr"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(read_spr("rm.spr").indexed.len(), 2);
    let idx: Vec<i32> = read_act("rm.act").actions[0].frames[0]
        .layers
        .iter()
        .map(|l| l.sprite_index)
        .collect();
    assert_eq!(idx, vec![0, -1, 1]);
    assert_eq!(
        rgrf(&[
            "spr",
            "remove",
            &p("a.spr"),
            "--frame",
            "0",
            "--act",
            &p("a.act"),
            "-o",
            &p("x.spr")
        ])
        .status
        .code(),
        Some(2),
        "--act needs --act-output (clap usage error)"
    );

    let mut pal2 = vec![0u8; PALETTE_LEN];
    pal2[4..8].copy_from_slice(&[9, 9, 9, 0]);
    std::fs::write(p("x.pal"), &pal2).unwrap();
    let o = rgrf(&[
        "spr",
        "set-palette",
        &p("a.spr"),
        "--pal",
        &p("x.pal"),
        "-o",
        &p("pal.spr"),
    ]);
    assert!(o.status.success());
    assert_eq!(read_spr("pal.spr").palette.unwrap()[4], 9);
    std::fs::write(p("bad.pal"), b"short").unwrap();
    assert_eq!(
        rgrf(&[
            "spr",
            "set-palette",
            &p("a.spr"),
            "--pal",
            &p("bad.pal"),
            "-o",
            &p("p2.spr")
        ])
        .status
        .code(),
        Some(1)
    );

    // palette images to true-colour and back, the ACT following
    let o = rgrf(&[
        "spr",
        "convert",
        &p("a.spr"),
        "--to",
        "rgba",
        "--act",
        &p("a.act"),
        "--act-output",
        &p("cv.act"),
        "-o",
        &p("cv.spr"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let c = read_spr("cv.spr");
    assert!(c.indexed.is_empty() && c.palette.is_none());
    assert_eq!(c.rgba.len(), 3);
    assert_eq!(c.rgba[1].pixels, [0, 255, 0, 255]);
    let layers: Vec<(i32, i32)> = read_act("cv.act").actions[0].frames[0]
        .layers
        .iter()
        .map(|l| (l.sprite_index, l.sprite_type))
        .collect();
    assert_eq!(layers, [(0, 1), (1, 1), (2, 1)]);
    let o = rgrf(&[
        "spr",
        "convert",
        &p("cv.spr"),
        "--to",
        "indexed",
        "--json",
        "-o",
        &p("cv2.spr"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(v["details"]["lossless"], true);
    assert_eq!(read_spr("cv2.spr").indexed.len(), 3);
    // converting to the kind it already is
    assert_eq!(
        rgrf(&[
            "spr",
            "convert",
            &p("cv.spr"),
            "--to",
            "rgba",
            "-o",
            &p("cv3.spr")
        ])
        .status
        .code(),
        Some(1)
    );

    let o = rgrf(&[
        "act",
        "translate",
        &p("a.act"),
        "--dx",
        "-3",
        "--dy",
        "5",
        "-o",
        &p("t.act"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let l = &read_act("t.act").actions[0].frames[0].layers[0];
    assert_eq!((l.offset_x, l.offset_y), (-3.0, 5.0));
    assert!(
        rgrf(&[
            "act",
            "interval",
            &p("a.act"),
            "--action",
            "1",
            "--value",
            "9",
            "-o",
            &p("i.act")
        ])
        .status
        .success()
    );
    assert_eq!(read_act("i.act").actions[1].interval, 9.0);
    assert!(
        rgrf(&[
            "act",
            "copy-action",
            &p("a.act"),
            "--from",
            "0",
            "--to",
            "2",
            "-o",
            &p("c.act")
        ])
        .status
        .success()
    );
    assert_eq!(read_act("c.act").actions.len(), 3);
    assert!(
        rgrf(&[
            "act",
            "remove-action",
            &p("a.act"),
            "--action",
            "0",
            "-o",
            &p("d.act")
        ])
        .status
        .success()
    );
    assert_eq!(read_act("d.act").actions.len(), 1);
    assert_eq!(
        rgrf(&[
            "act",
            "remove-action",
            &p("a.act"),
            "--action",
            "7",
            "-o",
            &p("e.act")
        ])
        .status
        .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn act_gif_scale_and_with_parts() {
    use rgrf_formats::act::{Act, Action, Anchor, Frame, Layer, V2_5};
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
    let act_with_anchor = |ax: i32, frames: usize| Act {
        version: V2_5,
        reserved: [0; 10],
        actions: vec![Action {
            frames: (0..frames)
                .map(|_| Frame {
                    layers: vec![Layer {
                        sprite_index: 0,
                        ..Layer::default()
                    }],
                    anchors: vec![Anchor {
                        unknown: [0; 4],
                        x: ax,
                        y: 0,
                        other: 0,
                    }],
                    ..Frame::default()
                })
                .collect(),
            interval: 4.0,
        }],
        sounds: vec![],
        trailing: vec![],
        warnings: vec![],
    };
    let dir = tmpdir("gif");
    std::fs::create_dir_all(&dir).unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();
    let bytes = spr.to_bytes(Version::V2_1).unwrap();
    for n in ["body", "hat"] {
        std::fs::write(p(&format!("{n}.spr")), &bytes).unwrap();
    }
    std::fs::write(
        p("body.act"),
        act_with_anchor(10, 3).to_bytes(V2_5).unwrap(),
    )
    .unwrap();
    std::fs::write(p("hat.act"), act_with_anchor(4, 3).to_bytes(V2_5).unwrap()).unwrap();

    let o = rgrf(&[
        "--json",
        "act",
        "render",
        &p("body.act"),
        "--gif",
        "--scale",
        "3",
        "-o",
        &p("a.gif"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(&std::fs::read(p("a.gif")).unwrap()[..6], b"GIF89a");
    let v = json(&o);
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(6), Some(6))
    );

    // the hat is bound to the body by the anchors (10 - 4 = 6 px to the right), or not
    let o = rgrf(&[
        "--json",
        "act",
        "render",
        &p("body.act"),
        "--with",
        &p("hat.act"),
        "--frame",
        "0",
        "-o",
        &p("w.png"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(json(&o)["width"], 8);
    let o = rgrf(&[
        "--json",
        "act",
        "render",
        &p("body.act"),
        "--with",
        &p("hat.act"),
        "--no-anchor",
        "--frame",
        "0",
        "-o",
        &p("n.png"),
    ]);
    assert_eq!(json(&o)["width"], 2);

    // a GIF is a whole action, never a single frame; the output is never overwritten
    assert_eq!(
        rgrf(&[
            "act",
            "render",
            &p("body.act"),
            "--gif",
            "--frame",
            "0",
            "-o",
            &p("b.gif")
        ])
        .status
        .code(),
        Some(1)
    );
    assert_eq!(
        rgrf(&["act", "render", &p("body.act"), "--gif", "-o", &p("a.gif")])
            .status
            .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lua_check_on_files_folders_and_grfs() {
    let dir = tmpdir("lua");
    let data = dir.join("src/data/luafiles514");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join("good.lub"), "t = { a = 1, b = { 2, 3 } }\n").unwrap();
    std::fs::write(data.join("bad.lub"), "t = {\n  a = 1,\n  b = { 2, 3 }\n").unwrap();
    std::fs::write(data.join("compiled.lub"), b"\x1bLuaQ\x00\x01").unwrap();
    std::fs::write(data.join("readme.txt"), "not lua {{{").unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();

    // a single file: 0 for a good one, 3 for a bad one
    assert_eq!(
        rgrf(&["lua", "check", data.join("good.lub").to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );
    let o = rgrf(&["lua", "check", data.join("bad.lub").to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(3));
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("line 4"),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );

    // a folder: the .txt is ignored; the cut-off compiled file is damaged, not "not checked"
    let o = rgrf(&["--json", "lua", "check", &p("src")]);
    assert_eq!(o.status.code(), Some(3));
    let v = json(&o);
    assert_eq!(
        (
            v["checked"].as_u64(),
            v["valid"].as_u64(),
            v["bytecode"].as_u64()
        ),
        (Some(3), Some(1), Some(1))
    );
    assert_eq!(v["bytecode_valid"], 0);
    assert_eq!(v["encrypted"], 0);
    let failed = v["failed"].as_array().unwrap();
    assert_eq!(failed.len(), 2);
    assert!(
        failed
            .iter()
            .any(|f| f["line"] == 4 && f["compiled"] == false)
    );
    assert!(failed.iter().any(|f| f["compiled"] == true));

    // inside a GRF, with and without a pattern
    let grf = p("t.grf");
    assert!(
        rgrf(&["build", &p("src"), "-o", &grf, "-q"])
            .status
            .success()
    );
    let o = rgrf(&["--json", "lua", "check", &grf]);
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(json(&o)["checked"], 3);
    let o = rgrf(&["--json", "lua", "check", &grf, "good"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(json(&o)["checked"], 1);
    assert_eq!(
        rgrf(&["lua", "check", &p("missing.lub")]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn lua_check_on_the_real_core_grf() {
    let Some(core) = testdata("client-dev/full/core.grf") else {
        return;
    };
    let o = rgrf(&["--json", "lua", "check", &core]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    let v = json(&o);
    assert_eq!(v["valid"], 427);
    assert_eq!(v["bytecode"], 0);
}

#[test]
fn client_check_finds_the_usual_traps() {
    let dir = tmpdir("client");
    let w = |rel: &str, text: &str| {
        let p = dir.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    };
    let conn = |name: &str, addr: &str| {
        format!(
            "<clientinfo><connection><balloon>{name}</balloon><address>{addr}</address><port>6900</port></connection></clientinfo>"
        )
    };
    // a.grf (index 1) and b.grf (index 2) overlap in data\x.txt, and disagree about the server
    w("src-a/data/x.txt", "short");
    w("src-a/data/sclientinfo.xml", &conn("Live", "10.0.0.1"));
    w("src-b/data/x.txt", "a longer text than in a");
    w("src-b/data/sclientinfo.xml", &conn("Live", "10.0.0.2"));
    w("src-b/data/luafiles514/bad.lub", "t = {\n a = 1\n");
    w("src-b/data/luafiles514/good.lub", "t = { a = 1 }\n");
    w("data/x.txt", "loose copy that the archive hides");
    w("data/only.txt", "loose and used");
    w(
        "DATA.INI",
        "[Data]\r\n1=a.grf\r\n2=b.grf\r\n3=missing.grf\r\n",
    );
    std::fs::write(
        dir.join("exe-with-systemen.exe"),
        b"MZ.. SystemEN\\Navi_Data.lub ..",
    )
    .unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();
    for n in ["a", "b"] {
        assert!(
            rgrf(&[
                "build",
                &p(&format!("src-{n}")),
                "-o",
                &p(&format!("{n}.grf")),
                "-q"
            ])
            .status
            .success()
        );
    }
    std::fs::write(dir.join("spare.grf"), std::fs::read(p("a.grf")).unwrap()).unwrap();

    let o = rgrf(&["--json", "client", "check", dir.to_str().unwrap()]);
    assert_eq!(
        o.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    let v = json(&o);
    assert_eq!(
        v["archives"].as_array().unwrap().len(),
        2,
        "the missing archive is not listed"
    );
    let all: Vec<String> = v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            format!(
                "{}: {}",
                f["level"].as_str().unwrap(),
                f["message"].as_str().unwrap()
            )
        })
        .collect();
    let has = |needle: &str| all.iter().any(|m| m.contains(needle));
    assert!(
        has("error: DATA.INI 3=missing.grf but the file does not exist"),
        "{all:?}"
    );
    assert!(
        has(
            "b.grf: 2 of 4 files are hidden by archives with a lower index (1 of them with a different size)"
        ),
        "{all:?}"
    );
    assert!(
        has("warning: 1 loose files are ignored") && has("data\\x.txt (hidden by a.grf)"),
        "{all:?}"
    );
    assert!(
        has("1 loose files, 1 used") || has("2 loose files, 1 used"),
        "{all:?}"
    );
    assert!(
        has("sclientinfo.xml (used: a.grf): Live 10.0.0.1:6900"),
        "{all:?}"
    );
    assert!(
        has("other copies list other servers (b.grf: Live 10.0.0.2:6900)"),
        "{all:?}"
    );
    assert!(
        has("error: Lua syntax: ") && has("bad.lub") && has("line 3"),
        "{all:?}"
    );
    assert!(
        has("spare.grf is in the folder but not in DATA.INI"),
        "{all:?}"
    );
    assert!(has("mentions SystemEN"), "{all:?}");

    // --no-lua skips the syntax check; fixing everything gives exit code 0 or 2 (warnings only)
    let o = rgrf(&[
        "--json",
        "client",
        "check",
        dir.to_str().unwrap(),
        "--no-lua",
    ]);
    assert!(!String::from_utf8_lossy(&o.stdout).contains("Lua syntax"));
    // not a client folder
    assert_eq!(
        rgrf(&["client", "check", &p("src-a")]).status.code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn client_check_on_the_real_full_client() {
    let Some(full) = testdata("client-dev/full") else {
        return;
    };
    let o = rgrf(&["--json", "client", "check", &full]);
    let v = json(&o);
    assert_ne!(
        o.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
    let names: Vec<&str> = v["archives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["file"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["cdata.grf", "core.grf", "cpal.grf", "data.grf"]);
    let text = v["findings"].to_string();
    assert!(text.contains("Live Server 185.163.118.80:6900"), "{text}");
    assert!(text.contains("0 with syntax errors"), "{text}");
}

#[test]
fn map_info_png_and_check() {
    use rgrf_formats::gat::{Cell, Gat};
    use rgrf_formats::rsw::{EffectObject, Light, ModelObject, Object, Rsw, SoundObject, Water};
    use rgrf_formats::version::Version;

    fn name<const N: usize>(s: &str) -> [u8; N] {
        let mut a = [0u8; N];
        a[..s.len()].copy_from_slice(s.as_bytes());
        a
    }
    let dir = tmpdir("map");
    let w = |rel: &str, bytes: &[u8]| {
        let p = dir.join("src").join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    };
    let cell = |kind| Cell {
        heights: [0.0, 1.0, 2.0, 3.0],
        kind,
    };
    let gat = Gat {
        version: Version::new(1, 2),
        width: 2,
        height: 2,
        cells: vec![cell(0), cell(1), cell(3), cell(5)],
    };
    let model = |file: &str| {
        Object::Model(Box::new(ModelObject {
            name: [0; 40],
            animation_type: 0,
            animation_speed: 1.0,
            block_type: 0,
            unknown_byte: 0,
            unknown_int: 0,
            model_name: name(file),
            node_name: name("n"),
            position: [0.0; 3],
            rotation: [0.0; 3],
            scale: [1.0; 3],
        }))
    };
    let rsw = Rsw {
        version: Version::new(2, 1),
        build_number: 0,
        unknown_byte: 0,
        ini: name("x.ini"),
        ground: name("town.gnd"),
        altitude: name("town.gat"),
        source: [0; 40],
        water: Some(Water {
            level: 1.0,
            kind: 0,
            wave_height: 0.0,
            wave_speed: 0.0,
            wave_pitch: 0.0,
            texture_cycling: 0,
        }),
        light: Some(Light {
            longitude: 45,
            latitude: 45,
            diffuse: [1.0; 3],
            ambient: [0.3; 3],
            intensity: 0.5,
        }),
        bounds: Some([-500, 500, -500, 500]),
        extra_ints: vec![],
        objects: vec![
            model("house.rsm"),
            model("tree.rsm"),
            Object::Sound(SoundObject {
                name: [0; 80],
                wave_name: name("wind.wav"),
                position: [0.0; 3],
                volume: 1.0,
                width: 1,
                height: 1,
                range: 5.0,
                cycle: 4.0,
            }),
            Object::Effect(EffectObject {
                name: [0; 80],
                position: [0.0; 3],
                effect_number: 1,
                emit_speed: 1.0,
                params: [0.0; 4],
            }),
        ],
        tail: vec![],
    };
    // a GND header with two textures
    let mut gnd = b"GRGN\x01\x07".to_vec();
    gnd.extend(2i32.to_le_bytes());
    gnd.extend(2i32.to_le_bytes());
    gnd.extend(10.0f32.to_le_bytes());
    gnd.extend(2i32.to_le_bytes());
    gnd.extend(80i32.to_le_bytes());
    for t in ["town\\grass.bmp", "town\\lost.bmp"] {
        let mut p = vec![0u8; 80];
        p[..t.len()].copy_from_slice(t.as_bytes());
        gnd.extend(p);
    }
    // one lightmap, one tile, 2 x 2 cubes standing 4 units high
    gnd.extend(1i32.to_le_bytes());
    for v in [8i32, 8, 1] {
        gnd.extend(v.to_le_bytes());
    }
    gnd.extend([128u8; 256]);
    gnd.extend(1i32.to_le_bytes());
    gnd.extend([0u8; 32]);
    gnd.extend([0u8; 4]);
    gnd.extend([200u8; 4]);
    for _ in 0..4 {
        for v in [-4.0f32; 4] {
            gnd.extend(v.to_le_bytes());
        }
        for t in [0i32, 0, -1] {
            gnd.extend(t.to_le_bytes());
        }
    }
    w("data/town.gat", &gat.to_bytes());
    w("data/town.rsw", &rsw.to_bytes());
    w("data/town.gnd", &gnd);
    w("data/texture/town/grass.bmp", b"x"); // town\lost.bmp is missing
    // house.rsm is a valid (empty) 1.4 model using the texture town\roof.bmp, which does not exist
    let mut house = b"GRSM\x01\x04".to_vec();
    house.extend(0i32.to_le_bytes()); // animation length
    house.extend(0i32.to_le_bytes()); // shade type
    house.push(255); // alpha
    house.extend([0u8; 16]);
    house.extend(1i32.to_le_bytes()); // one texture
    let mut tex = vec![0u8; 40];
    tex[..14].copy_from_slice(b"town\\roof.bmp\0");
    house.extend(tex);
    house.extend([0u8; 40]); // main mesh name
    house.extend(0i32.to_le_bytes()); // no meshes
    house.extend(0i32.to_le_bytes()); // scale key frames
    house.extend(0i32.to_le_bytes()); // volume boxes
    w("data/model/house.rsm", &house); // tree.rsm is missing
    // wind.wav is missing too
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();
    let grf = p("m.grf");
    assert!(
        rgrf(&["build", &p("src"), "-o", &grf, "-q"])
            .status
            .success()
    );

    let o = rgrf(&["--json", "map", "info", &grf, r"data\town.gat"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(
        (v["width"].as_u64(), v["height"].as_u64()),
        (Some(2), Some(2))
    );
    assert_eq!(v["types"].as_array().unwrap().len(), 4);
    // change cells: the whole 2 x 2 map to water at height 7, in a new file
    let o = rgrf(&[
        "--json",
        "map",
        "gat-set",
        &grf,
        r"data\town.gat",
        "--from",
        "0,0",
        "--to",
        "1,1",
        "--type",
        "3",
        "--height",
        "7",
        "-o",
        &p("set.gat"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(json(&o)["details"]["cells_changed"].as_u64().unwrap() >= 1);
    let v = json(&rgrf(&["--json", "map", "info", &p("set.gat")]));
    assert_eq!(v["types"].as_array().unwrap().len(), 1);
    assert_eq!(v["height_min"], 7.0);
    // outside the map, and nothing to change, are usage errors
    for args in [vec!["--from", "9,9", "--type", "0"], vec!["--from", "0,0"]] {
        let mut a = vec![
            "map",
            "gat-set",
            &grf,
            r"data\town.gat",
            "-o",
            "/tmp/never.gat",
        ];
        a.extend(args);
        assert_eq!(rgrf(&a).status.code(), Some(1));
    }
    let v = json(&rgrf(&["--json", "map", "info", &grf, r"data\town.rsw"]));
    assert_eq!(v["objects"]["models"], 2);
    assert_eq!(v["ground"], "town.gnd");
    let v = json(&rgrf(&["--json", "map", "info", &grf, r"data\town.gnd"]));
    assert_eq!(v["textures"][1], "town\\lost.bmp");
    assert_eq!(
        rgrf(&["map", "info", &grf, r"data\town.rsw.bak"])
            .status
            .code(),
        Some(1)
    );

    let o = rgrf(&[
        "--json",
        "map",
        "png",
        &grf,
        r"data\town.gat",
        "--scale",
        "4",
        "-o",
        &p("t.png"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(
        (json(&o)["width"].as_u64(), json(&o)["height"].as_u64()),
        (Some(8), Some(8))
    );
    assert_eq!(
        rgrf(&["map", "png", &grf, r"data\town.gat", "-o", &p("t.png")])
            .status
            .code(),
        Some(1)
    );

    let o = rgrf(&["--json", "map", "check", &grf]);
    assert_eq!(o.status.code(), Some(3));
    let v = json(&o);
    assert_eq!(v["maps_checked"], 1);
    assert_eq!(v["problems"], 4);
    let text = v["maps_with_problems"].to_string();
    for needle in [
        "texture data\\\\texture\\\\town\\\\lost.bmp",
        "model data\\\\model\\\\tree.rsm",
        "sound data\\\\wav\\\\wind.wav",
        "model data\\\\model\\\\house.rsm uses the texture data\\\\texture\\\\town\\\\roof.bmp",
    ] {
        assert!(text.contains(needle), "{needle} in {text}");
    }
    assert!(!text.contains("grass.bmp"));
    // a named map that does not exist
    assert_eq!(
        rgrf(&["map", "check", &grf, "nowhere"]).status.code(),
        Some(1)
    );

    // flat: without options the ground is flat, without light, shadows, objects and water
    let o = rgrf(&["--json", "map", "flat", &grf, "town", "-o", &p("flat.grf")]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(
        (v["changed_maps"].as_u64(), v["files"].as_u64()),
        (Some(1), Some(3))
    );
    let cat = |path: &str| rgrf(&["cat", &p("flat.grf"), path]).stdout;
    let g = rgrf_formats::gnd_map::parse(&cat(r"data\town.gnd")).unwrap();
    assert!(
        g.cubes
            .iter()
            .all(|c| c.height == [0.0; 4] && c.front == -1)
    );
    assert_eq!(g.lightmaps[..64], [255u8; 64]);
    assert_eq!(g.tiles[0].color, [200; 4], "colours stay unless asked");
    let r = Rsw::parse(&cat(r"data\town.rsw")).unwrap();
    assert!(r.objects.is_empty());
    assert_eq!(r.water.unwrap().level, 100.0);
    // only the colour: the GAT and RSW stay as they are and are not written
    let o = rgrf(&[
        "--json",
        "map",
        "flat",
        &grf,
        "-o",
        &p("color.grf"),
        "--remove-color",
    ]);
    assert_eq!(json(&o)["files"], 1);
    // an existing output needs --overwrite
    assert_eq!(
        rgrf(&["map", "flat", &grf, "-o", &p("flat.grf")])
            .status
            .code(),
        Some(1)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn map_check_on_the_real_full_client() {
    let Some(full) = testdata("client-dev/full") else {
        return;
    };
    let o = rgrf(&[
        "--json", "map", "check", &full, "prontera", "payon", "geffen",
    ]);
    assert_eq!(
        o.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    let v = json(&o);
    assert_eq!(v["maps_checked"], 3);
    assert!(v["files_looked_up"].as_u64().unwrap() > 500);
}

/// `x = 1` as Lua 5.1 bytecode (LOADK, SETGLOBAL, RETURN), written by hand.
fn tiny_lub() -> Vec<u8> {
    let mut b = b"\x1bLuaQ\x00\x01\x04\x04\x04\x08\x00".to_vec();
    let s = |v: &mut Vec<u8>, t: &str| {
        v.extend(((t.len() + 1) as u32).to_le_bytes());
        v.extend(t.as_bytes());
        v.push(0);
    };
    s(&mut b, "@t.lua");
    b.extend([0u8; 8]); // line defined, last line
    b.extend([0, 0, 2, 2]); // upvalues, params, vararg, stack
    b.extend(3u32.to_le_bytes());
    for i in [1u32, 7 | (1 << 14), 30 | (1 << 23)] {
        b.extend(i.to_le_bytes());
    }
    b.extend(2u32.to_le_bytes());
    b.push(3);
    b.extend(1.0f64.to_le_bytes());
    b.push(4);
    s(&mut b, "x");
    b.extend([0u8; 16]); // protos, lines, locals, upvalue names
    b
}

#[test]
fn lua_decompile_prints_source_and_refuses_text() {
    let dir = tmpdir("decompile");
    std::fs::create_dir_all(&dir).unwrap();
    let lub = dir.join("t.lub");
    std::fs::write(&lub, tiny_lub()).unwrap();
    let o = rgrf(&["lua", "decompile", lub.to_str().unwrap()]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let out = String::from_utf8_lossy(&o.stdout);
    assert!(out.contains("x = 1"), "{out}");
    let o = rgrf(&["lua", "decompile", "--listing", lub.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&o.stdout).contains("SETGLOBAL"));
    // to a file, which is never overwritten
    let target = dir.join("t.lua");
    let o = rgrf(&[
        "lua",
        "decompile",
        lub.to_str().unwrap(),
        "-o",
        target.to_str().unwrap(),
    ]);
    assert!(o.status.success());
    assert!(std::fs::read_to_string(&target).unwrap().contains("x = 1"));
    assert!(
        !rgrf(&[
            "lua",
            "decompile",
            lub.to_str().unwrap(),
            "-o",
            target.to_str().unwrap()
        ])
        .status
        .success()
    );
    // plain text is refused
    let txt = dir.join("p.lub");
    std::fs::write(&txt, "x = 1").unwrap();
    assert!(
        !rgrf(&["lua", "decompile", txt.to_str().unwrap()])
            .status
            .success()
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn thor_and_rgz_patches_are_created_opened_and_merged() {
    let dir = tmpdir("patches");
    let root = dir.join("root/data/sprite");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("a.txt"), b"alpha".repeat(500)).unwrap();
    std::fs::write(dir.join("root/data/b.txt"), b"beta").unwrap();
    let (src, thor, rgz) = (
        dir.join("root").to_string_lossy().into_owned(),
        dir.join("p.thor").to_string_lossy().into_owned(),
        dir.join("p.rgz").to_string_lossy().into_owned(),
    );
    let o = rgrf(&[
        "thor",
        "create",
        &src,
        "-o",
        &thor,
        "--merge-into-grf",
        "--target",
        "data.grf",
        "--remove",
        "data\\gone.txt",
        "--json",
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(
        (v["files"].as_u64(), v["removals"].as_u64()),
        (Some(2), Some(1))
    );
    let o = rgrf(&["info", &thor, "--json"]);
    let v = json(&o);
    assert_eq!(v["kind"], "thor");
    assert_eq!(v["removals"], 1);
    assert_eq!(v["patch"]["target_grf"], "data.grf");
    let o = rgrf(&["ls", &thor, "--json"]);
    let v = json(&o);
    let kinds: Vec<&str> = v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds.iter().filter(|k| **k == "remove").count(), 1);
    assert_eq!(rgrf(&["verify", &thor]).status.code(), Some(0));
    let out = dir.join("out");
    let o = rgrf(&["extract", &thor, &out.to_string_lossy()]);
    assert!(o.status.success());
    assert_eq!(std::fs::read(out.join("data/b.txt")).unwrap(), b"beta");
    assert!(!out.join("data/gone.txt").exists());
    // RGZ from the Thor patch, then both merged into a GRF
    assert!(rgrf(&["rgz", "create", &thor, "-o", &rgz]).status.success());
    let o = rgrf(&["info", &rgz, "--json"]);
    assert_eq!(json(&o)["kind"], "rgz");
    let grf = dir.join("m.grf").to_string_lossy().into_owned();
    let o = rgrf(&["merge", &rgz, &thor, "-o", &grf, "-q"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(json(&rgrf(&["info", &grf, "--json"]))["entries"], 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_old_event_grf_0x102_opens_verifies_and_converts() {
    let Some(grf) = testdata("0 OTHER CLIENTS/RO O.G Client/Ragnarök Online/event.grf") else {
        return;
    };
    let v = json(&rgrf(&["info", &grf, "--json"]));
    assert_eq!(
        (
            v["version"].as_str(),
            v["entries"].as_u64(),
            v["des_scrambled"].as_u64()
        ),
        (Some("0x102"), Some(39), Some(39))
    );
    let o = rgrf(&["verify", &grf, "--json"]);
    assert_eq!(o.status.code(), Some(0));
    let v = json(&o);
    assert_eq!(
        (v["errors"].as_u64(), v["warnings"].as_u64()),
        (Some(0), Some(0))
    );
    let dir = tmpdir("event");
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("event200.grf").to_string_lossy().into_owned();
    assert!(rgrf(&["repack", &grf, "-o", &out, "-q"]).status.success());
    let o = rgrf(&["diff", &grf, &out, "--json"]);
    let v = json(&o);
    assert_eq!(
        (v["same"].as_u64(), v["changed"].as_array().map(Vec::len)),
        (Some(39), Some(0))
    );
    assert_eq!(json(&rgrf(&["info", &out, "--json"]))["version"], "0x200");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lua_compile_makes_client_ready_bytecode_and_round_trips_korean_text() {
    let dir = tmpdir("luacompile");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.lua");
    std::fs::write(
        &src,
        "T = { name = \"유저인터페이스\", n = 3 + 4 }\nfunction f(a)\n\treturn a .. T.name\nend\n",
    )
    .unwrap();
    let out = dir.join("t.lub");
    let (s, o) = (
        src.to_string_lossy().into_owned(),
        out.to_string_lossy().into_owned(),
    );
    let r = rgrf(&["lua", "compile", &s, "-o", &o, "--json"]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(json(&r)["functions"], 2);
    let bytes = std::fs::read(&out).unwrap();
    // header of the client's files: Lua 5.1, little endian, int 4, size_t 4, instruction 4, double
    assert_eq!(&bytes[..12], b"\x1bLuaQ\x00\x01\x04\x04\x04\x08\x00");
    // the Korean text is CP949 in the file, and shows as Korean again when decompiled
    assert!(bytes.windows(2).any(|w| w == [0xc0, 0xaf]));
    let r = rgrf(&["lua", "decompile", &o]);
    let text = String::from_utf8_lossy(&r.stdout);
    assert!(
        text.contains("유저인터페이스") && text.contains("n = 7"),
        "{text}"
    );
    assert_eq!(rgrf(&["lua", "check", &o]).status.code(), Some(0));
    // never over an existing file; syntax errors name file, line and token
    assert!(!rgrf(&["lua", "compile", &s, "-o", &o]).status.success());
    std::fs::write(&src, "x = 1\ny = = 2\n").unwrap();
    let r = rgrf(&[
        "lua",
        "compile",
        &s,
        "-o",
        &dir.join("bad.lub").to_string_lossy(),
        "--overwrite",
    ]);
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stderr).contains("t.lua:2: unexpected symbol near '='"));
    assert!(!dir.join("bad.lub").exists());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lua_compile_version_5_0_writes_the_older_format() {
    let dir = tmpdir("luacompile50");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("t.lua");
    std::fs::write(
        &src,
        "function f(a, ...)\n\tfor i = 1, a do print(i, arg.n) end\n\treturn a\nend\n",
    )
    .unwrap();
    let out = dir.join("t.lub");
    let (s, o) = (
        src.to_string_lossy().into_owned(),
        out.to_string_lossy().into_owned(),
    );
    let r = rgrf(&[
        "lua",
        "compile",
        &s,
        "-o",
        &o,
        "--lua-version",
        "5.0",
        "--json",
    ]);
    assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
    assert_eq!(json(&r)["functions"], 2);
    // Lua 5.0, little endian, int 4, size_t 4, instruction 4, OP/A/B/C 6/8/9/9, double
    let bytes = std::fs::read(&out).unwrap();
    assert_eq!(
        &bytes[..14],
        b"\x1bLua\x50\x01\x04\x04\x04\x06\x08\x09\x09\x08"
    );
    assert_eq!(rgrf(&["lua", "check", &o]).status.code(), Some(0));
    let listing = rgrf(&["lua", "decompile", &o, "--listing"]);
    assert!(String::from_utf8_lossy(&listing.stdout).contains("FORLOOP"));
    // 5.0 grammar: no `#` operator
    std::fs::write(&src, "x = #t\n").unwrap();
    let r = rgrf(&[
        "lua",
        "compile",
        &s,
        "-o",
        &o,
        "--lua-version",
        "5.0",
        "--overwrite",
    ]);
    assert_eq!(r.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&r.stderr).contains("t.lua:1: unexpected symbol near `#'"));
    let _ = std::fs::remove_dir_all(dir);
}

fn files_below(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            v.extend(files_below(&p));
        } else {
            v.push(p);
        }
    }
    v.sort();
    v
}

/// Files that GRF Editor's own `GrfCL` wrote (Thor, RGZ, GPF, GRF from the same folder, made in the
/// Windows VM by tools/golden/run-patch-in-vm.sh) open here and hold exactly the folder's files.
#[test]
fn patch_files_written_by_grf_editor_are_read_correctly() {
    let Some(src) = testdata("0 GRFs/reference/patchformats/src") else {
        return;
    };
    let out = testdata("0 GRFs/reference/patchformats/out").unwrap();
    let src = std::path::Path::new(&src).join("data");
    let want: Vec<_> = files_below(&src);
    assert!(want.len() >= 5);
    for name in ["ref.thor", "ref.rgz", "ref.gpf", "ref.grf"] {
        let file = format!("{out}/{name}");
        assert!(std::path::Path::new(&file).exists(), "missing {file}");
        let v = json(&rgrf(&["info", &file, "--json"]));
        assert_eq!(v["entries"].as_u64(), Some(want.len() as u64), "{name}");
        assert_eq!(
            v["kind"],
            match name {
                "ref.thor" => "thor",
                "ref.rgz" => "rgz",
                _ => "grf",
            }
        );
        let o = rgrf(&["verify", &file, "--json"]);
        let r = json(&o);
        assert_eq!(
            (r["errors"].as_u64(), r["warnings"].as_u64()),
            (Some(0), Some(0)),
            "{name}"
        );
        let dest = tmpdir(&format!("gc-{name}"));
        assert!(
            rgrf(&["extract", &file, &dest.to_string_lossy(), "-q"])
                .status
                .success()
        );
        // GrfCL added the folder `data` below the GRF path `data`: data\data\...
        let got = files_below(&dest.join("data/data"));
        assert_eq!(got.len(), want.len(), "{name}");
        for (g, w) in got.iter().zip(&want) {
            assert_eq!(
                std::fs::read(g).unwrap(),
                std::fs::read(w).unwrap(),
                "{name}: {}",
                g.display()
            );
        }
        let _ = std::fs::remove_dir_all(dest);
    }
}

#[test]
fn grf_editor_encryption_end_to_end() {
    let dir = tmpdir("crypt");
    std::fs::create_dir_all(dir.join("src/data")).unwrap();
    std::fs::write(dir.join("src/data/a.txt"), "hello hello hello hello\n").unwrap();
    std::fs::write(dir.join("src/data/b.txt"), "second second second second\n").unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();
    let pw = ["--password", "geheim123"];
    let with = |extra: &[&str]| -> Vec<String> {
        pw.iter()
            .chain(extra)
            .map(|s| (*s).to_owned())
            .collect::<Vec<_>>()
    };
    let run = |extra: &[&str]| {
        let v = with(extra);
        rgrf(&v.iter().map(String::as_str).collect::<Vec<_>>())
    };

    // build encrypted; without the key the files are listed as encrypted and not extracted
    let o = run(&["--encrypt", "build", &p("src"), "-o", &p("e.grf"), "-q"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&rgrf(&["--json", "info", &p("e.grf")]));
    assert_eq!(v["grf_editor_encryption"]["encrypted_entries"], 2);
    assert_eq!(v["grf_editor_encryption"]["key_given"], false);
    let o = rgrf(&["extract", &p("e.grf"), &p("out1")]);
    assert!(
        String::from_utf8_lossy(&o.stdout).contains("2 encrypted"),
        "{}",
        String::from_utf8_lossy(&o.stdout)
    );
    assert!(!dir.join("out1/data/a.txt").exists());

    // with the key (password or key file) everything is read; a wrong one is refused
    let o = run(&["extract", &p("e.grf"), &p("out2")]);
    assert!(o.status.success());
    assert_eq!(
        std::fs::read(dir.join("out2/data/a.txt")).unwrap(),
        std::fs::read(dir.join("src/data/a.txt")).unwrap()
    );
    let o = run(&["key", "create", "-o", &p("k.grfkey")]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert_eq!(std::fs::metadata(dir.join("k.grfkey")).unwrap().len(), 260);
    let o = rgrf(&[
        "--key-file",
        &p("k.grfkey"),
        "cat",
        &p("e.grf"),
        r"data\b.txt",
    ]);
    assert_eq!(o.stdout, b"second second second second\n");
    let o = rgrf(&["--password", "another one", "verify", &p("e.grf")]);
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("does not belong"));

    // a repack with the key decrypts, with --encrypt it encrypts again
    let o = run(&["repack", &p("e.grf"), "-o", &p("plain.grf"), "-q"]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(
        json(&rgrf(&["--json", "info", &p("plain.grf")]))
            .get("grf_editor_encryption")
            .is_none()
    );
    let o = run(&[
        "--encrypt",
        "repack",
        &p("plain.grf"),
        "-o",
        &p("again.grf"),
        "-q",
    ]);
    assert!(o.status.success());
    // --encrypt without a key is a usage error
    let o = rgrf(&["--encrypt", "repack", &p("plain.grf"), "-o", &p("x.grf")]);
    assert_eq!(o.status.code(), Some(1));

    // the client side: needs GRF Editor's cps.dll (here a stand-in with the placeholder)
    let mut dll = vec![5u8; 300];
    dll.extend_from_slice(b"uncompress\0");
    dll.extend_from_slice(b"TKTK");
    dll.extend(vec![0u8; 520]);
    std::fs::write(dir.join("cps.dll"), &dll).unwrap();
    let mut exe = vec![1u8; 200];
    exe.extend_from_slice(b"cps.dll\0uncompress\0");
    std::fs::write(dir.join("client.exe"), &exe).unwrap();
    let o = run(&[
        "--json",
        "client",
        "encrypt",
        &p("client.exe"),
        "--cps-dll",
        &p("cps.dll"),
        "-o",
        &p("patched"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    let dll_name = v["dll_name"].as_str().unwrap();
    assert!(dll_name.ends_with(".dll") && dir.join("patched").join(dll_name).exists());
    assert!(dir.join("patched/client.exe").exists());
    // without --cps-dll the built-in GRF Editor template is used
    let o = run(&[
        "--json",
        "client",
        "encrypt",
        &p("client.exe"),
        "-o",
        &p("patched2"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let name = json(&o)["dll_name"].as_str().unwrap().to_owned();
    assert_eq!(
        std::fs::metadata(dir.join("patched2").join(name))
            .unwrap()
            .len(),
        176128
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn image_reduce_dir_writes_an_optimised_copy_and_leaves_the_source_alone() {
    use rgrf_formats::spr::RgbaImage;
    let dir = tmpdir("reduce-dir");
    let src = dir.join("grf/data/texture");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::create_dir_all(dir.join("grf/data/empty")).unwrap();
    // 200 colours at random: lossless as 8 bits and smaller
    let mut pixels = Vec::new();
    for i in 0..128u64 * 128 {
        let mut r = i + 1;
        for _ in 0..3 {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
        }
        let r = (r % 200) as u8;
        pixels.extend([r, r / 2, 255 - r, 255]);
    }
    let bmp = rgrf_formats::bmp::write_rgb(&RgbaImage {
        width: 128,
        height: 128,
        pixels,
    });
    std::fs::write(src.join("a.BMP"), &bmp).unwrap();
    std::fs::write(src.join("note.txt"), b"text").unwrap();
    let p = |n: &str| dir.join(n).to_str().unwrap().to_owned();

    let o = rgrf(&[
        "--json",
        "image",
        "reduce-dir",
        &p("grf"),
        "-o",
        &p("grf-opt"),
    ]);
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let v = json(&o);
    assert_eq!(
        (v["files"].clone(), v["reduced"].clone()),
        (2.into(), 1.into())
    );
    assert!(v["compressed_bytes_after"].as_u64() < v["compressed_bytes_before"].as_u64());
    let out = std::fs::read(dir.join("grf-opt/data/texture/a.BMP")).unwrap();
    assert!(out.len() < bmp.len());
    assert_eq!(rgrf_formats::bmp::info(&out).unwrap().bits, 8);
    assert_eq!(
        std::fs::read(dir.join("grf-opt/data/texture/note.txt")).unwrap(),
        b"text"
    );
    assert!(dir.join("grf-opt/data/empty").is_dir());
    // the source is untouched
    assert_eq!(std::fs::read(src.join("a.BMP")).unwrap(), bmp);

    // an output that has files needs --overwrite; one inside the source is refused
    assert_eq!(
        rgrf(&["image", "reduce-dir", &p("grf"), "-o", &p("grf-opt")])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        rgrf(&["image", "reduce-dir", &p("grf"), "-o", &p("grf/inner")])
            .status
            .code(),
        Some(1)
    );
    assert!(
        rgrf(&[
            "image",
            "reduce-dir",
            &p("grf"),
            "-o",
            &p("grf-opt"),
            "--overwrite"
        ])
        .status
        .success()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
