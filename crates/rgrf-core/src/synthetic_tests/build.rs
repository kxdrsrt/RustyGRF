//! Tests of `build` on small directory trees.

use super::*;
use crate::build::{BuildOptions, build};
use crate::encode::Method;
use crate::writer::Format;
use std::fs;
use std::path::Path;

fn write(root: &Path, rel: &str, data: &[u8]) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, data).unwrap();
}

fn text(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| b"abcdefghij klmnopqrstuv"[(i * 7 + usize::from(seed) + i / 13) % 23])
        .collect()
}

fn tree(tag: &str) -> std::path::PathBuf {
    let root = tmpdir(tag).join("src");
    write(&root, "data/a.txt", &text(4000, 1));
    write(&root, "data/sprite/b.spr", &text(9000, 2));
    write(&root, "data/sprite/deep/er/c.act", &text(300, 3));
    write(&root, "data/empty.txt", b"");
    // CP1252 mojibake as produced by extracting Korean archives (bytes C0 AF C0 FA)
    write(
        &root,
        "data/texture/\u{c0}\u{af}\u{c0}\u{fa}/ui.bmp",
        &text(2500, 4),
    );
    root
}

fn opts(method: Method, format: Format) -> BuildOptions {
    BuildOptions {
        method,
        format,
        ..BuildOptions::default()
    }
}

fn names(g: &Archive<crate::io::Source>) -> Vec<Vec<u8>> {
    g.entries()
        .iter()
        .map(|e| e.name.as_bytes().to_vec())
        .collect()
}

#[test]
fn builds_a_readable_archive_with_correct_names_and_content() {
    for format in [Format::V200, Format::V300] {
        for method in [Method::Zlib { level: 9 }, Method::Lzma { preset: 5 }] {
            let root = tree("b1");
            let dst = root.parent().unwrap().join("out.grf");
            let r = build(&root, &dst, &opts(method, format), &Control::default()).unwrap();
            assert!(r.verified);
            assert_eq!((r.files, r.format), (5, format.version()));
            let g = Archive::open(&dst).unwrap();
            assert_eq!(g.header().version, format.version());
            let want: Vec<Vec<u8>> = vec![
                b"data\\a.txt".to_vec(),
                b"data\\empty.txt".to_vec(),
                b"data\\sprite\\b.spr".to_vec(),
                b"data\\sprite\\deep\\er\\c.act".to_vec(),
                b"data\\texture\\\xc0\xaf\xc0\xfa\\ui.bmp".to_vec(),
            ];
            assert_eq!(names(&g), want, "sorted by path, names as bytes");
            assert_eq!(g.read(0).unwrap().data, text(4000, 1));
            assert_eq!(g.read(1).unwrap().data, b"");
            assert_eq!(g.read(4).unwrap().data, text(2500, 4));
            assert!(g.entries().iter().all(|e| e.is_file()));
            let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
            assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
            fs::remove_dir_all(root.parent().unwrap()).unwrap();
        }
    }
}

#[test]
fn extract_then_build_gives_back_the_same_archive() {
    let root = tree("b2");
    let dir = root.parent().unwrap().to_path_buf();
    let o = BuildOptions {
        format: Format::V300,
        ..opts(Method::Zlib { level: 9 }, Format::V300)
    };
    build(&root, &dir.join("a.grf"), &o, &Control::default()).unwrap();
    let a = Archive::open(dir.join("a.grf")).unwrap();
    let all: Vec<usize> = (0..a.len()).collect();
    let out = dir.join("extracted");
    extract(
        &a,
        &all,
        &out,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap();
    build(&out, &dir.join("b.grf"), &o, &Control::default()).unwrap();
    assert_eq!(
        fs::read(dir.join("a.grf")).unwrap(),
        fs::read(dir.join("b.grf")).unwrap()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn output_is_reproducible_and_dry_run_writes_nothing() {
    let root = tree("b3");
    let dir = root.parent().unwrap().to_path_buf();
    for (n, m) in [
        Method::Zlib { level: 6 },
        Method::Zopfli { iterations: 2 },
        Method::Lzma { preset: 6 },
    ]
    .into_iter()
    .enumerate()
    {
        let o = opts(m, Format::V200);
        build(
            &root,
            &dir.join(format!("x{n}.grf")),
            &o,
            &Control::default(),
        )
        .unwrap();
        build(
            &root,
            &dir.join(format!("y{n}.grf")),
            &o,
            &Control::default(),
        )
        .unwrap();
        assert_eq!(
            fs::read(dir.join(format!("x{n}.grf"))).unwrap(),
            fs::read(dir.join(format!("y{n}.grf"))).unwrap(),
            "{m:?}"
        );
    }
    let dry = BuildOptions {
        dry_run: true,
        ..BuildOptions::default()
    };
    let r = build(&root, &dir.join("never.grf"), &dry, &Control::default()).unwrap();
    assert!(r.dry_run && !dir.join("never.grf").exists());
    assert_eq!((r.files, r.unpacked_bytes), (5, 4000 + 9000 + 300 + 2500));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn junk_and_symlinks_are_skipped_and_reported() {
    let root = tree("b4");
    write(&root, "data/.DS_Store", b"junk");
    write(&root, "data/._a.txt", b"junk");
    write(&root, "Thumbs.db", b"junk");
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("data/a.txt"), root.join("data/link.txt")).unwrap();
    let dir = root.parent().unwrap().to_path_buf();
    let r = build(
        &root,
        &dir.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(r.files, 5);
    let reasons: Vec<_> = r.skipped.iter().map(|s| s.reason).collect();
    assert!(
        reasons
            .iter()
            .filter(|x| **x == "file manager metadata")
            .count()
            == 3,
        "{reasons:?}"
    );
    #[cfg(unix)]
    assert!(reasons.contains(&"symbolic link"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn composed_and_decomposed_names_end_up_as_the_same_bytes() {
    let dir = tmpdir("b5");
    let root = dir.join("src");
    // "à" decomposed (a + combining grave) must become the single CP1252 byte 0xE0
    write(&root, "data/a\u{300}x.txt", b"nfd");
    build(
        &root,
        &dir.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let g = Archive::open(dir.join("o.grf")).unwrap();
    assert_eq!(g.entry(0).unwrap().name.as_bytes(), b"data\\\xe0x.txt");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn names_that_cannot_be_written_in_the_mode_are_an_error_and_other_modes_work() {
    let dir = tmpdir("b6");
    let root = dir.join("src");
    write(&root, "data/\u{65e5}\u{672c}.txt", b"x");
    let err = build(
        &root,
        &dir.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("cannot be turned")),
        "{err:?}"
    );
    assert!(!dir.join("o.grf").exists());
    let utf8 = BuildOptions {
        names: NameMode::Utf8,
        ..BuildOptions::default()
    };
    build(&root, &dir.join("o.grf"), &utf8, &Control::default()).unwrap();
    let g = Archive::open(dir.join("o.grf")).unwrap();
    assert_eq!(
        g.entry(0).unwrap().name.as_bytes(),
        "data\\日本.txt".as_bytes()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn paths_that_differ_only_in_case_are_refused_when_the_file_system_allows_them() {
    let dir = tmpdir("b7");
    let root = dir.join("src");
    write(&root, "data/A.txt", b"1");
    write(&root, "data/a.txt", b"2");
    let on_disk = fs::read_dir(root.join("data")).unwrap().count();
    if on_disk == 2 {
        let err = build(
            &root,
            &dir.join("o.grf"),
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap_err();
        assert!(
            matches!(err, Error::InvalidInput(ref m) if m.contains("same archive path")),
            "{err:?}"
        );
    } else {
        eprintln!("case-insensitive file system: collision cannot be created, assertion skipped");
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dedupe_shares_blocks_of_identical_files() {
    let dir = tmpdir("b8");
    let root = dir.join("src");
    let same = text(6000, 7);
    write(&root, "data/one.txt", &same);
    write(&root, "data/two.txt", &same);
    write(&root, "data/three.txt", &text(6000, 8));
    let plain = build(
        &root,
        &dir.join("p.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let ded = BuildOptions {
        dedupe: true,
        ..BuildOptions::default()
    };
    let r = build(&root, &dir.join("d.grf"), &ded, &Control::default()).unwrap();
    assert_eq!(
        (plain.blocks, plain.shared_files, r.blocks, r.shared_files),
        (3, 0, 2, 1)
    );
    assert!(r.output_file_len < plain.output_file_len && r.verified);
    let g = Archive::open(dir.join("d.grf")).unwrap();
    let by_name = |n: &[u8]| {
        g.entries()
            .iter()
            .find(|e| e.name.as_bytes() == n)
            .unwrap()
            .offset
    };
    assert_eq!(by_name(b"data\\one.txt"), by_name(b"data\\two.txt"));
    assert_ne!(by_name(b"data\\one.txt"), by_name(b"data\\three.txt"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsafe_requests_are_refused() {
    let root = tree("b9");
    let dir = root.parent().unwrap().to_path_buf();
    // output inside the packed folder
    let err = build(
        &root,
        &root.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("must not lie inside")),
        "{err:?}"
    );
    // existing output is kept unless --overwrite
    fs::write(dir.join("o.grf"), b"precious").unwrap();
    let err = build(
        &root,
        &dir.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::TargetExists(_)));
    assert_eq!(fs::read(dir.join("o.grf")).unwrap(), b"precious");
    // too small a format limit: error and nothing left behind
    let small = BuildOptions {
        overwrite: true,
        max_file_len: Some(300),
        ..BuildOptions::default()
    };
    let err = build(&root, &dir.join("o.grf"), &small, &Control::default()).unwrap_err();
    assert!(matches!(err, Error::TooLarge(_)), "{err:?}");
    assert_eq!(fs::read(dir.join("o.grf")).unwrap(), b"precious");
    let left: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(left.len(), 2, "src and o.grf only: {left:?}");
    // empty and missing folders
    let empty = dir.join("empty");
    fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        build(
            &empty,
            &dir.join("e.grf"),
            &BuildOptions::default(),
            &Control::default()
        ),
        Err(Error::InvalidInput(_))
    ));
    assert!(
        build(
            &dir.join("missing"),
            &dir.join("e.grf"),
            &BuildOptions::default(),
            &Control::default()
        )
        .is_err()
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn colliding_archive_paths_are_refused_on_every_file_system() {
    // a file literally named `data\a.txt` and the folder `data` with `a.txt` map to the same path
    let dir = tmpdir("b10");
    let root = dir.join("src");
    write(&root, "data/a.txt", b"1");
    fs::write(root.join("data\\a.txt"), b"2").unwrap();
    let err = build(
        &root,
        &dir.join("o.grf"),
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(ref m) if m.contains("same archive path")),
        "{err:?}"
    );
    assert!(!dir.join("o.grf").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn builds_legacy_archives_that_read_back() {
    for format in [Format::V102, Format::V103] {
        let root = tree("b-legacy");
        let dst = root.parent().unwrap().join("out.grf");
        let r = build(
            &root,
            &dst,
            &BuildOptions {
                dedupe: true,
                ..opts(Method::Zlib { level: 9 }, format)
            },
            &Control::default(),
        )
        .unwrap();
        assert!(r.verified);
        let g = Archive::open(&dst).unwrap();
        assert_eq!(g.header().version, format.version());
        assert_eq!(g.len(), 5);
        assert_eq!(g.read(0).unwrap().data, text(4000, 1));
        assert_eq!(g.read(1).unwrap().data, b"");
        assert_eq!(g.read(4).unwrap().data, text(2500, 4));
        // the .act file gets the head-only scrambling
        assert_eq!(g.entry(3).unwrap().flags, 0x05);
        assert_eq!(g.entry(0).unwrap().flags, 0x03);
        let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
        assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
        fs::remove_dir_all(root.parent().unwrap()).unwrap();
    }
}
