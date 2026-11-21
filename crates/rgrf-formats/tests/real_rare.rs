//! IMF, FNA, Thumbs.db, BSON and ICO files of the real clients (needs `RGRF_TESTDATA`, see
//! rgrf-core's golden tests).

use std::path::PathBuf;

use rayon::prelude::*;
use rgrf_core::{Archive, NameMode};
use rgrf_formats::{bson, fna, ico, imf, thumbsdb};

fn base() -> Option<PathBuf> {
    let Some(base) = std::env::var_os("RGRF_TESTDATA") else {
        assert!(
            std::env::var_os("RGRF_REQUIRE_TESTDATA").is_none(),
            "RGRF_TESTDATA is not set"
        );
        eprintln!("SKIPPED: RGRF_TESTDATA is not set");
        return None;
    };
    Some(PathBuf::from(base))
}

fn find(dir: &std::path::Path, ext: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            find(&p, ext, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext)) {
            out.push(p);
        }
    }
}

#[test]
fn every_imf_and_fna_of_the_real_archive_reads_and_fna_writes_back_the_same() {
    let Some(base) = base() else { return };
    let a = Archive::open(base.join("0 OTHER CLIENTS/client-kro/data.grf")).unwrap();
    let (mut imfs, mut fnas) = (0, 0);
    for (i, e) in a.entries().iter().enumerate() {
        let name = e.name.display(NameMode::Cp1252).to_ascii_lowercase();
        let is = |x: &str| name.starts_with("data\\imf\\") && name.ends_with(x);
        if !(is(".imf") || is(".fna")) {
            continue;
        }
        let data = a.read(i).unwrap().data;
        if is(".imf") {
            let f = imf::Imf::parse(&data).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(!f.layers.is_empty());
            imfs += 1;
        } else {
            let f = fna::Fna::parse(&data).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert_eq!(f.to_bytes(), data, "{name} does not write back the same");
            fnas += 1;
        }
    }
    eprintln!("{imfs} IMF and {fnas} FNA files read");
    assert!(imfs > 100 && fnas > 10);
}

#[test]
fn real_bson_files_go_through_json_to_the_same_bytes() {
    let Some(base) = base() else { return };
    let mut files = Vec::new();
    find(&base.join("kro-latest"), "bson", &mut files);
    assert!(!files.is_empty(), "no BSON files found");
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        let doc = bson::parse(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let json = bson::to_json(&doc);
        let back = bson::from_json(&json).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_eq!(back, doc, "{}", f.display());
        assert_eq!(bson::to_bytes(&back), bytes, "{}", f.display());
    }
}

#[test]
fn real_thumbs_db_files_list_their_pictures_with_jpeg_data() {
    let Some(base) = base() else { return };
    let mut files = Vec::new();
    find(&base.join("0 OTHER CLIENTS"), "db", &mut files);
    find(&base.join("client-dev"), "db", &mut files);
    files.retain(|f| {
        f.file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("thumbs.db"))
    });
    assert!(!files.is_empty(), "no Thumbs.db found");
    // (databases, thumbnails, thumbnails with a JPEG); the newer Windows versions made files without
    // the catalog of the old ones, and some thumbnails are stored in a way that is not read
    let (ok, thumbs, with_jpeg): (usize, usize, usize) = files
        .par_iter()
        .map(|f| match thumbsdb::parse(&std::fs::read(f).unwrap()) {
            Ok(db) => {
                let j = db.thumbs.iter().filter(|t| t.jpeg.is_some()).count();
                for t in db.thumbs.iter().filter_map(|t| t.jpeg.as_ref()) {
                    assert!(t.starts_with(&[0xff, 0xd8]), "{}", f.display());
                }
                (1, db.thumbs.len(), j)
            }
            Err(thumbsdb::ThumbsError::NoCatalog) => (0, 0, 0),
            Err(e) => panic!("{}: {e}", f.display()),
        })
        .reduce(|| (0, 0, 0), |a, b| (a.0 + b.0, a.1 + b.1, a.2 + b.2));
    let total = with_jpeg;
    assert!(ok > 0 && thumbs > 0, "no readable Thumbs.db");
    eprintln!(
        "{} Thumbs.db files, {ok} with a catalog, {thumbs} thumbnails, {with_jpeg} with a JPEG",
        files.len()
    );
    assert!(total > 0);
}

#[test]
fn real_icons_decode() {
    let Some(base) = base() else { return };
    for rel in [
        "0 OTHER CLIENTS/client-kro/ragnarok.ico",
        "1 CLIENT TOOLS/0 NEMO/Input/ro16.ico",
        "1 CLIENT TOOLS/0 WARP/Inputs/Sample.ico",
    ] {
        let bytes = std::fs::read(base.join(rel)).unwrap();
        let images = ico::parse(&bytes).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(!images.is_empty());
        for i in &images {
            assert!(i.width > 0 && i.height > 0, "{rel}");
        }
    }
}
