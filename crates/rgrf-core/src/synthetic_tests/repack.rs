//! Tests of `repack` on small archives built in memory.

use super::*;
use crate::codec::{Codec, sniff};
use crate::encode::Method;
use crate::repack::{
    Dedupe, RepackOptions, Sel, Selection, plan_units, prepare, repack, verify_output,
};
use crate::writer::Format;

fn text(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| b"abcdefghij klmnopqrstuv"[(i * 7 + usize::from(seed) + i / 13) % 23])
        .collect()
}

fn noise(n: usize) -> Vec<u8> {
    let mut x = 0x9E37_79B9_7F4A_7C15u64;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn opts(method: Method, format: Format) -> RepackOptions {
    RepackOptions {
        method,
        format: Some(format),
        ..RepackOptions::default()
    }
}

fn run(
    src: &Archive<Vec<u8>>,
    tag: &str,
    o: &RepackOptions,
) -> (
    crate::repack::RepackReport,
    Archive<crate::io::Source>,
    std::path::PathBuf,
) {
    let dir = tmpdir(tag);
    let dst = dir.join("out.grf");
    let r = repack(src, &dst, o, &Control::default()).unwrap();
    let a = Archive::open(&dst).unwrap();
    (r, a, dir)
}

fn mixed_source() -> Archive<Vec<u8>> {
    let mut lz = file("data\\lz.act", &text(9000, 3));
    lz.stream = Stream::Lzma(text(9000, 3));
    let mut korean = file("x", &text(4000, 5));
    korean.name = b"data\\sprite\\\xc0\xaf\xc0\xfa.spr".to_vec();
    open(&[
        file("data\\a.txt", &text(5000, 1)),
        lz,
        marker("data\\dir.txt"),
        file("data\\empty.txt", b""),
        file("data\\noise.png", &noise(20_000)),
        korean,
        file("DATA\\Upper.TXT", b"x"),
    ])
}

fn assert_same_content(src: &Archive<Vec<u8>>, out: &Archive<crate::io::Source>) {
    assert_eq!(src.len(), out.len());
    for i in 0..src.len() {
        let (a, b) = (src.entry(i).unwrap(), out.entry(i).unwrap());
        assert_eq!(a.name, b.name, "entry {i} name bytes");
        assert_eq!(
            (a.size, a.is_marker()),
            (b.size, b.is_marker()),
            "entry {i}"
        );
        let (x, y) = (src.read(i).unwrap(), out.read(i).unwrap());
        assert_eq!((x.status, x.data), (y.status, y.data), "entry {i} content");
    }
}

#[test]
fn every_method_and_format_keeps_names_order_and_content() {
    let src = mixed_source();
    let methods = [
        Method::Zlib { level: 6 },
        Method::Zopfli { iterations: 2 },
        Method::Lzma { preset: 5 },
    ];
    for (n, m) in methods.into_iter().enumerate() {
        for format in [Format::V200, Format::V300] {
            let o = RepackOptions {
                recompress_all: true,
                ..opts(m, format)
            };
            let (r, out, dir) = run(&src, &format!("rt{n}{format:?}"), &o);
            assert!(r.verified, "{m:?} {format:?}");
            assert_eq!(out.header().version, format.version());
            assert_eq!((r.entries, r.markers, r.files), (7, 1, 6));
            assert_same_content(&src, &out);
            for e in out.entries().iter().filter(|e| e.is_file() && e.size > 0) {
                let want = if m.codec() == Codec::Lzma {
                    [Codec::Lzma, Codec::Zlib]
                } else {
                    [Codec::Zlib, Codec::Zlib]
                };
                let i = out.entries().iter().position(|x| x == e).unwrap();
                assert!(
                    want.contains(&out.sniff_codec(i).unwrap().unwrap()),
                    "{m:?}"
                );
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[test]
fn zlib_target_leaves_no_lzma_entries_and_incompressible_data_is_stored() {
    let src = mixed_source();
    let (r, out, dir) = run(
        &src,
        "nolzma",
        &opts(Method::Zlib { level: 9 }, Format::V200),
    );
    for i in 0..out.len() {
        assert_ne!(out.sniff_codec(i).unwrap(), Some(Codec::Lzma), "entry {i}");
    }
    // PNG-like noise does not grow beyond stored zlib
    let e = out.entry(4).unwrap();
    assert!(e.zsize <= e.size + 6 + 5 * e.size.div_ceil(16_384));
    assert!(r.recompressed >= 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn original_streams_are_kept_when_same_codec_and_not_larger() {
    let src = open(&[
        file("data\\a.txt", &text(5000, 1)),
        file("data\\b.txt", &text(7000, 2)),
    ]);
    // level 9 source, level 1 target: the better original wins
    let (r, out, dir) = run(&src, "keep", &opts(Method::Zlib { level: 1 }, Format::V200));
    assert_eq!((r.kept_original, r.recompressed), (2, 0));
    for i in 0..2 {
        assert_eq!(src.read_raw(i).unwrap(), out.read_raw(i).unwrap());
    }
    std::fs::remove_dir_all(dir).unwrap();

    let forced = RepackOptions {
        recompress_all: true,
        ..opts(Method::Zlib { level: 1 }, Format::V200)
    };
    let (r, _, dir) = run(&src, "force", &forced);
    assert_eq!((r.kept_original, r.recompressed), (0, 2));
    std::fs::remove_dir_all(dir).unwrap();
}

fn dedupe_source() -> Archive<Vec<u8>> {
    let a = text(6000, 1);
    let mut shared = file("data\\a_shared.txt", &a);
    shared.share_with = Some(0);
    open(&[
        file("data\\a.txt", &a),
        shared,
        file("data\\a_copy.txt", &a), // same content, separate block
        file("data\\other.txt", &text(6000, 9)),
        marker("data\\dir.txt"),
    ])
}

#[test]
fn dedupe_levels_share_blocks_as_documented_and_output_shrinks() {
    let src = dedupe_source();
    let mut sizes = Vec::new();
    for (d, blocks, shared) in [
        (Dedupe::None, 4, 0),
        (Dedupe::Source, 3, 1),
        (Dedupe::Content, 2, 2),
    ] {
        let o = RepackOptions {
            dedupe: d,
            ..opts(Method::Zlib { level: 9 }, Format::V300)
        };
        let (r, out, dir) = run(&src, &format!("dd{d:?}"), &o);
        assert_eq!((r.blocks, r.shared_files), (blocks, shared), "{d:?}");
        assert!(r.verified);
        assert_same_content(&src, &out);
        let offsets: Vec<u64> = (0..3).map(|i| out.entry(i).unwrap().offset).collect();
        match d {
            Dedupe::None => assert!(offsets[0] != offsets[1] && offsets[1] != offsets[2]),
            Dedupe::Source => assert!(offsets[0] == offsets[1] && offsets[1] != offsets[2]),
            Dedupe::Content => assert!(offsets[0] == offsets[1] && offsets[1] == offsets[2]),
        }
        sizes.push(r.output_file_len);
        std::fs::remove_dir_all(dir).unwrap();
    }
    assert!(sizes[0] > sizes[1] && sizes[1] > sizes[2], "{sizes:?}");
}

#[test]
fn encrypted_and_damaged_entries_are_copied_unchanged() {
    let data = noise(2000);
    let stored = tests_support::zlib_level(&data, 0);
    let mut damaged = file("data\\wav\\worm.wav", &data);
    damaged.stream = Stream::Literal(stored[..stored.len() - 11].to_vec());
    let mut enc = file("data\\enc.lub", &text(3000, 4));
    enc.flags = flags::FILE | flags::GRF_EDITOR_CRYPT;
    enc.stream = Stream::Literal((0..40u8).collect());
    let src = open(&[file("data\\ok.txt", &text(3000, 2)), damaged, enc]);
    for format in [Format::V200, Format::V300] {
        let (r, out, dir) = run(
            &src,
            &format!("raw{format:?}"),
            &opts(Method::Lzma { preset: 4 }, format),
        );
        assert_eq!((r.damaged.clone(), r.encrypted_blocks), (vec![1], 1));
        assert!(r.verified);
        assert_eq!(
            src.read_raw(1).unwrap(),
            out.read_raw(1).unwrap(),
            "damaged stream unchanged"
        );
        assert_eq!(
            src.read_raw_aligned(2).unwrap(),
            out.read_raw_aligned(2).unwrap(),
            "encrypted bytes unchanged"
        );
        let (a, b) = (src.entry(2).unwrap(), out.entry(2).unwrap());
        assert_eq!(
            (a.flags, a.zsize, a.zsize_aligned, a.size),
            (b.flags, b.zsize, b.zsize_aligned, b.size)
        );
        assert_eq!(out.read(2).unwrap().status, DecodeStatus::Encrypted);
        assert_eq!(src.read(1).unwrap().data, out.read(1).unwrap().data);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn output_is_reproducible_byte_for_byte() {
    let src = mixed_source();
    for (n, m) in [
        Method::Zopfli { iterations: 3 },
        Method::Lzma { preset: 6 },
        Method::Zlib { level: 9 },
    ]
    .into_iter()
    .enumerate()
    {
        let o = RepackOptions {
            recompress_all: true,
            dedupe: Dedupe::Content,
            ..opts(m, Format::V200)
        };
        let (_, _, d1) = run(&src, &format!("rep1{n}"), &o);
        let (_, _, d2) = run(&src, &format!("rep2{n}"), &o);
        assert_eq!(
            std::fs::read(d1.join("out.grf")).unwrap(),
            std::fs::read(d2.join("out.grf")).unwrap(),
            "{m:?}"
        );
        std::fs::remove_dir_all(d1).unwrap();
        std::fs::remove_dir_all(d2).unwrap();
    }
}

#[test]
fn existing_target_and_format_limit_leave_nothing_behind() {
    let src = mixed_source();
    let dir = tmpdir("safety");
    let dst = dir.join("out.grf");
    std::fs::write(&dst, b"precious").unwrap();
    let o = opts(Method::Zlib { level: 9 }, Format::V200);
    let err = repack(&src, &dst, &o, &Control::default()).unwrap_err();
    assert!(matches!(err, Error::TargetExists(_)));
    assert_eq!(std::fs::read(&dst).unwrap(), b"precious");

    // too small a limit: error, the existing file stays, no temp file remains
    let limited = RepackOptions {
        overwrite: true,
        max_file_len: Some(200),
        ..o.clone()
    };
    let err = repack(&src, &dst, &limited, &Control::default()).unwrap_err();
    assert!(matches!(err, Error::TooLarge(_)), "{err:?}");
    assert_eq!(std::fs::read(&dst).unwrap(), b"precious");
    let names: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1, "{names:?}");

    let over = RepackOptions {
        overwrite: true,
        ..o
    };
    repack(&src, &dst, &over, &Control::default()).unwrap();
    assert!(Archive::open(&dst).is_ok());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn dry_run_writes_nothing_and_reports_the_plan() {
    let src = dedupe_source();
    let dir = tmpdir("dry");
    let dst = dir.join("out.grf");
    let o = RepackOptions {
        dry_run: true,
        dedupe: Dedupe::Content,
        ..opts(Method::Lzma { preset: 9 }, Format::V300)
    };
    let r = repack(&src, &dst, &o, &Control::default()).unwrap();
    assert!(r.dry_run && !dst.exists());
    assert_eq!(
        (r.entries, r.markers, r.blocks, r.shared_files),
        (5, 1, 2, 2)
    );
    assert!(std::fs::read_dir(&dir).unwrap().next().is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_key_block_without_protected_entries_is_dropped() {
    let src = Archive::from_source(build_with(
        &[file("data\\a", &text(100, 1))],
        Layout::V300 { key_block: 32 },
    ))
    .unwrap();
    let dir = tmpdir("keyblock");
    let r = repack(
        &src,
        &dir.join("o.grf"),
        &opts(Method::Zlib { level: 9 }, Format::V300),
        &Control::default(),
    )
    .unwrap();
    assert!(r.verified);
    let a = Archive::open(dir.join("o.grf")).unwrap();
    assert_eq!(
        (a.table_info().key_block, a.read(0).unwrap().data),
        (0, text(100, 1))
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_output_check_detects_a_damaged_result() {
    let src = mixed_source();
    let o = opts(Method::Zlib { level: 9 }, Format::V200);
    let sources = [&src];
    let sel = Selection {
        sources: &sources,
        sel: (0..src.len()).map(|idx| Sel { src: 0, idx }).collect(),
        names: Vec::new(),
    };
    let units = plan_units(&sel, &o, &Control::default()).unwrap();
    let mut unit_of = vec![None; src.len()];
    for (u, unit) in units.iter().enumerate() {
        for &m in &unit.members {
            unit_of[m] = Some(u);
        }
    }
    let expected: Vec<_> = units
        .iter()
        .enumerate()
        .map(|(u, unit)| prepare(&sel, u, unit, &o, false).unwrap().expected)
        .collect();
    let (_, out, dir) = run(&src, "tamper", &o);
    let path = dir.join("out.grf");
    verify_output(
        &sel,
        &units,
        &unit_of,
        &expected,
        &path,
        (None, None),
        &Control::default(),
    )
    .unwrap();

    // flip one byte inside the first compressed stream (the zlib checksum or data)
    let mut bytes = std::fs::read(&path).unwrap();
    let at = (46 + out.entry(0).unwrap().offset) as usize + 8;
    bytes[at] ^= 0xFF;
    std::fs::write(&path, bytes).unwrap();
    let err = verify_output(
        &sel,
        &units,
        &unit_of,
        &expected,
        &path,
        (None, None),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::VerifyFailed(_)), "{err:?}");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn lzma_output_uses_the_measured_stream_layout() {
    let src = open(&[file("data\\a.txt", &text(30_000, 1))]);
    let o = RepackOptions {
        recompress_all: true,
        ..opts(Method::Lzma { preset: 6 }, Format::V200)
    };
    let (_, out, dir) = run(&src, "layout", &o);
    let raw = out.read_raw(0).unwrap();
    assert_eq!(sniff(&raw), Some(Codec::Lzma));
    assert_eq!(&raw[..6], &[0x00, 0x5d, 0x00, 0x00, 0x30, 0x00]);
    std::fs::remove_dir_all(dir).unwrap();
}
