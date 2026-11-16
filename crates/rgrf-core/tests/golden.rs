//! Compares rgrf-core with the GRF Editor reference in `tests/golden/` (see its README).
//!
//! Needs the real archives: set `RGRF_TESTDATA` to the `RO/1 CLIENT-SIDE` folder (this project's
//! `.cargo/config.toml` does that locally). Without it the tests are skipped with a message;
//! set `RGRF_REQUIRE_TESTDATA=1` to make that a failure. `RGRF_GOLDEN_FULL=1` also hashes
//! all 202,358 entries of the big `data.grf` (4 GB, takes a while).

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;

use rayon::prelude::*;
use rgrf_core::hash::Digest;
use rgrf_core::{Archive, DecodeStatus, NameMode};

const ARCHIVES: [(&str, &str); 7] = [
    ("cdata", "client-dev/full/cdata.grf"),
    ("core", "client-dev/full/core.grf"),
    ("cpal", "client-dev/full/cpal.grf"),
    ("rdata", "0 OTHER CLIENTS/client-kro/rdata.grf"),
    ("data", "0 OTHER CLIENTS/client-kro/data.grf"),
    // GRF 0x300 (Event Horizon), 3.8 GB and 5.7 GB
    ("data-kro-latest", "0 GRFs/data kro latest.grf"),
    ("data-merged", "0 GRFs/data (merged kro+chris).grf"),
];

struct Row {
    zsize: u32,
    zsize_aligned: u32,
    size: u32,
    flags: u8,
    offset: u64,
    crc32: String,
    md5: String,
}

fn archive_path(name: &str) -> Option<PathBuf> {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        assert!(
            std::env::var_os("RGRF_REQUIRE_TESTDATA").is_none(),
            "RGRF_TESTDATA is not set"
        );
        eprintln!("SKIPPED {name}: RGRF_TESTDATA is not set");
        return None;
    };
    let rel = ARCHIVES.iter().find(|(n, _)| *n == name).unwrap().1;
    let p = PathBuf::from(base).join(rel);
    assert!(p.exists(), "missing test archive {}", p.display());
    Some(p)
}

fn load_golden(name: &str) -> HashMap<String, Row> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/golden/{name}.tsv.gz"));
    let mut text = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut text)
        .unwrap();
    let mut rows = HashMap::new();
    for line in text.lines().skip(1) {
        let c: Vec<&str> = line.split('\t').collect();
        assert_eq!(c.len(), 10, "bad golden line: {line}");
        let flags: u8 = c[5].split(':').next().unwrap().parse().unwrap();
        let row = Row {
            zsize: c[2].parse().unwrap(),
            zsize_aligned: c[3].parse().unwrap(),
            size: c[4].parse().unwrap(),
            flags,
            offset: c[6].parse().unwrap(),
            crc32: c[7].to_owned(),
            md5: c[8].to_owned(),
        };
        assert!(
            rows.insert(c[0].to_owned(), row).is_none(),
            "duplicate golden path {}",
            c[0]
        );
    }
    rows
}

fn check_table(name: &str) {
    let Some(path) = archive_path(name) else {
        return;
    };
    let golden = load_golden(name);
    let started = std::time::Instant::now();
    let grf = Archive::open(&path).unwrap();
    eprintln!(
        "{name}: opened {} entries in {:?}",
        grf.len(),
        started.elapsed()
    );
    assert_eq!(grf.len(), golden.len(), "entry count");
    for e in grf.entries() {
        let shown = e.name.display(NameMode::Cp1252);
        let g = golden
            .get(&shown)
            .unwrap_or_else(|| panic!("not in golden: {shown:?}"));
        assert_eq!(
            (e.zsize, e.zsize_aligned, e.size, e.flags),
            (g.zsize, g.zsize_aligned, g.size, g.flags),
            "{shown}"
        );
        if !e.is_marker() {
            assert_eq!(e.abs_offset(), g.offset, "{shown}");
        }
    }
}

fn check_hashes(name: &str) {
    let Some(path) = archive_path(name) else {
        return;
    };
    let golden = load_golden(name);
    let grf = Archive::open(&path).unwrap();
    let problems: Vec<String> = (0..grf.len())
        .into_par_iter()
        .filter_map(|i| {
            let e = &grf.entries()[i];
            let shown = e.name.display(NameMode::Cp1252);
            let g = &golden[&shown];
            let d = grf.read(i).unwrap();
            let is_worm = shown.ends_with("worm_tail_damage.wav");
            match (&d.status, e.is_encrypted(), is_worm) {
                (DecodeStatus::Encrypted, true, _) => return None,
                (
                    DecodeStatus::Truncated {
                        got: 1403,
                        expected: 1410,
                    },
                    _,
                    true,
                ) => {
                    // GRF Editor returns empty data here; the tolerant reader keeps the 1403 bytes
                    return (d.data.len() != 1403).then(|| format!("{shown}: partial data length"));
                }
                (DecodeStatus::Ok, ..) => {}
                (other, ..) => return Some(format!("{shown}: unexpected status {other:?}")),
            }
            if e.is_marker() {
                return None;
            }
            let dig = Digest::of(&d.data);
            (dig.crc32_hex() != g.crc32 || dig.md5_hex() != g.md5).then(|| {
                format!(
                    "{shown}: crc {} md5 {} (golden {} {})",
                    dig.crc32_hex(),
                    dig.md5_hex(),
                    g.crc32,
                    g.md5
                )
            })
        })
        .collect();
    assert!(
        problems.is_empty(),
        "{} mismatches, first: {:?}",
        problems.len(),
        &problems[..problems.len().min(5)]
    );
}

#[test]
fn cdata_table() {
    check_table("cdata");
}
#[test]
fn core_table() {
    check_table("core");
}
#[test]
fn cpal_table() {
    check_table("cpal");
}
#[test]
fn rdata_table() {
    check_table("rdata");
}
#[test]
fn data_table() {
    check_table("data");
}
#[test]
fn cdata_hashes() {
    check_hashes("cdata");
}
#[test]
fn core_hashes() {
    check_hashes("core");
}
#[test]
fn cpal_hashes() {
    check_hashes("cpal");
}
#[test]
fn rdata_hashes() {
    check_hashes("rdata");
}
#[test]
fn data_hashes() {
    if std::env::var_os("RGRF_GOLDEN_FULL").is_none() {
        eprintln!("SKIPPED data_hashes: set RGRF_GOLDEN_FULL=1 (hashes 4 GB)");
        return;
    }
    check_hashes("data");
}
#[test]
fn data_kro_latest_table() {
    check_table("data-kro-latest");
}
#[test]
fn data_merged_table() {
    check_table("data-merged");
}
#[test]
fn data_kro_latest_hashes() {
    if std::env::var_os("RGRF_GOLDEN_FULL").is_none() {
        eprintln!("SKIPPED data_kro_latest_hashes: set RGRF_GOLDEN_FULL=1 (hashes 4 GB)");
        return;
    }
    check_hashes("data-kro-latest");
}
#[test]
fn data_merged_hashes() {
    if std::env::var_os("RGRF_GOLDEN_FULL").is_none() {
        eprintln!("SKIPPED data_merged_hashes: set RGRF_GOLDEN_FULL=1 (hashes 6 GB)");
        return;
    }
    check_hashes("data-merged");
}
