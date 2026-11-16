//! GRF 0x300: 64-bit offsets, key block, header rules (builds on the helpers of the parent module).

use super::*;

fn open300(entries: &[T], key_block: usize) -> Archive<Vec<u8>> {
    Archive::from_source(build_with(entries, Layout::V300 { key_block })).unwrap()
}

#[test]
fn reads_zlib_lzma_markers_and_reports_the_version() {
    let s = sample();
    let mut lz = file("data\\b.act", &s);
    lz.stream = Stream::Lzma(s.clone());
    let g = open300(
        &[
            file("data\\a.spr", &s),
            lz,
            file("data\\e.txt", b""),
            marker("data\\dir.txt"),
        ],
        0,
    );
    let t = g.table_info();
    assert_eq!(
        (g.header().version, g.len(), t.prefix_len, t.key_block),
        (0x300, 4, 12, 0)
    );
    assert_eq!(&g.header().magic[..13], b"Event Horizon");
    for i in 0..2 {
        let d = g.read(i).unwrap();
        assert_eq!(
            (d.status, d.data),
            (DecodeStatus::Ok, s.clone()),
            "entry {i}"
        );
    }
    assert!(g.entry(3).unwrap().is_marker());
    let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
    assert_eq!((rep.exit_code(), rep.markers), (0, 1), "{:?}", rep.issues);
}

#[test]
fn key_block_is_accounted_for_and_bounded() {
    let s = sample();
    let g = open300(&[file("data\\a", &s)], 64);
    assert_eq!(g.table_info().key_block, 64);
    assert_eq!(g.table_info().end(), g.file_len());
    let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
    assert!(kinds(&rep, Severity::Info).contains(&IssueKind::KeyBlock));
    assert_eq!(rep.exit_code(), 0);

    // key block claimed larger than the file, and larger than the allowed maximum
    let mut bytes = build_with(&[file("data\\a", &s)], Layout::V300 { key_block: 0 });
    let at = g.table_info().abs_offset as usize;
    bytes[at..at + 4].copy_from_slice(&1000u32.to_le_bytes());
    assert!(matches!(
        Archive::from_source(bytes.clone()),
        Err(Error::CorruptTable(_))
    ));
    bytes[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(matches!(
        Archive::from_source(bytes),
        Err(Error::CorruptTable(_))
    ));
}

#[test]
fn entries_with_the_gravity_flag_are_reported_as_encrypted() {
    let mut e = file("data\\protected.lub", &sample());
    e.flags = flags::FILE | flags::GRAVITY_CRYPT;
    let g = open300(&[e], 16);
    assert!(g.entry(0).unwrap().is_encrypted());
    assert_eq!(g.read(0).unwrap().status, DecodeStatus::Encrypted);
}

#[test]
fn header_with_a_seed_uses_the_32_bit_layout_like_grf_editor() {
    let s = sample();
    let mut bytes = build_with(
        &[file("data\\a", &s), file("data\\b", &s)],
        Layout::V300 { key_block: 0 },
    );
    // bytes 35..37 are not zero, so 34..38 is a seed and not the upper half of a 64-bit offset
    bytes[34..38].copy_from_slice(&0x0100_0000u32.to_le_bytes());
    bytes[38..42].copy_from_slice(&(2u32 + 7 + 0x0100_0000).to_le_bytes());
    let g = Archive::from_source(bytes).unwrap();
    assert_eq!((g.len(), g.header().seed), (2, 0x0100_0000));
}

/// A "file" of `len` bytes that is zero except for the given segments, so offsets above 4 GiB
/// can be tested without storing gigabytes.
struct Sparse {
    len: u64,
    segs: Vec<(u64, Vec<u8>)>,
}

impl crate::io::ReadAt for Sparse {
    fn read_exact_at(&self, buf: &mut [u8], offset: u64) -> std::io::Result<()> {
        let end = offset + buf.len() as u64;
        if end > self.len {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        buf.fill(0);
        for (start, bytes) in &self.segs {
            let (lo, hi) = (offset.max(*start), end.min(start + bytes.len() as u64));
            if lo < hi {
                buf[(lo - offset) as usize..(hi - offset) as usize]
                    .copy_from_slice(&bytes[(lo - start) as usize..(hi - start) as usize]);
            }
        }
        Ok(())
    }

    fn len(&self) -> std::io::Result<u64> {
        Ok(self.len)
    }
}

#[test]
fn reads_data_and_table_beyond_4_gib() {
    const GAP: u64 = 5 << 30;
    let near = b"near the start".to_vec();
    let far = sample();
    let mut e_far = file("data\\far.bin", &far);
    e_far.offset = Some(GAP);
    let entries = [file("data\\near.txt", &near), e_far];
    let (mut header, data, tail) = build_parts(&entries, Layout::V300 { key_block: 0 });
    let near_len = tests_support::zlib(&near).len().div_ceil(8) * 8;
    let far_stream = &data[near_len..];
    let table_at = GAP + far_stream.len() as u64; // relative to the end of the header
    header[30..38].copy_from_slice(&table_at.to_le_bytes());
    let src = Sparse {
        len: 46 + table_at + tail.len() as u64,
        segs: vec![
            (0, header),
            (46, data[..near_len].to_vec()),
            (46 + GAP, far_stream.to_vec()),
            (46 + table_at, tail),
        ],
    };
    let g = Archive::from_source(src).unwrap();
    assert!(g.table_info().abs_offset > u64::from(u32::MAX));
    assert!(g.entry(1).unwrap().abs_offset() > u64::from(u32::MAX));
    assert_eq!(g.read(0).unwrap().data, near);
    let d = g.read(1).unwrap();
    assert_eq!((d.status, d.data), (DecodeStatus::Ok, far));
    let rep = verify(&g, VerifyOptions::default(), &Control::default()).unwrap();
    assert_eq!(rep.exit_code(), 0, "{:?}", rep.issues);
}

#[test]
fn absurd_64_bit_table_offset_is_rejected_without_panic() {
    let mut bytes = build_with(&[file("data\\a", b"x")], Layout::V300 { key_block: 0 });
    bytes[30..38].copy_from_slice(&u64::MAX.to_le_bytes()); // bytes 35..37 non-zero: 32-bit path
    assert!(Archive::from_source(bytes.clone()).is_err());
    bytes[30..38].copy_from_slice(&(1u64 << 36).to_le_bytes()); // 64-bit path, beyond the file
    assert!(matches!(
        Archive::from_source(bytes),
        Err(Error::CorruptTable(_))
    ));
}
