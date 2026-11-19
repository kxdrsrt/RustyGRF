//! Measures the BMP files of the real `data.grf` (needs `RGRF_TESTDATA`, see rgrf-core's golden
//! tests).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};

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
fn bmp_headers_of_the_client() {
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.bmp"), NameMode::Cp1252);
    let kinds = Mutex::new(BTreeMap::<String, usize>::new());
    idx.par_iter().for_each(|&i| {
        let d = arc.read(i).unwrap().data;
        let u16_at = |o: usize| u16::from_le_bytes([d[o], d[o + 1]]);
        let u32_at = |o: usize| u32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
        let key = if d.len() < 54 || &d[..2] != b"BM" {
            format!("not-bmp {:02x?}", &d[..d.len().min(4)])
        } else {
            let bits = u16_at(28);
            let used = u32_at(46);
            let h = u32_at(22) as i32;
            if bits == 8 {
                let used = if used == 0 {
                    "0".into()
                } else if used == 256 {
                    "256".into()
                } else {
                    "<256".to_string()
                };
                format!(
                    "bits=8 dib={} used={used} topdown={} pal-gap={}",
                    u32_at(14),
                    h < 0,
                    u32_at(10) as i64
                        - 54
                        - 4 * if u32_at(46) == 0 {
                            256
                        } else {
                            u32_at(46) as i64
                        }
                )
            } else {
                format!("bits={bits} dib={} comp={}", u32_at(14), u32_at(30))
            }
        };
        *kinds.lock().unwrap().entry(key).or_default() += 1;
    });
    for (k, n) in kinds.into_inner().unwrap() {
        println!("{n:>7}  {k}");
    }
}

/// Every 24-bit BMP reduced to 8 bits: how many qualify at which PSNR, and what it saves in the
/// archive (zlib 9, as `build` writes).
#[test]
fn reducing_the_true_colour_bmps() {
    use rgrf_core::encode::{Method, encode};
    use rgrf_formats::bmp::{Keep, Reduced, parse, reduce};
    use rgrf_formats::quantize::is_key;
    let Some(path) = data_grf() else { return };
    let arc = Archive::open(&path).unwrap();
    let idx = arc.select(Some("*.bmp"), NameMode::Cp1252);
    let t0 = std::time::Instant::now();
    // (psnr, stored size now, stored size reduced) of each 24-bit file that reduced
    let rows: Vec<(String, f64, u64, u64)> = idx
        .par_iter()
        .filter_map(|&i| {
            let e = &arc.entries()[i];
            let d = arc.read(i).unwrap().data;
            match reduce(&d, 0.0) {
                Reduced::Done { bytes, psnr, .. } => {
                    // the transparency survives exactly and nothing else becomes transparent
                    let (a, b) = (parse(&d).unwrap(), parse(&bytes).unwrap());
                    let px = |img: &rgrf_formats::spr::RgbaImage, j: usize| {
                        [
                            img.pixels[4 * j],
                            img.pixels[4 * j + 1],
                            img.pixels[4 * j + 2],
                        ]
                    };
                    for j in 0..a.pixels.len() / 4 {
                        let (x, y) = (px(&a, j), px(&b, j));
                        assert_eq!(is_key(x), is_key(y), "{}", e.name.display(NameMode::Cp1252));
                        assert!(!is_key(x) || x == y);
                    }
                    Some((
                        e.name.display(NameMode::Cp1252),
                        psnr,
                        u64::from(e.zsize),
                        encode(Method::DEFAULT, &bytes).len() as u64,
                    ))
                }
                Reduced::Kept(Keep::Indexed(_)) => None,
                Reduced::Kept(k) => Some((format!("kept {k:?}"), -1.0, 0, 0)),
            }
        })
        .collect();
    let secs = t0.elapsed().as_secs_f64();
    let kept: BTreeMap<String, usize> =
        rows.iter()
            .filter(|r| r.1 < 0.0)
            .fold(BTreeMap::new(), |mut m, r| {
                *m.entry(r.0.split('(').next().unwrap().to_string())
                    .or_default() += 1;
                m
            });
    println!(
        "reduced {} files in {secs:.1} s; kept: {kept:?}",
        rows.len()
    );
    let mib = |b: u64| b as f64 / (1 << 20) as f64;
    let total: u64 = rows.iter().map(|r| r.2).sum();
    println!("stored size of those now: {:.1} MiB", mib(total));
    for min in [f64::INFINITY, 45.0, 40.0, 38.0, 35.0, 30.0] {
        // only where the stored file really gets smaller, as the archive tool decides
        let take: Vec<_> = rows.iter().filter(|r| r.1 >= min && r.3 < r.2).collect();
        let (a, b): (u64, u64) = take.iter().fold((0, 0), |s, r| (s.0 + r.2, s.1 + r.3));
        println!(
            "psnr >= {min:>4}: {:>6} files, {:>7.1} -> {:>7.1} MiB stored (saves {:.1} MiB)",
            take.len(),
            mib(a),
            mib(b),
            mib(a.saturating_sub(b))
        );
    }
    let grown = rows.iter().filter(|r| r.1 >= 0.0 && r.3 >= r.2).count();
    println!("left out because 8 bits would be stored larger: {grown}");
    assert!(rows.len() > 30_000 && kept.values().sum::<usize>() < 200);
}
