//! Tests on small archives built in memory (cases real archives do not contain).

use crate::archive::Archive;

mod build;
mod crypt;
mod legacy;
mod merge;
mod raw;
mod repack;
mod v300;
use crate::codec::{DecodeStatus, tests_support};
use crate::container::flags;
use crate::error::Error;
use crate::extract::{ExtractOptions, OnCollision, extract, safe_relative_path};
use crate::name::{GrfPath, NameMode};
use crate::progress::Control;
use crate::verify::{IssueKind, Severity, VerifyOptions, verify};

#[derive(Clone)]
enum Stream {
    Zlib(Vec<u8>),
    Lzma(Vec<u8>),
    /// Stored as given (damaged or hand-made streams).
    Literal(Vec<u8>),
}

#[derive(Clone)]
struct T {
    name: Vec<u8>,
    size: u32,
    stream: Stream,
    flags: u8,
    share_with: Option<usize>,
    offset: Option<u64>,
    aligned: Option<u32>,
}

fn file(name: &str, data: &[u8]) -> T {
    T {
        name: name.as_bytes().to_vec(),
        size: data.len() as u32,
        stream: Stream::Zlib(data.to_vec()),
        flags: flags::FILE,
        share_with: None,
        offset: None,
        aligned: None,
    }
}

fn marker(name: &str) -> T {
    T {
        stream: Stream::Literal(Vec::new()),
        flags: 0,
        size: 0,
        ..file(name, b"")
    }
}

fn build(entries: &[T]) -> Vec<u8> {
    build_with(entries, Layout::V200)
}

#[derive(Clone, Copy)]
enum Layout {
    V200,
    /// 0x300: 64-bit offsets, 12-byte table prefix, optional key block behind the table.
    V300 {
        key_block: usize,
    },
}

fn build_with(entries: &[T], layout: Layout) -> Vec<u8> {
    let (header, data, tail) = build_parts(entries, layout);
    [header, data, tail].concat()
}

/// Header (46 bytes), data area, and everything after it (table block and key block).
fn build_parts(entries: &[T], layout: Layout) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let v300 = matches!(layout, Layout::V300 { .. });
    let mut data = Vec::new();
    let mut table = Vec::new();
    let mut placed: Vec<(u64, u32, u32)> = Vec::new(); // offset, zsize, aligned
    for t in entries {
        let (offset, zsize, aligned) = if let Some(j) = t.share_with {
            placed[j]
        } else {
            let stream = match &t.stream {
                Stream::Zlib(d) => tests_support::zlib(d),
                Stream::Lzma(d) => tests_support::lzma_stream(d),
                Stream::Literal(b) => b.clone(),
            };
            let offset = data.len() as u64;
            let aligned = (stream.len() as u32).div_ceil(8) * 8;
            data.extend_from_slice(&stream);
            data.resize(data.len() + (aligned as usize - stream.len()), 0);
            (offset, stream.len() as u32, aligned)
        };
        placed.push((offset, zsize, aligned));
        table.extend_from_slice(&t.name);
        table.push(0);
        table.extend_from_slice(&zsize.to_le_bytes());
        table.extend_from_slice(&t.aligned.unwrap_or(aligned).to_le_bytes());
        table.extend_from_slice(&t.size.to_le_bytes());
        table.push(t.flags);
        let off = t.offset.unwrap_or(offset);
        if v300 {
            table.extend_from_slice(&off.to_le_bytes());
        } else {
            table.extend_from_slice(&(off as u32).to_le_bytes());
        }
    }
    let packed = tests_support::zlib(&table);
    let mut out = Vec::new();
    if v300 {
        out.extend_from_slice(b"Event Horizon\0c\0");
    } else {
        out.extend_from_slice(b"Master of Magic\0");
    }
    out.extend((1..=14u8).collect::<Vec<_>>());
    if v300 {
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32).to_le_bytes());
        out.extend_from_slice(&0x300u32.to_le_bytes());
    } else {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(entries.len() as u32 + 7).to_le_bytes());
        out.extend_from_slice(&0x200u32.to_le_bytes());
    }
    let header = std::mem::take(&mut out);
    if let Layout::V300 { key_block } = layout {
        out.extend_from_slice(&(key_block as u32).to_le_bytes());
    }
    out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    out.extend_from_slice(&(table.len() as u32).to_le_bytes());
    out.extend_from_slice(&packed);
    if let Layout::V300 { key_block } = layout {
        out.extend(std::iter::repeat_n(0xAB, key_block));
    }
    (header, data, out)
}

fn open(entries: &[T]) -> Archive<Vec<u8>> {
    Archive::from_source(build(entries)).unwrap()
}

fn kinds(rep: &crate::verify::VerifyReport, s: Severity) -> Vec<IssueKind> {
    rep.issues
        .iter()
        .filter(|i| i.severity == s)
        .map(|i| i.kind)
        .collect()
}

fn tmpdir(tag: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("rgrf-test-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn sample() -> Vec<u8> {
    (0..5000u32).map(|i| (i % 251) as u8).collect()
}

// ---- opening and reading ------------------------------------------------------------------

#[test]
fn reads_zlib_lzma_empty_and_marker_entries() {
    let s = sample();
    let mut lz = file("data\\b.act", &s);
    lz.stream = Stream::Lzma(s.clone());
    let g = open(&[
        file("data\\a.spr", &s),
        lz,
        file("data\\empty.txt", b""),
        marker("data\\dir.txt"),
    ]);
    assert_eq!(g.len(), 4);
    for i in 0..2 {
        let d = g.read(i).unwrap();
        assert_eq!(
            (d.status, d.data),
            (DecodeStatus::Ok, s.clone()),
            "entry {i}"
        );
    }
    assert_eq!(g.read(2).unwrap().data, b"");
    assert!(g.entry(3).unwrap().is_marker());
    assert_eq!(g.read(3).unwrap().status, DecodeStatus::Ok);
    assert!(matches!(g.read(4), Err(Error::NoSuchEntry(4))));
}

#[test]
fn names_are_bytes_and_lookup_ignores_case_and_separator() {
    let mut e = file("x", b"1");
    e.name = vec![
        b'd', b'a', b't', b'a', b'\\', 0xC0, 0xAF, 0x83, b'.', b'S', b'P', b'R',
    ];
    let g = open(&[e.clone()]);
    assert_eq!(g.entries()[0].name.as_bytes(), &e.name[..]);
    let probe = GrfPath::new(vec![
        b'D', b'A', b'T', b'A', b'/', 0xC0, 0xAF, 0x83, b'.', b's', b'p', b'r',
    ]);
    assert_eq!(g.find(&probe), Some(0));
    assert_eq!(g.find(&GrfPath::new(b"data\\none".to_vec())), None);
}

#[test]
fn rejects_non_grf_unsupported_and_damaged_containers() {
    let good = build(&[file("data\\a", b"abc")]);
    assert!(matches!(
        Archive::from_source(vec![0u8; 10]),
        Err(Error::NotGrf(_))
    ));
    // another magic with a GRF version opens, as in GRF Editor; with no GRF version it is no GRF
    let mut other_magic = good.clone();
    other_magic[0] = b'X';
    assert_eq!(Archive::from_source(other_magic).unwrap().len(), 1);
    let mut bad_magic = good.clone();
    bad_magic[0] = b'X';
    bad_magic[42..46].copy_from_slice(&0x400u32.to_le_bytes());
    assert!(matches!(
        Archive::from_source(bad_magic),
        Err(Error::NotGrf(_))
    ));
    let mut v400 = good.clone();
    v400[42..46].copy_from_slice(&0x400u32.to_le_bytes());
    assert!(matches!(
        Archive::from_source(v400),
        Err(Error::UnsupportedVersion(0x400))
    ));
    assert!(matches!(
        Archive::from_source(good[..good.len() - 4].to_vec()),
        Err(Error::CorruptTable(_))
    ));
    let mut wrong_count = good.clone();
    wrong_count[38..42].copy_from_slice(&(9u32 + 7).to_le_bytes());
    assert!(matches!(
        Archive::from_source(wrong_count),
        Err(Error::CorruptTable(_))
    ));
    let mut few = good.clone();
    few[38..42].copy_from_slice(&3u32.to_le_bytes());
    assert!(matches!(
        Archive::from_source(few),
        Err(Error::CorruptTable(_))
    ));
    let mut off = good;
    off[30..34].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Archive::from_source(off),
        Err(Error::CorruptTable(_))
    ));
}

#[test]
fn empty_archive_opens() {
    let g = open(&[]);
    assert!(g.is_empty());
    assert_eq!(
        verify(&g, VerifyOptions::default(), &Control::default())
            .unwrap()
            .exit_code(),
        0
    );
}

#[test]
fn entry_beyond_the_file_is_an_error_not_a_panic() {
    let mut e = file("data\\a", b"abc");
    e.offset = Some(1 << 30);
    let g = open(&[e]);
    assert!(matches!(g.read(0), Err(Error::EntryOutOfRange { .. })));
}

#[test]
fn select_uses_glob_or_substring() {
    let g = open(&[
        file("data\\sprite\\a.spr", b"1"),
        file("data\\sprite\\b.act", b"2"),
        file("data\\Job4.spr", b"3"),
    ]);
    assert_eq!(g.select(None, NameMode::Cp1252), vec![0, 1, 2]);
    assert_eq!(g.select(Some("*.spr"), NameMode::Cp1252), vec![0, 2]);
    assert_eq!(g.select(Some("job4"), NameMode::Cp1252), vec![2]);
    assert_eq!(g.select(Some("data/sprite/"), NameMode::Cp1252), vec![0, 1]);
}

// ---- verify -------------------------------------------------------------------------------

fn run_verify(entries: &[T]) -> crate::verify::VerifyReport {
    verify(
        &open(entries),
        VerifyOptions::default(),
        &Control::default(),
    )
    .unwrap()
}

#[test]
fn clean_archive_has_only_infos() {
    let rep = run_verify(&[file("data\\a", &sample()), marker("data\\dir.txt")]);
    assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
    assert_eq!(kinds(&rep, Severity::Info), vec![IssueKind::Markers]);
    assert_eq!(
        (rep.entries, rep.files, rep.markers, rep.decoded),
        (2, 1, 1, 2)
    );
}

#[test]
fn truncated_stream_is_a_warning_and_does_not_stop_verify() {
    let data: Vec<u8> = (0..2000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let stored = tests_support::zlib_level(&data, 0);
    let mut damaged = file("data\\wav\\worm_tail_damage.wav", &data);
    damaged.stream = Stream::Literal(stored[..stored.len() - 11].to_vec());
    let rep = run_verify(&[file("data\\ok", &sample()), damaged]);
    assert_eq!(rep.exit_code(), 2);
    assert_eq!(kinds(&rep, Severity::Warning), vec![IssueKind::Truncated]);
}

#[test]
fn corrupt_stream_is_an_error() {
    let mut e = file("data\\x", &sample());
    e.stream = Stream::Literal(vec![0x78, 0x9c, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]);
    let rep = run_verify(&[e]);
    assert_eq!(
        (rep.exit_code(), kinds(&rep, Severity::Error)),
        (3, vec![IssueKind::Corrupt])
    );
}

#[test]
fn out_of_range_overlap_duplicates_alignment_encrypted_and_sharing() {
    let s = sample();
    let a = file("data\\a", &s);
    let mut far = file("data\\far", &s);
    far.offset = Some(1 << 30);
    let mut shared = file("data\\shared", &s);
    shared.share_with = Some(0);
    let mut enc = file("data\\enc", &s);
    enc.flags = flags::FILE | flags::GRF_EDITOR_CRYPT | flags::GRAVITY_CRYPT;
    let mut misaligned = file("data\\mis", &s);
    misaligned.aligned = Some(1);
    let dup = file("DATA/A", &s);
    let rep = run_verify(&[a, far, shared, enc, misaligned, dup]);
    let errors = kinds(&rep, Severity::Error);
    assert_eq!(errors, vec![IssueKind::OutOfRange]);
    let warnings = kinds(&rep, Severity::Warning);
    for k in [
        IssueKind::Alignment,
        IssueKind::DuplicatePath,
        IssueKind::Encrypted,
    ] {
        assert!(warnings.contains(&k), "{k:?} missing in {warnings:?}");
    }
    assert!(kinds(&rep, Severity::Info).contains(&IssueKind::SharedData));
    assert_eq!(rep.exit_code(), 3);
}

#[test]
fn alignment_rounded_up_to_4_is_accepted_except_for_des_entries() {
    let s = sample();
    let mut stream = tests_support::zlib(&s);
    // trailing bytes after the stream are ignored; make zsize % 8 == 2 so that 4 and 8 differ
    stream.resize(stream.len() + (10 - stream.len() % 8) % 8, 0);
    let zsize = stream.len() as u32;
    assert_eq!(zsize % 8, 2);
    let four = |name: &str, flags: u8| T {
        stream: Stream::Literal(stream.clone()),
        aligned: Some(zsize.div_ceil(4) * 4),
        flags,
        ..file(name, &s)
    };
    let rep = run_verify(&[four("data\\a", flags::FILE)]);
    assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
    let rep = run_verify(&[four("data\\des", flags::FILE | flags::DATA_CRYPT)]);
    assert!(
        kinds(&rep, Severity::Warning).contains(&IssueKind::Alignment),
        "{:?}",
        rep.issues
    );
}

#[test]
fn partial_overlap_is_an_error() {
    let s = sample();
    let a = file("data\\a", &s);
    let mut b = file("data\\b", &s);
    b.offset = Some(4); // starts inside entry a
    let rep = run_verify(&[a, b]);
    assert!(
        kinds(&rep, Severity::Error).contains(&IssueKind::Overlap),
        "{:?}",
        rep.issues
    );
}

/// A folder entry as libgrf writes it (measured in Thoth.grf and nodelay.grf): flags 0x02, the
/// fixed sizes of `GRFFILE_DIR_*` and an offset inside another entry's data.
fn libgrf_folder(name: &str) -> T {
    T {
        stream: Stream::Literal(vec![0x5a; 0x449]),
        flags: flags::HEADER_CRYPT,
        size: 0x55C,
        offset: Some(0x55C),
        aligned: Some(0x714),
        ..file(name, b"")
    }
}

#[test]
fn libgrf_folder_entries_are_markers_not_damaged_files() {
    let big: Vec<u8> = (0..20_000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let entries = [
        file("data\\sprite\\a.spr", &big),
        libgrf_folder("data"),
        libgrf_folder("data\\sprite"),
    ];
    let g = open(&entries);
    let e = g.entry(1).unwrap();
    assert!(e.is_folder() && e.is_marker() && !e.is_des() && !e.stores_uncompressed());
    assert_eq!(g.read(1).unwrap().status, DecodeStatus::Ok);
    let rep = run_verify(&entries);
    assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
    assert_eq!((rep.files, rep.markers), (1, 2));
    // extracting used to stop: the entry `data` collided with the folder `data`
    let dir = tmpdir("libgrf-folder");
    let r = extract(
        &g,
        &all(&g),
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!((r.written, r.skipped_markers), (1, 2));
    assert_eq!(std::fs::read(dir.join("data/sprite/a.spr")).unwrap(), big);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn verify_can_be_cancelled() {
    let g = open(&[file("data\\a", &sample())]);
    let token = crate::progress::CancelToken::new();
    token.cancel();
    let ctl = Control {
        cancel: Some(&token),
        progress: None,
    };
    assert!(matches!(
        verify(&g, VerifyOptions::default(), &ctl),
        Err(Error::Cancelled)
    ));
}

// ---- extract ------------------------------------------------------------------------------

fn all(g: &Archive<Vec<u8>>) -> Vec<usize> {
    (0..g.len()).collect()
}

#[test]
fn extracts_files_skips_markers_and_keeps_names() {
    let s = sample();
    let mut korean = file("x", &s);
    korean.name = b"data\\sprite\\\xc0\xaf\xc0\xfa.spr".to_vec();
    let g = open(&[
        file("data\\a\\b.txt", b"hello"),
        korean,
        marker("data\\dir.txt"),
    ]);
    let dir = tmpdir("basic");
    let r = extract(
        &g,
        &all(&g),
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!((r.written, r.skipped_markers, r.damaged.len()), (2, 1, 0));
    assert_eq!(std::fs::read(dir.join("data/a/b.txt")).unwrap(), b"hello");
    assert_eq!(
        std::fs::read(dir.join("data/sprite/\u{c0}\u{af}\u{c0}\u{fa}.spr")).unwrap(),
        s
    );
    let r2 = extract(
        &g,
        &[1],
        &dir.join("kr"),
        &ExtractOptions {
            names: NameMode::Cp949,
            ..ExtractOptions::default()
        },
        &Control::default(),
    )
    .unwrap();
    assert_eq!(r2.written, 1);
    assert!(dir.join("kr/data/sprite/유저.spr").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unsafe_paths_are_refused_before_anything_is_written() {
    for bad in [
        "data\\..\\..\\evil",
        "\\abs\\x",
        "data\\\\x",
        "data\\.\\x",
        "..\\x",
        "data\\a\\",
    ] {
        let mut e = file("x", b"1");
        e.name = bad.as_bytes().to_vec();
        assert!(
            matches!(
                safe_relative_path(&GrfPath::new(e.name.clone()), NameMode::Cp1252),
                Err(Error::UnsafePath(_))
            ),
            "{bad}"
        );
    }
    let g = open(&[file("data\\ok", b"1"), file("data\\..\\..\\evil", b"2")]);
    let dir = tmpdir("unsafe");
    let err = extract(
        &g,
        &all(&g),
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::UnsafePath(_)));
    assert!(!dir.join("data").exists(), "nothing may be written");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn existing_files_are_not_overwritten_by_default() {
    let g = open(&[file("data\\a.txt", b"new")]);
    let dir = tmpdir("collide");
    std::fs::create_dir_all(dir.join("data")).unwrap();
    std::fs::write(dir.join("data/a.txt"), b"old").unwrap();
    let ctl = Control::default();

    let err = extract(&g, &[0], &dir, &ExtractOptions::default(), &ctl).unwrap_err();
    assert!(matches!(err, Error::TargetExists(_)));
    assert_eq!(std::fs::read(dir.join("data/a.txt")).unwrap(), b"old");

    let skip = ExtractOptions {
        on_collision: OnCollision::Skip,
        ..ExtractOptions::default()
    };
    let r = extract(&g, &[0], &dir, &skip, &ctl).unwrap();
    assert_eq!((r.written, r.skipped_collisions), (0, 1));

    let rename = ExtractOptions {
        on_collision: OnCollision::Rename,
        ..ExtractOptions::default()
    };
    let r = extract(&g, &[0], &dir, &rename, &ctl).unwrap();
    assert_eq!((r.written, r.renamed), (1, 1));
    assert_eq!(std::fs::read(dir.join("data/a~1.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(dir.join("data/a.txt")).unwrap(), b"old");

    let over = ExtractOptions {
        overwrite: true,
        ..ExtractOptions::default()
    };
    extract(&g, &[0], &dir, &over, &ctl).unwrap();
    assert_eq!(std::fs::read(dir.join("data/a.txt")).unwrap(), b"new");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn same_path_twice_in_one_archive_collides_instead_of_overwriting() {
    let g = open(&[
        file("data\\a.txt", b"first"),
        file("data\\a.txt", b"second"),
    ]);
    let dir = tmpdir("dup");
    let err = extract(
        &g,
        &all(&g),
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::TargetExists(_)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn damaged_entries_are_written_with_recovered_data_and_reported() {
    let data: Vec<u8> = (0..2000u32)
        .map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    let stored = tests_support::zlib_level(&data, 0);
    let mut damaged = file("data\\wav\\worm.wav", &data);
    damaged.stream = Stream::Literal(stored[..stored.len() - 11].to_vec());
    let mut enc = file("data\\enc.lub", &data);
    enc.flags = flags::FILE | flags::GRF_EDITOR_CRYPT;
    let g = open(&[damaged, enc]);
    let dir = tmpdir("damaged");
    let r = extract(
        &g,
        &all(&g),
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!((r.written, r.skipped_encrypted, r.damaged.len()), (1, 1, 1));
    let written = std::fs::read(dir.join("data/wav/worm.wav")).unwrap();
    assert!(!written.is_empty() && written.len() < data.len());
    assert_eq!(written, data[..written.len()]);
    assert!(!dir.join("data/enc.lub").exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn directory_spelling_follows_archive_order_not_thread_timing() {
    // On a case-insensitive file system (macOS, Windows) the first spelling wins; on a
    // case-sensitive one both exist. In both cases the first one must be present.
    let entries: Vec<T> = (0..64)
        .map(|i| {
            file(
                &format!("data\\{}\\f{i}.txt", if i == 0 { "Foo" } else { "foo" }),
                b"x",
            )
        })
        .collect();
    let g = open(&entries);
    for round in 0..5 {
        let dir = tmpdir(&format!("case{round}"));
        extract(
            &g,
            &all(&g),
            &dir,
            &ExtractOptions::default(),
            &Control::default(),
        )
        .unwrap();
        let names: Vec<String> = std::fs::read_dir(dir.join("data"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"Foo".to_owned()), "{names:?}");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn duplicate_targets_are_written_in_archive_order_not_thread_timing() {
    let mut entries: Vec<T> = (0..40)
        .map(|i| file(&format!("data\\other{i}.txt"), b"o"))
        .collect();
    entries.insert(5, file("data\\dup.txt", b"first"));
    entries.insert(30, file("DATA\\Dup.TXT", b"second"));
    let g = open(&entries);
    let ctl = Control::default();
    let dup = |dir: &std::path::Path| {
        let mut found = Vec::new();
        for e in std::fs::read_dir(dir.join("data")).unwrap() {
            let name = e.unwrap().file_name().to_string_lossy().to_lowercase();
            if name.starts_with("dup") {
                found.push(name);
            }
        }
        found.sort();
        found
    };
    for round in 0..4 {
        let dir = tmpdir(&format!("dupr{round}"));
        let rename = ExtractOptions {
            on_collision: OnCollision::Rename,
            ..ExtractOptions::default()
        };
        let r = extract(&g, &all(&g), &dir, &rename, &ctl).unwrap();
        assert_eq!(r.renamed, 1);
        assert_eq!(std::fs::read(dir.join("data/dup.txt")).unwrap(), b"first");
        assert_eq!(dup(&dir).len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();

        let dir = tmpdir(&format!("dupo{round}"));
        let over = ExtractOptions {
            overwrite: true,
            ..ExtractOptions::default()
        };
        extract(&g, &all(&g), &dir, &over, &ctl).unwrap();
        assert_eq!(
            std::fs::read(dir.join("data/dup.txt")).unwrap(),
            b"second",
            "last wins"
        );
        std::fs::remove_dir_all(&dir).unwrap();

        let dir = tmpdir(&format!("dups{round}"));
        let skip = ExtractOptions {
            on_collision: OnCollision::Skip,
            ..ExtractOptions::default()
        };
        let r = extract(&g, &all(&g), &dir, &skip, &ctl).unwrap();
        assert_eq!(r.skipped_collisions, 1);
        assert_eq!(std::fs::read(dir.join("data/dup.txt")).unwrap(), b"first");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[test]
fn names_that_differ_only_in_case_end_up_in_one_tree_on_every_file_system() {
    let g = open(&[
        file("data\\Sprite\\a.spr", b"1"),
        file("DATA\\sprite\\b.spr", b"2"),
        file("Data\\SPRITE\\A.SPR", b"3"),
    ]);
    let dir = tmpdir("onetree");
    let opts = ExtractOptions {
        on_collision: OnCollision::Rename,
        ..ExtractOptions::default()
    };
    let r = extract(&g, &all(&g), &dir, &opts, &Control::default()).unwrap();
    assert_eq!((r.written, r.renamed), (3, 1));
    let names = |p: &std::path::Path| -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(p)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    assert_eq!(names(&dir), ["data"], "the first spelling wins");
    assert_eq!(names(&dir.join("data")), ["Sprite"]);
    assert_eq!(
        names(&dir.join("data/Sprite")),
        ["a.spr", "a~1.spr", "b.spr"]
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn windows_file_name_rules() {
    use crate::extract::invalid_on_windows as bad;
    for n in [
        "a?b",
        "a*",
        "x:y",
        "q\"r",
        "a|b",
        "<a>",
        "name.",
        "name ",
        "nul",
        "NUL.txt",
        "Con",
        "aux.tar.gz",
        "com1",
        "LPT9.log",
        "a\u{1}b",
    ] {
        assert!(bad(n), "{n} must be refused");
    }
    for n in [
        "data",
        "nulls.txt",
        "console",
        "com0",
        "com10",
        "lpt",
        "a.b",
        ".hidden",
        "한글",
        "É",
        "ÉÉ",
        "éé1",
        "abé",
        "co\u{e9}1",
    ] {
        assert!(!bad(n), "{n} is fine");
    }
}

#[test]
fn entries_claiming_huge_sizes_are_refused_without_decoding() {
    let mut e = file("data\\bomb.bin", b"tiny");
    e.size = u32::MAX; // table claims 4 GiB
    let mut g = open(&[e, file("data\\ok.bin", b"fine")]);
    let d = g.read(0).unwrap();
    assert!(
        matches!(d.status, DecodeStatus::Corrupt(ref m) if m.contains("exceeds the limit")),
        "{:?}",
        d.status
    );
    assert!(d.data.is_empty());
    assert_eq!(g.read(1).unwrap().data, b"fine");
    g.set_max_entry_size(2);
    assert!(matches!(
        g.read(1).unwrap().status,
        DecodeStatus::Corrupt(_)
    ));
    let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
    assert_eq!(rep.exit_code(), 3);
}

#[cfg(unix)]
#[test]
fn symlink_leading_out_of_the_destination_is_refused() {
    let g = open(&[file("data\\a.txt", b"x")]);
    let dir = tmpdir("symlink");
    let outside = tmpdir("symlink-outside");
    std::os::unix::fs::symlink(&outside, dir.join("data")).unwrap();
    let err = extract(
        &g,
        &[0],
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::UnsafePath(_)), "{err:?}");
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}

#[cfg(unix)]
#[test]
fn symlinks_never_let_directories_or_files_appear_outside() {
    let g = open(&[file("data\\newdir\\deeper\\a.txt", b"x")]);
    let dir = tmpdir("symlink-deep");
    let outside = tmpdir("symlink-deep-outside");
    std::os::unix::fs::symlink(&outside, dir.join("data")).unwrap();
    let err = extract(
        &g,
        &[0],
        &dir,
        &ExtractOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::UnsafePath(_)), "{err:?}");
    assert!(
        std::fs::read_dir(&outside).unwrap().next().is_none(),
        "no directory may be created outside of the destination"
    );
    std::fs::remove_dir_all(&dir).unwrap();

    // a symlink sitting at the final path must not be written through, even with --overwrite
    let g = open(&[file("data\\a.txt", b"payload")]);
    let dir = tmpdir("symlink-final");
    std::fs::create_dir_all(dir.join("data")).unwrap();
    let victim = outside.join("victim.txt");
    std::fs::write(&victim, b"keep").unwrap();
    std::os::unix::fs::symlink(&victim, dir.join("data/a.txt")).unwrap();
    let over = ExtractOptions {
        overwrite: true,
        ..ExtractOptions::default()
    };
    let err = extract(&g, &[0], &dir, &over, &Control::default()).unwrap_err();
    assert!(matches!(err, Error::UnsafePath(_)), "{err:?}");
    assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
    std::fs::remove_dir_all(dir).unwrap();
    std::fs::remove_dir_all(outside).unwrap();
}
