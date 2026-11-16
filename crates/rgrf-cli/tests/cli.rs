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
    let Some(grf) = testdata("client-dev/full/cdata.grf") else {
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
fn verify_exit_codes_clean_and_with_warning() {
    if let Some(grf) = testdata("client-dev/full/cdata.grf") {
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
    let Some(grf) = testdata("client-dev/full/cdata.grf") else {
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
