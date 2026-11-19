//! GRF 0x102/0x103 (scrambled names and data, plain table) and DES entries of 0x200 archives.

use super::*;
use crate::des::{self, Cycle};
use crate::encode::Method;
use crate::repack::{RepackOptions, repack};
use crate::writer::Format;

/// One entry of a legacy archive: path, content, and the type byte (1 file, 0 directory/raw).
struct L {
    name: &'static [u8],
    data: Vec<u8>,
    kind: u8,
}

fn text(n: usize) -> Vec<u8> {
    (0..n)
        .map(|i| b"the quick brown fox jumps over "[i % 31])
        .collect()
}

/// A 0x102/0x103 archive as the old tools wrote it (layout of GRF Editor's writer).
fn build_legacy(version: u32, entries: &[L]) -> Vec<u8> {
    let mut data = Vec::new();
    let mut table = Vec::new();
    for e in entries {
        let stream = if e.kind == 1 {
            tests_support::zlib(&e.data)
        } else {
            e.data.clone()
        };
        let aligned = stream.len().div_ceil(8) * 8;
        let mut stored = stream.clone();
        stored.resize(aligned, 0);
        if e.kind == 1 || e.name.contains(&b'.') {
            let head_only = [&b".gnd"[..], b".gat", b".act", b".str"]
                .iter()
                .any(|x| e.name.len() > 4 && e.name[e.name.len() - 4..].eq_ignore_ascii_case(x));
            let cycle = if head_only {
                Cycle::Head
            } else {
                Cycle::for_size(stream.len() as u32)
            };
            des::encrypt_data(&mut stored, cycle);
        }
        let offset = data.len() as u32;
        data.extend_from_slice(&stored);
        let name = des::encode_name(e.name);
        table.extend_from_slice(&(name.len() as u32 + 6).to_le_bytes());
        table.extend_from_slice(&[0, 0]);
        table.extend_from_slice(&name);
        table.extend_from_slice(&0u32.to_le_bytes());
        table.extend_from_slice(&(stream.len() as u32 + e.data.len() as u32 + 715).to_le_bytes());
        table.extend_from_slice(&(aligned as u32 + 37579).to_le_bytes());
        table.extend_from_slice(&(e.data.len() as u32).to_le_bytes());
        table.push(e.kind);
        table.extend_from_slice(&offset.to_le_bytes());
    }
    let mut out = b"Master of Magic\0".to_vec();
    out.extend(1..=14u8);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u32 + 7).to_le_bytes());
    out.extend_from_slice(&version.to_le_bytes());
    out.extend(data);
    out.extend(table);
    out
}

fn sample_entries() -> Vec<L> {
    vec![
        L {
            name: b"data",
            data: Vec::new(),
            kind: 0,
        },
        L {
            name: b"data\\texts\\a.txt",
            data: text(5000),
            kind: 1,
        },
        L {
            name: b"data\\maps\\prt.gat",
            data: text(9000),
            kind: 1,
        },
        L {
            name: b"data\\sprite\\\xc0\xaf\xc0\xfa.spr",
            data: text(300),
            kind: 1,
        },
        L {
            name: b"data\\raw.bin",
            data: text(777),
            kind: 0,
        },
    ]
}

#[test]
fn reads_names_flags_and_data_of_0x102_and_0x103() {
    for version in [0x102, 0x103] {
        let g = Archive::from_source(build_legacy(version, &sample_entries())).unwrap();
        assert_eq!(g.header().version, version);
        assert!(g.header().is_legacy());
        // the directory entry is dropped
        assert_eq!(g.len(), 4);
        let names: Vec<_> = g
            .entries()
            .iter()
            .map(|e| e.name.display(NameMode::Cp1252))
            .collect();
        assert_eq!(names[0], "data\\texts\\a.txt");
        assert_eq!(names[2].as_bytes()[..13], *b"data\\sprite\\\xc3");
        let flags: Vec<u8> = g.entries().iter().map(|e| e.flags).collect();
        // mixed cycle: 3, head only (.gat): 5, mixed: 3, stored uncompressed (no FILE bit): 2
        assert_eq!(flags, [0x03, 0x05, 0x03, 0x02]);
        for (i, want) in [text(5000), text(9000), text(300), text(777)]
            .into_iter()
            .enumerate()
        {
            let d = g.read(i).unwrap();
            assert_eq!((d.status, d.data), (DecodeStatus::Ok, want), "entry {i}");
        }
        assert!(g.entries().iter().all(|e| e.is_des() && !e.needs_key()));
        let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
        assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
    }
}

#[test]
fn legacy_tables_with_bad_lengths_are_errors_not_panics() {
    let good = build_legacy(0x102, &sample_entries());
    let mut cuts = Vec::new();
    for cut in [1, 5, 17, 40] {
        cuts.push(good[..good.len() - cut].to_vec());
    }
    let mut huge = good.clone();
    let table_at = 46 + u32::from_le_bytes(good[30..34].try_into().unwrap()) as usize;
    huge[table_at..table_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    cuts.push(huge);
    let mut tiny = good;
    let table_at = 46 + u32::from_le_bytes(tiny[30..34].try_into().unwrap()) as usize;
    tiny[table_at..table_at + 4].copy_from_slice(&3u32.to_le_bytes());
    cuts.push(tiny);
    for bytes in cuts {
        assert!(matches!(
            Archive::from_source(bytes),
            Err(Error::CorruptTable(_))
        ));
    }
}

#[test]
fn repack_turns_a_legacy_archive_into_a_plain_0x200() {
    let src = Archive::from_source(build_legacy(0x102, &sample_entries())).unwrap();
    let dir = tmpdir("legacy-repack");
    let dst = dir.join("out.grf");
    let o = RepackOptions {
        method: Method::Zlib { level: 6 },
        format: Some(Format::V200),
        ..RepackOptions::default()
    };
    let r = repack(&src, &dst, &o, &Control::default()).unwrap();
    assert_eq!(r.encrypted_blocks, 0);
    let out = Archive::open(&dst).unwrap();
    assert_eq!(out.header().version, 0x200);
    assert_eq!(out.len(), 4);
    for i in 0..4 {
        assert_eq!(out.entry(i).unwrap().name, src.entry(i).unwrap().name);
        assert!(!out.entry(i).unwrap().is_des(), "entry {i} is plain");
        assert_eq!(out.read(i).unwrap().data, src.read(i).unwrap().data);
    }
    // the stored-uncompressed entry stays uncompressed (flags 0)
    assert!(out.entry(3).unwrap().is_raw());
    let _ = std::fs::remove_dir_all(dir);
}

/// A DES entry of a 0x200 archive: the stream (zlib plus padding) is scrambled as a whole.
fn des_entry(name: &str, data: &[u8], head_only: bool) -> T {
    let stream = tests_support::zlib(data);
    let mut stored = stream.clone();
    stored.resize(stream.len().div_ceil(8) * 8, 0);
    let cycle = if head_only {
        Cycle::Head
    } else {
        Cycle::for_size(stream.len() as u32)
    };
    des::encrypt_data(&mut stored, cycle);
    T {
        stream: Stream::Literal(stored.clone()),
        flags: flags::FILE
            | if head_only {
                flags::DATA_CRYPT
            } else {
                flags::HEADER_CRYPT
            },
        aligned: Some(stored.len() as u32),
        // the size field is that of the real stream; the padding is not part of it
        ..file(name, data)
    }
}

#[test]
fn des_entries_of_a_0x200_archive_are_readable_and_copied_unchanged() {
    let (a, b) = (text(6000), text(2500));
    let mut e1 = des_entry("data\\a.txt", &a, false);
    let mut e2 = des_entry("data\\b.gat", &b, true);
    // the table's zsize is the real stream length, not the padded one
    for e in [&mut e1, &mut e2] {
        if let Stream::Literal(s) = &e.stream {
            let real = tests_support::zlib(if e.size as usize == a.len() { &a } else { &b });
            assert!(s.len() >= real.len());
        }
    }
    let g = open(&[e1.clone(), e2.clone(), file("data\\c.txt", &a)]);
    for (i, want) in [(0, &a), (1, &b), (2, &a)] {
        let d = g.read(i).unwrap();
        // zsize here is the padded length; zlib ignores the trailing zeros
        assert_eq!(d.data, *want, "entry {i}");
    }
    assert!(g.entry(0).unwrap().is_des() && g.entry(1).unwrap().is_des());
    assert!(!g.entry(2).unwrap().is_des());
    // a key-needing entry stays unreadable and is the only one counted as encrypted
    let mut keyed = file("data\\k.txt", &a);
    keyed.flags = flags::FILE | flags::GRF_EDITOR_CRYPT;
    keyed.stream = Stream::Literal(vec![7; 16]);
    let g = open(&[e1, e2, keyed]);
    assert_eq!(g.read(2).unwrap().status, DecodeStatus::Encrypted);
    assert!(g.entry(2).unwrap().needs_key());
    assert!(g.entries().iter().filter(|e| e.needs_key()).count() == 1);
}

#[test]
fn repack_writes_0x102_and_0x103_that_read_back_identically() {
    // a 0x200 source with a stored-uncompressed entry, shared content and a head-only extension
    let (a, b) = (text(6000), text(2500));
    let mut raw = file("data\\raw.bin", &b);
    raw.stream = Stream::Literal(b.clone());
    raw.flags = 0;
    let src = open(&[
        file("data\\a.txt", &a),
        file("data\\b.gat", &b),
        file("data\\copy.txt", &a),
        raw,
        file("data\\empty.txt", b""),
    ]);
    for format in [Format::V102, Format::V103] {
        let dir = tmpdir("legacy-write");
        let dst = dir.join("out.grf");
        let o = RepackOptions {
            format: Some(format),
            ..RepackOptions::default()
        };
        let r = repack(&src, &dst, &o, &Control::default()).unwrap();
        assert!(r.verified);
        let out = Archive::open(&dst).unwrap();
        assert!(out.header().is_legacy());
        assert_eq!(out.header().version, format.version());
        assert_eq!(out.len(), src.len());
        for i in 0..src.len() {
            let e = out.entry(i).unwrap();
            assert_eq!(e.name, src.entry(i).unwrap().name);
            assert!(e.is_des(), "entry {i} is scrambled");
            assert_eq!(out.read(i).unwrap().data, src.read(i).unwrap().data, "{i}");
        }
        // head-only extensions use flags 5, the rest 3; stored-uncompressed keeps no FILE bit
        let flags: Vec<u8> = out.entries().iter().map(|e| e.flags).collect();
        assert_eq!(flags, [0x03, 0x05, 0x03, 0x02, 0x03]);
        let rep = verify(&out, VerifyOptions::default(), &Control::default()).unwrap();
        assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);

        // and back: legacy to 0x200 and to legacy again
        let back = dir.join("back.grf");
        let o = RepackOptions {
            format: Some(Format::V200),
            ..RepackOptions::default()
        };
        repack(&out, &back, &o, &Control::default()).unwrap();
        let back = Archive::open(&back).unwrap();
        for i in 0..src.len() {
            assert_eq!(back.read(i).unwrap().data, src.read(i).unwrap().data);
        }
        let again = dir.join("again.grf");
        let o = RepackOptions {
            format: Some(format),
            ..RepackOptions::default()
        };
        repack(&out, &again, &o, &Control::default()).unwrap();
        let again = Archive::open(&again).unwrap();
        assert_eq!(again.len(), src.len());
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn a_key_encrypted_entry_cannot_go_into_a_legacy_archive() {
    let a = text(3000);
    let mut keyed = file("data\\k.txt", &a);
    keyed.flags = flags::FILE | flags::GRF_EDITOR_CRYPT;
    keyed.stream = Stream::Literal(vec![7; 16]);
    let src = open(&[keyed]);
    let dir = tmpdir("legacy-key");
    let o = RepackOptions {
        format: Some(Format::V103),
        ..RepackOptions::default()
    };
    let e = repack(&src, &dir.join("o.grf"), &o, &Control::default()).unwrap_err();
    assert!(matches!(e, Error::Unsupported(_)), "{e:?}");
    assert!(!dir.join("o.grf").exists());
    let _ = std::fs::remove_dir_all(dir);
}
