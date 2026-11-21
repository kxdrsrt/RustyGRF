//! GRF Editor's encryption: hidden marker entry, RC4 over the aligned streams, no flag in the table.

use super::*;
use crate::build::{BuildOptions, build};
use crate::grfcrypt::{self, Key};
use crate::repack::{RepackOptions, repack};
use crate::writer::Format;
use std::fs;
use std::path::PathBuf;

fn text(n: usize, seed: u8) -> Vec<u8> {
    (0..n)
        .map(|i| b"abcdefghij klmnopqrstuv"[(i * 7 + usize::from(seed) + i / 13) % 23])
        .collect()
}

/// A folder with a few files, built into an encrypted archive with `key`; returns the folder.
fn encrypted(tag: &str, key: &Key) -> (PathBuf, PathBuf) {
    let dir = tmpdir(tag);
    let src = dir.join("src/data");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("a.txt"), text(4000, 1)).unwrap();
    fs::write(src.join("sub/b.spr"), text(9000, 2)).unwrap();
    fs::write(src.join("empty.txt"), b"").unwrap();
    fs::write(src.join("tiny.txt"), b"x").unwrap();
    let out = dir.join("enc.grf");
    let o = BuildOptions {
        encrypt: Some(*key),
        ..BuildOptions::default()
    };
    let r = build(&dir.join("src"), &out, &o, &Control::default()).unwrap();
    assert!(r.verified);
    (dir, out)
}

#[test]
fn an_encrypted_archive_needs_its_key_and_checks_it() {
    let key = grfcrypt::key_from_password("secret").unwrap();
    let (dir, path) = encrypted("crypt1", &key);
    let g = Archive::open(&path).unwrap();
    let info = g.encryption().unwrap();
    assert_eq!(info.marker_crc, Some(grfcrypt::key_crc(&key)));
    assert!(!info.key_set && info.encrypted_entries >= 3, "{info:?}");
    // 4 files and the hidden entry; no key: encrypted ones are reported, the others read
    assert_eq!(g.len(), 5);
    let hidden = g.find(&GrfPath::new(&b"__encryption.info"[..])).unwrap();
    assert_eq!(
        g.read(hidden).unwrap().data,
        grfcrypt::key_crc(&key).to_le_bytes()
    );
    let a = g.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert!(g.entry(a).unwrap().needs_key());
    assert_eq!(g.read(a).unwrap().status, DecodeStatus::Encrypted);

    // a wrong key is refused, the right one makes everything readable
    let mut g = g;
    let wrong = grfcrypt::key_from_password("wrong").unwrap();
    assert!(matches!(g.set_key(wrong), Err(Error::WrongKey)));
    g.set_key(key).unwrap();
    assert!(!g.entry(a).unwrap().needs_key());
    assert_eq!(g.read(a).unwrap().data, text(4000, 1));
    let b = g.find(&GrfPath::new(&b"data\\sub\\b.spr"[..])).unwrap();
    assert_eq!(g.read(b).unwrap().data, text(9000, 2));
    let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
    assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
    // opening with the key does the same in one step
    let g = Archive::open_with_key(&path, &key).unwrap();
    assert_eq!(g.read(a).unwrap().data, text(4000, 1));
    assert!(matches!(
        Archive::open_with_key(&path, &wrong),
        Err(Error::WrongKey)
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn repack_decrypts_re_encrypts_and_copies_unchanged() {
    let key = grfcrypt::key_from_password("secret").unwrap();
    let key2 = grfcrypt::key_from_password("other key").unwrap();
    let (dir, path) = encrypted("crypt2", &key);
    let ctl = Control::default();

    // without a key the encrypted bytes are copied as they are, and stay readable with the key
    let src = Archive::open(&path).unwrap();
    let copy = dir.join("copy.grf");
    repack(&src, &copy, &RepackOptions::default(), &ctl).unwrap();
    let c = Archive::open_with_key(&copy, &key).unwrap();
    assert_eq!(c.len(), 5);
    let a = c.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert_eq!(c.read(a).unwrap().data, text(4000, 1));
    // ... but no new key without the old one
    let o = RepackOptions {
        encrypt: Some(key2),
        ..RepackOptions::default()
    };
    assert!(matches!(
        repack(&src, &dir.join("no.grf"), &o, &ctl),
        Err(Error::Unsupported(_))
    ));

    // with the key: plain output, the hidden entry is gone
    let src = Archive::open_with_key(&path, &key).unwrap();
    let plain = dir.join("plain.grf");
    repack(&src, &plain, &RepackOptions::default(), &ctl).unwrap();
    let p = Archive::open(&plain).unwrap();
    assert!(p.encryption().is_none());
    assert_eq!(p.len(), 4);
    let a = p.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert_eq!(p.read(a).unwrap().data, text(4000, 1));

    // another key
    let again = dir.join("again.grf");
    let o = RepackOptions {
        encrypt: Some(key2),
        ..RepackOptions::default()
    };
    repack(&src, &again, &o, &ctl).unwrap();
    assert!(matches!(
        Archive::open_with_key(&again, &key),
        Err(Error::WrongKey)
    ));
    let n = Archive::open_with_key(&again, &key2).unwrap();
    assert_eq!(n.len(), 5);
    let a = n.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert_eq!(n.read(a).unwrap().data, text(4000, 1));

    // plain archive to encrypted
    let enc = dir.join("enc2.grf");
    repack(&p, &enc, &o, &ctl).unwrap();
    assert!(Archive::open(&enc).unwrap().encryption().is_some());
    // only the 0x200 format can be encrypted
    let o300 = RepackOptions {
        encrypt: Some(key),
        format: Some(Format::V300),
        ..RepackOptions::default()
    };
    assert!(matches!(
        repack(&p, &dir.join("x.grf"), &o300, &ctl),
        Err(Error::Unsupported(_))
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn single_entries_can_be_encrypted_and_decrypted() {
    use std::collections::HashSet;
    let key = grfcrypt::key_from_password("secret").unwrap();
    let (dir, enc) = encrypted("crypt-single", &key);
    let name = |n: &str| GrfPath::new(n.as_bytes()).key();
    let a = name("data\\a.txt");
    let b = name("data\\sub\\b.spr");
    let opts = |only: &[&Vec<u8>]| RepackOptions {
        encrypt: Some(key),
        encrypt_only: Some(only.iter().map(|k| (*k).clone()).collect::<HashSet<_>>()),
        recompress_all: true,
        ..RepackOptions::default()
    };

    // the encrypted archive (opened with its key) keeps only a.txt encrypted
    let src = Archive::open_with_key(&enc, &key).unwrap();
    let one = dir.join("one.grf");
    let r = repack(&src, &one, &opts(&[&a]), &Control::default()).unwrap();
    assert!(r.verified);
    let g = Archive::open(&one).unwrap();
    assert_eq!(g.encryption().unwrap().encrypted_entries, 1);
    let ia = g.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    let ib = g.find(&GrfPath::new(&b"data\\sub\\b.spr"[..])).unwrap();
    assert!(g.entry_encrypted(ia) && !g.entry_encrypted(ib));
    // the plain one reads without the key, the encrypted one does not
    assert_eq!(g.read(ib).unwrap().data, text(9000, 2));
    assert_eq!(g.read(ia).unwrap().status, DecodeStatus::Encrypted);
    let g = Archive::open_with_key(&one, &key).unwrap();
    assert_eq!(g.read(ia).unwrap().data, text(4000, 1));

    // encrypting b.spr as well, from the mixed archive
    let both = dir.join("both.grf");
    repack(&g, &both, &opts(&[&a, &b]), &Control::default()).unwrap();
    let g2 = Archive::open_with_key(&both, &key).unwrap();
    assert_eq!(g2.encryption().unwrap().encrypted_entries, 2);
    assert_eq!(g2.read(ib).unwrap().data, text(9000, 2));

    // the same from a plain archive with a new key
    let plain = dir.join("plain.grf");
    repack(&src, &plain, &RepackOptions::default(), &Control::default()).unwrap();
    let p = Archive::open(&plain).unwrap();
    assert!(p.encryption().is_none());
    let sel = dir.join("sel.grf");
    repack(&p, &sel, &opts(&[&b]), &Control::default()).unwrap();
    let s = Archive::open_with_key(&sel, &key).unwrap();
    assert_eq!(s.encryption().unwrap().encrypted_entries, 1);
    assert!(s.entry_encrypted(s.find(&GrfPath::new(&b"data\\sub\\b.spr"[..])).unwrap()));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn an_encrypted_file_table_opens_with_the_key_only() {
    let key = grfcrypt::key_from_password("table key").unwrap();
    let (dir, path) = encrypted("crypt3", &key);
    let mut bytes = fs::read(&path).unwrap();
    // encrypt the compressed table the way GRF Editor does: counter = table size + 8
    let off = 46 + u32::from_le_bytes(bytes[30..34].try_into().unwrap()) as usize;
    let zsize = u32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
    let size = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap());
    grfcrypt::crypt(
        &key,
        &mut bytes[off + 8..off + 8 + zsize],
        u64::from(size) + 8,
    );
    let enc = dir.join("table.grf");
    fs::write(&enc, &bytes).unwrap();
    assert!(matches!(Archive::open(&enc), Err(Error::EncryptedTable)));
    let g = Archive::open_with_key(&enc, &key).unwrap();
    let a = g.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert_eq!(g.read(a).unwrap().data, text(4000, 1));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn the_file_table_alone_can_be_encrypted_and_decrypted() {
    let key = grfcrypt::key_from_password("table only").unwrap();
    let (dir, enc) = encrypted("crypt-table", &key);
    let src = Archive::open_with_key(&enc, &key).unwrap();
    assert!(!src.table_encrypted());
    // plain entries, encrypted table
    let out = dir.join("table.grf");
    let o = RepackOptions {
        encrypt_table: Some(key),
        ..RepackOptions::default()
    };
    let r = repack(&src, &out, &o, &Control::default()).unwrap();
    assert!(r.verified);
    assert!(matches!(Archive::open(&out), Err(Error::EncryptedTable)));
    let g = Archive::open_with_key(&out, &key).unwrap();
    assert!(g.table_encrypted() && g.encryption().is_none());
    let a = g.find(&GrfPath::new(&b"data\\a.txt"[..])).unwrap();
    assert_eq!(g.read(a).unwrap().data, text(4000, 1));
    // a wrong key does not read the table
    let wrong = grfcrypt::key_from_password("another").unwrap();
    assert!(Archive::open_with_key(&out, &wrong).is_err());
    // entries and table both encrypted, then the table plain again
    let both = dir.join("both.grf");
    let o = RepackOptions {
        encrypt: Some(key),
        encrypt_table: Some(key),
        ..RepackOptions::default()
    };
    repack(&g, &both, &o, &Control::default()).unwrap();
    let b = Archive::open_with_key(&both, &key).unwrap();
    assert!(b.table_encrypted());
    assert_eq!(b.encryption().unwrap().encrypted_entries, 4);
    let o = RepackOptions {
        encrypt: Some(key),
        ..RepackOptions::default()
    };
    let back = dir.join("back.grf");
    repack(&b, &back, &o, &Control::default()).unwrap();
    let c = Archive::open(&back).unwrap();
    assert!(!c.table_encrypted() && c.encryption().is_some());
    // the old formats cannot hold it
    let o = RepackOptions {
        encrypt_table: Some(key),
        format: Some(Format::V102),
        ..RepackOptions::default()
    };
    assert!(repack(&c, &dir.join("old.grf"), &o, &Control::default()).is_err());
    let _ = fs::remove_dir_all(dir);
}
