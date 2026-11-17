//! Tests of `diff`, `merge` and `patch` on small archives built in memory.

use super::*;
use crate::merge::{diff, merge, patch};
use crate::name::GrfPath;
use crate::repack::{Dedupe, RepackOptions};

fn data(tag: u8, n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| b'a' + ((i + usize::from(tag)) % 23) as u8)
        .collect()
}

fn content<R: crate::io::ReadAt>(a: &Archive<R>, name: &str) -> Vec<u8> {
    let i = a
        .find(&GrfPath::new(name.as_bytes().to_vec()))
        .unwrap_or_else(|| panic!("{name} missing"));
    a.read(i).unwrap().data
}

fn names<R: crate::io::ReadAt>(a: &Archive<R>) -> Vec<String> {
    a.entries()
        .iter()
        .map(|e| String::from_utf8_lossy(e.name.as_bytes()).into_owned())
        .collect()
}

fn out(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = tmpdir(tag);
    let dst = dir.join("out.grf");
    (dir, dst)
}

#[test]
fn diff_reports_added_removed_changed_and_same_by_content() {
    let a = open(&[
        file("data\\a.txt", &data(1, 100)),
        file("data\\b.txt", &data(2, 100)),
        file("data\\c.txt", &data(3, 100)),
        file("data\\same.txt", &data(4, 100)),
    ]);
    let b = open(&[
        // same size, different content
        file("DATA/a.txt", &data(9, 100)),
        file("data\\B.TXT", &data(2, 100)),
        file("data\\d.txt", &data(5, 100)),
        file("data\\same.txt", &data(4, 100)),
    ]);
    let r = diff(&a, &b, &Control::default()).unwrap();
    let list = |v: &[GrfPath]| -> Vec<String> {
        v.iter()
            .map(|n| String::from_utf8_lossy(n.as_bytes()).into_owned())
            .collect()
    };
    assert_eq!(list(&r.only_a), ["data\\c.txt"]);
    assert_eq!(list(&r.only_b), ["data\\d.txt"]);
    assert_eq!(list(&r.changed), ["data\\a.txt"]);
    assert_eq!(r.same, 2, "b.txt differs only in case and separator");
    assert!(!r.is_identical());
    assert!(diff(&a, &a, &Control::default()).unwrap().is_identical());
}

#[test]
fn merge_first_archive_wins_and_the_lowest_priority_sets_the_order() {
    let custom = open(&[file("data\\x.txt", b"custom")]);
    let rdata = open(&[
        file("data\\y.txt", b"rdata-y"),
        file("data\\x.txt", b"rdata-x"),
        file("data\\new.txt", b"rdata-new"),
    ]);
    let base = open(&[
        file("data\\x.txt", b"base-x"),
        file("data\\y.txt", b"base-y"),
        file("data\\z.txt", b"base-z"),
        marker("data\\folder"),
    ]);
    let (dir, dst) = out("merge-prio");
    let r = merge(
        &[&custom, &rdata, &base],
        &dst,
        &RepackOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let m = Archive::open(&dst).unwrap();
    assert_eq!(content(&m, "data\\x.txt"), b"custom");
    assert_eq!(content(&m, "data\\y.txt"), b"rdata-y");
    assert_eq!(content(&m, "data\\z.txt"), b"base-z");
    assert_eq!(content(&m, "data\\new.txt"), b"rdata-new");
    assert_eq!(
        names(&m),
        ["data\\x.txt", "data\\y.txt", "data\\z.txt", "data\\new.txt"]
    );
    assert!(r.repack.verified);

    let used: Vec<_> = r
        .sources
        .iter()
        .map(|s| (s.files, s.used, s.overridden))
        .collect();
    assert_eq!(used, [(1, 1, 0), (3, 2, 1), (3, 1, 2)]);
    assert_eq!(r.sources[2].markers_skipped, 1);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn merge_is_reproducible_and_can_share_equal_content_across_archives() {
    let a = open(&[file("data\\one.txt", &data(1, 3000))]);
    let b = open(&[
        file("data\\two.txt", &data(1, 3000)),
        file("data\\three.txt", &data(2, 3000)),
    ]);
    let o = RepackOptions {
        dedupe: Dedupe::Content,
        ..RepackOptions::default()
    };
    let (dir, d1) = out("merge-dedupe");
    let r = merge(&[&a, &b], &d1, &o, &Control::default()).unwrap();
    assert_eq!(
        (r.repack.files, r.repack.blocks, r.repack.shared_files),
        (3, 2, 1)
    );
    let d2 = dir.join("second.grf");
    merge(&[&a, &b], &d2, &o, &Control::default()).unwrap();
    assert_eq!(std::fs::read(&d1).unwrap(), std::fs::read(&d2).unwrap());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn merge_inside_one_archive_the_later_entry_wins_and_a_0x300_source_selects_0x300() {
    let v300 = Archive::from_source(build_with(
        &[
            file("data\\d.txt", b"first"),
            file("data\\D.TXT", b"second"),
        ],
        Layout::V300 { key_block: 0 },
    ))
    .unwrap();
    let (dir, dst) = out("merge-v300");
    let r = merge(
        &[&v300],
        &dst,
        &RepackOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(r.repack.format, 0x300);
    assert_eq!(r.sources[0].shadowed, 1);
    let m = Archive::open(&dst).unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!(content(&m, "data\\d.txt"), b"second");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn merge_refuses_nothing_to_merge_and_an_existing_target() {
    let a = open(&[file("data\\a.txt", b"a")]);
    let none: [&Archive<Vec<u8>>; 0] = [];
    let (dir, dst) = out("merge-refuse");
    let e = merge(&none, &dst, &RepackOptions::default(), &Control::default()).unwrap_err();
    assert!(matches!(e, Error::InvalidInput(_)), "{e:?}");
    std::fs::write(&dst, b"x").unwrap();
    let e = merge(&[&a], &dst, &RepackOptions::default(), &Control::default()).unwrap_err();
    assert!(matches!(e, Error::TargetExists(_)), "{e:?}");
    assert_eq!(std::fs::read(&dst).unwrap(), b"x");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn patch_contains_only_new_and_changed_files_and_restores_the_new_state_over_the_old() {
    let old = open(&[
        file("data\\keep.txt", &data(1, 200)),
        file("data\\edit.txt", &data(2, 200)),
        file("data\\gone.txt", &data(3, 200)),
    ]);
    let new = open(&[
        file("data\\keep.txt", &data(1, 200)),
        file("data\\edit.txt", &data(7, 250)),
        file("data\\add.txt", &data(4, 200)),
    ]);
    let (dir, dst) = out("patch");
    let r = patch(
        &old,
        &new,
        &dst,
        &RepackOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!((r.added, r.changed, r.removed), (1, 1, 1));
    let p = Archive::from_source(std::fs::read(&dst).unwrap()).unwrap();
    assert_eq!(names(&p), ["data\\edit.txt", "data\\add.txt"]);

    // patch before old gives the new state, except for what the new state removed
    let merged = dir.join("merged.grf");
    merge(
        &[&p, &old],
        &merged,
        &RepackOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let m = Archive::from_source(std::fs::read(&merged).unwrap()).unwrap();
    let d = diff(&m, &new, &Control::default()).unwrap();
    assert_eq!(d.only_a.len(), 1, "only the removed file is left over");
    assert!(d.only_b.is_empty() && d.changed.is_empty());

    let same = dir.join("same.grf");
    let e = patch(
        &new,
        &new,
        &same,
        &RepackOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(e, Error::InvalidInput(_)), "{e:?}");
    assert!(!same.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn copy_streams_keeps_every_stream_byte_for_byte_across_formats() {
    let mut lz = file("data\\lz.txt", &data(3, 6000));
    lz.stream = Stream::Lzma(data(3, 6000));
    let a = open(&[file("data\\z.txt", &data(1, 6000)), lz]);
    let b = open(&[file("data\\only_b.txt", &data(2, 4000))]);
    let (dir, dst) = out("copy");
    // the method says zlib 9, but copying must ignore it and keep the LZMA stream
    let o = RepackOptions {
        copy_streams: true,
        format: Some(crate::writer::Format::V300),
        ..RepackOptions::default()
    };
    let r = merge(&[&a, &b], &dst, &o, &Control::default()).unwrap();
    assert_eq!((r.repack.recompressed, r.repack.kept_original), (0, 3));
    assert!(r.repack.verified);
    let m = Archive::open(&dst).unwrap();
    assert_eq!(m.header().version, 0x300);
    // every output stream is exactly the stream of its source entry
    for (src, name) in [
        (&a, "data\\z.txt"),
        (&a, "data\\lz.txt"),
        (&b, "data\\only_b.txt"),
    ] {
        let p = GrfPath::new(name.as_bytes().to_vec());
        let (i, j) = (src.find(&p).unwrap(), m.find(&p).unwrap());
        assert_eq!(src.read_raw(i).unwrap(), m.read_raw(j).unwrap(), "{name}");
        assert_eq!(src.read(i).unwrap().data, m.read(j).unwrap().data, "{name}");
    }
    let lz_at = m.find(&GrfPath::new(b"data\\lz.txt".to_vec())).unwrap();
    assert_eq!(
        m.sniff_codec(lz_at).unwrap(),
        Some(crate::codec::Codec::Lzma)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn copy_streams_keeps_a_damaged_entry_damaged_and_verify_reports_it() {
    let a = open(&[file("data\\a.txt", &data(1, 3000))]);
    let (dir, dst) = out("copy-tamper");
    let o = RepackOptions {
        copy_streams: true,
        ..RepackOptions::default()
    };
    merge(&[&a], &dst, &o, &Control::default()).unwrap();
    let mut bytes = std::fs::read(&dst).unwrap();
    let at = 46 + 8; // inside the first stream
    bytes[at] ^= 0xFF;
    std::fs::write(&dst, &bytes).unwrap();
    let again = dir.join("again.grf");
    let broken = Archive::from_source(bytes).unwrap();
    // a copy keeps the damage as it is; only `verify` looks for it
    merge(&[&broken], &again, &o, &Control::default()).unwrap();
    let rep = crate::verify::verify(
        &Archive::open(&again).unwrap(),
        crate::verify::VerifyOptions { decode: true },
        &Control::default(),
    )
    .unwrap();
    assert!(
        rep.count(crate::verify::Severity::Error) + rep.count(crate::verify::Severity::Warning) > 0
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn replace_only_updates_existing_paths_and_adds_nothing_new() {
    use crate::merge::merge_with;
    let base = open(&[
        file("data\\a.txt", b"base a"),
        file("data\\b.txt", b"base b"),
    ]);
    let update = open(&[
        file("DATA/A.TXT", b"new a"),
        file("data\\only_in_update.txt", b"x"),
    ]);
    let (dir, dst) = out("replace-only");
    let r = merge_with(
        &[&update, &base],
        &dst,
        &RepackOptions::default(),
        true,
        &Control::default(),
    )
    .unwrap();
    let m = Archive::open(&dst).unwrap();
    assert_eq!(
        names(&m),
        ["data\\a.txt", "data\\b.txt"],
        "order and set of the base"
    );
    assert_eq!(content(&m, "data\\a.txt"), b"new a");
    assert_eq!(content(&m, "data\\b.txt"), b"base b");
    assert_eq!(r.sources[0].ignored_new, 1);
    assert_eq!((r.sources[0].used, r.sources[1].used), (1, 1));

    // without the flag the new path is added
    let all = dir.join("all.grf");
    merge_with(
        &[&update, &base],
        &all,
        &RepackOptions::default(),
        false,
        &Control::default(),
    )
    .unwrap();
    assert_eq!(Archive::open(&all).unwrap().len(), 3);
    std::fs::remove_dir_all(dir).unwrap();
}
