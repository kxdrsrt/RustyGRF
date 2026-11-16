//! Entries with flags 0 and data are files stored uncompressed (measured in the 0x300 archives:
//! 30 and 25 such PNGs). They used to be mistaken for empty markers and lost by `repack`.

use super::*;
use crate::encode::Method;
use crate::repack::{Dedupe, RepackOptions, repack};
use crate::writer::Format;

fn png(n: usize) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend((0..n).map(|i| (i * 31 % 251) as u8));
    v
}

/// A flags-0 entry whose stream is the file content itself.
fn raw_file(name: &str, data: &[u8]) -> T {
    T {
        stream: Stream::Literal(data.to_vec()),
        flags: 0,
        ..file(name, data)
    }
}

fn source(layout: Layout) -> Archive<Vec<u8>> {
    let data = png(3000);
    Archive::from_source(build_with(
        &[
            file("data\\a.txt", b"text text text text"),
            raw_file("data\\illust\\a.png", &data),
            marker("data\\empty_dir.txt"),
            // same content as the raw entry, but stored compressed: must never be merged with it
            file("data\\illust\\copy.png", &data),
        ],
        layout,
    ))
    .unwrap()
}

#[test]
fn are_classified_read_verified_and_extracted_as_files() {
    for layout in [Layout::V200, Layout::V300 { key_block: 0 }] {
        let g = source(layout);
        let (raw, marker) = (g.entry(1).unwrap(), g.entry(2).unwrap());
        assert!(raw.is_raw() && !raw.is_marker() && !raw.is_file());
        assert!(marker.is_marker() && !marker.is_raw());
        assert_eq!(g.read(1).unwrap().data, png(3000));
        assert_eq!(g.sniff_codec(1).unwrap(), None, "no codec for stored data");

        let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
        assert_eq!(
            (rep.exit_code(), rep.markers, rep.raw, rep.files),
            (0, 1, 1, 3),
            "{:?}",
            rep.issues
        );
        assert!(kinds(&rep, Severity::Info).contains(&IssueKind::RawEntries));

        let dir = tmpdir("rawx");
        let all: Vec<usize> = (0..g.len()).collect();
        let r = extract(
            &g,
            &all,
            &dir,
            &ExtractOptions::default(),
            &Control::default(),
        )
        .unwrap();
        assert_eq!((r.written, r.skipped_markers), (3, 1));
        assert_eq!(
            std::fs::read(dir.join("data/illust/a.png")).unwrap(),
            png(3000)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn repack_keeps_them_unchanged_for_every_method_format_and_dedupe() {
    let src = source(Layout::V300 { key_block: 0 });
    for method in [Method::Zlib { level: 9 }, Method::Lzma { preset: 5 }] {
        for format in [Format::V200, Format::V300] {
            for dedupe in [Dedupe::None, Dedupe::Source, Dedupe::Content] {
                let dir = tmpdir("rawrp");
                let dst = dir.join("o.grf");
                let o = RepackOptions {
                    method,
                    format: Some(format),
                    dedupe,
                    ..RepackOptions::default()
                };
                let r = repack(&src, &dst, &o, &Control::default()).unwrap();
                let out = Archive::open(&dst).unwrap();
                assert!(r.verified);
                assert_eq!(
                    (r.stored_blocks, r.markers),
                    (1, 1),
                    "{method:?} {format:?} {dedupe:?}"
                );
                let (a, b) = (src.entry(1).unwrap(), out.entry(1).unwrap());
                assert_eq!(
                    (a.flags, a.zsize, a.size),
                    (b.flags, b.zsize, b.size),
                    "unchanged entry"
                );
                assert_eq!(src.read_raw(1).unwrap(), out.read_raw(1).unwrap());
                assert_eq!(out.read(1).unwrap().data, png(3000));
                // the compressed copy with identical content stays a compressed entry
                assert!(out.entry(3).unwrap().is_file(), "{dedupe:?}");
                assert_ne!(out.entry(1).unwrap().offset, out.entry(3).unwrap().offset);
                assert_eq!(out.read(3).unwrap().data, png(3000));
                std::fs::remove_dir_all(dir).unwrap();
            }
        }
    }
}

#[test]
fn a_lost_raw_entry_would_fail_the_output_check() {
    // the check compares entry kinds, so the old bug (raw entry written as an empty marker)
    // cannot pass unnoticed any more
    let src = source(Layout::V200);
    let dir = tmpdir("rawbug");
    let dst = dir.join("o.grf");
    repack(&src, &dst, &RepackOptions::default(), &Control::default()).unwrap();
    let out = Archive::open(&dst).unwrap();
    assert!(src.entry(1).unwrap().is_raw() && out.entry(1).unwrap().is_raw());
    assert_eq!(out.entry(1).unwrap().size, src.entry(1).unwrap().size);
    std::fs::remove_dir_all(dir).unwrap();
}
