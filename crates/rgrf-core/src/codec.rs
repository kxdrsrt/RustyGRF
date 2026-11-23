//! Decoding of entry streams. Reads tolerantly like the client and reports what it saw.

use flate2::{Decompress, FlushDecompress, Status};
use std::io::Read;

use crate::container::flags;

/// Result of decoding one entry. Damaged entries still carry the data that could be recovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeStatus {
    Ok,
    /// Stream ended early; `got` bytes recovered (the client reads such entries as well).
    Truncated {
        got: usize,
        expected: usize,
    },
    /// Stream is complete but its length differs from the size in the table.
    SizeMismatch {
        got: usize,
        expected: usize,
    },
    /// Stream is damaged (bad header, bad data).
    Corrupt(String),
    /// Encrypted entry; decryption is not supported in this version, no data returned.
    Encrypted,
}

impl DecodeStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub data: Vec<u8>,
    pub status: DecodeStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Zlib,
    Lzma,
}

/// First byte `0x00` marks the unofficial LZMA stream (LessGRF), everything else is zlib.
pub fn sniff(stream: &[u8]) -> Option<Codec> {
    match stream.first() {
        None => None,
        Some(0x00) => Some(Codec::Lzma),
        Some(_) => Some(Codec::Zlib),
    }
}

const FIRST_CHUNK: usize = 1 << 20;

pub fn decode(stream: &[u8], size: usize, entry_flags: u8) -> Decoded {
    if entry_flags & flags::ANY_CRYPT != 0 {
        return Decoded {
            data: Vec::new(),
            status: DecodeStatus::Encrypted,
        };
    }
    if entry_flags & flags::FILE == 0 {
        // flags 0: an empty marker, or a file stored uncompressed (measured, see Entry::is_raw)
        let got = stream.len().min(size);
        let status = match stream.len().cmp(&size) {
            std::cmp::Ordering::Equal => DecodeStatus::Ok,
            std::cmp::Ordering::Less => DecodeStatus::Truncated {
                got,
                expected: size,
            },
            std::cmp::Ordering::Greater => DecodeStatus::SizeMismatch {
                got: stream.len(),
                expected: size,
            },
        };
        return Decoded {
            data: stream[..got].to_vec(),
            status,
        };
    }
    if size == 0 && stream.is_empty() {
        return Decoded {
            data: Vec::new(),
            status: DecodeStatus::Ok,
        };
    }
    match sniff(stream) {
        None => Decoded {
            data: Vec::new(),
            status: DecodeStatus::Truncated {
                got: 0,
                expected: size,
            },
        },
        Some(Codec::Zlib) => decode_zlib(stream, size),
        Some(Codec::Lzma) => decode_lzma(stream, size),
    }
}

fn decode_zlib(stream: &[u8], size: usize) -> Decoded {
    let limit = size.saturating_add(1);
    let mut d = Decompress::new(true);
    let mut out: Vec<u8> = Vec::with_capacity(limit.min(FIRST_CHUNK));
    let mut ended = false;
    let mut error = None;
    loop {
        if out.len() >= limit {
            break;
        }
        if out.len() == out.capacity() {
            out.reserve(out.capacity().max(FIRST_CHUNK).min(limit - out.len()));
        }
        let (in_before, out_before) = (d.total_in(), out.len());
        let input = &stream[usize::try_from(in_before)
            .unwrap_or(stream.len())
            .min(stream.len())..];
        match d.decompress_vec(input, &mut out, FlushDecompress::None) {
            Ok(Status::StreamEnd) => {
                ended = true;
                break;
            }
            Ok(_) if d.total_in() == in_before && out.len() == out_before => break,
            Ok(_) => {}
            Err(e) => {
                error = Some(e.message().unwrap_or("invalid zlib data").to_owned());
                break;
            }
        }
    }
    let got = out.len();
    if let Some(msg) = error {
        let msg = if got == size && deflate_body_is_complete(stream, size) {
            format!("Adler-32 checksum does not match; the content is complete ({got} bytes)")
        } else {
            msg
        };
        return Decoded {
            data: out,
            status: DecodeStatus::Corrupt(msg),
        };
    }
    if got > size {
        out.truncate(size);
        return Decoded {
            data: out,
            status: DecodeStatus::SizeMismatch {
                got: got.min(limit),
                expected: size,
            },
        };
    }
    let status = match (ended, got == size) {
        (true, true) => DecodeStatus::Ok,
        (true, false) => DecodeStatus::SizeMismatch {
            got,
            expected: size,
        },
        (false, _) => DecodeStatus::Truncated {
            got,
            expected: size,
        },
    };
    Decoded { data: out, status }
}

/// Whether the deflate data behind the 2-byte zlib header ends properly after exactly `size`
/// bytes, i.e. only the Adler-32 checksum behind it is wrong (measured: 284 entries of desu.grf).
fn deflate_body_is_complete(stream: &[u8], size: usize) -> bool {
    let Some(body) = stream.get(2..) else {
        return false;
    };
    let mut d = Decompress::new(false);
    let mut out = Vec::with_capacity(size.saturating_add(1));
    matches!(
        d.decompress_vec(body, &mut out, FlushDecompress::Finish),
        Ok(Status::StreamEnd)
    ) && out.len() == size
}

/// Layout (measured, see docs/02-format-notes.md): `00`, LZMA properties byte, dictionary size
/// (u32 little endian), then the raw LZMA stream; the size comes from the file table.
fn decode_lzma(stream: &[u8], size: usize) -> Decoded {
    const HEADER: usize = 6;
    if stream.len() < HEADER {
        return Decoded {
            data: Vec::new(),
            status: DecodeStatus::Corrupt("LZMA stream shorter than its 6-byte header".into()),
        };
    }
    let props = stream[1];
    let dict = u32::from_le_bytes([stream[2], stream[3], stream[4], stream[5]]);
    let mut out = Vec::with_capacity(size.min(FIRST_CHUNK * 64));
    let reader = match lzma_rust2::LzmaReader::new_with_props(
        &stream[HEADER..],
        size as u64,
        props,
        dict,
        None,
    ) {
        Ok(r) => r,
        Err(e) => {
            return Decoded {
                data: out,
                status: DecodeStatus::Corrupt(format!("LZMA header: {e}")),
            };
        }
    };
    let result = reader.take(size as u64).read_to_end(&mut out);
    let got = out.len();
    let status = match result {
        Ok(_) if got == size => DecodeStatus::Ok,
        Ok(_) => DecodeStatus::Truncated {
            got,
            expected: size,
        },
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => DecodeStatus::Truncated {
            got,
            expected: size,
        },
        Err(e) => DecodeStatus::Corrupt(format!("LZMA data: {e}")),
    };
    Decoded { data: out, status }
}

/// Helpers that build entry streams for tests (shared with the synthetic archive tests).
#[cfg(test)]
pub(crate) mod tests_support {
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    pub fn zlib_level(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    pub fn zlib(data: &[u8]) -> Vec<u8> {
        zlib_level(data, 9)
    }

    /// Same layout as the measured entries: 00, props, dict size (3 MiB), raw LZMA.
    pub fn lzma_stream(data: &[u8]) -> Vec<u8> {
        let mut opts = lzma_rust2::LzmaOptions::with_preset(6);
        opts.dict_size = 3 << 20;
        let mut raw = Vec::new();
        {
            let mut w = lzma_rust2::LzmaWriter::new_no_header(&mut raw, &opts, false).unwrap();
            w.write_all(data).unwrap();
            w.finish().unwrap();
        }
        let mut s = vec![0x00, opts.get_props()];
        s.extend_from_slice(&opts.dict_size.to_le_bytes());
        s.extend_from_slice(&raw);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::tests_support::{lzma_stream, zlib_level as zlib};
    use super::*;
    fn sample(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| u8::try_from((i * 7 + i / 13) % 251).unwrap())
            .collect()
    }

    #[test]
    fn zlib_roundtrip_including_large_and_empty() {
        for n in [0usize, 1, 100, 5_000, 3_000_000] {
            let data = sample(n);
            let d = decode(&zlib(&data, 9), n, flags::FILE);
            assert_eq!(d.status, DecodeStatus::Ok, "n={n}");
            assert_eq!(d.data, data);
        }
    }

    #[test]
    fn zlib_ignores_alignment_padding_after_the_stream() {
        let data = sample(1000);
        let mut s = zlib(&data, 6);
        s.extend_from_slice(&[0, 0, 0, 0, 0]);
        assert_eq!(decode(&s, data.len(), flags::FILE).data, data);
    }

    #[test]
    fn truncated_zlib_returns_partial_data_and_reports_it() {
        // incompressible data -> stored blocks, like worm_tail_damage.wav
        let data: Vec<u8> = (0..2000u32)
            .map(|i| u8::try_from(i.wrapping_mul(2_654_435_761) >> 24).unwrap())
            .collect();
        let s = zlib(&data, 0);
        let cut = &s[..s.len() - 11];
        let d = decode(cut, data.len(), flags::FILE);
        assert!(
            matches!(d.status, DecodeStatus::Truncated { expected: 2000, .. }),
            "{:?}",
            d.status
        );
        assert!(!d.data.is_empty() && d.data.len() < data.len());
        assert_eq!(d.data, data[..d.data.len()]);
    }

    #[test]
    fn size_mismatch_is_reported_both_ways() {
        let data = sample(500);
        let s = zlib(&data, 9);
        let shorter = decode(&s, 400, flags::FILE);
        assert!(matches!(
            shorter.status,
            DecodeStatus::SizeMismatch { expected: 400, .. }
        ));
        assert_eq!(shorter.data.len(), 400);
        let longer = decode(&s, 600, flags::FILE);
        assert_eq!(
            longer.status,
            DecodeStatus::SizeMismatch {
                got: 500,
                expected: 600
            }
        );
    }

    #[test]
    fn corrupt_zlib_is_reported() {
        let d = decode(&[0x78, 0x9c, 0xff, 0xff, 0xff, 0xff], 100, flags::FILE);
        assert!(
            matches!(d.status, DecodeStatus::Corrupt(_)),
            "{:?}",
            d.status
        );
    }

    #[test]
    fn wrong_checksum_with_complete_content_is_named() {
        let data = sample(5_000);
        let mut s = zlib(&data, 9);
        let n = s.len();
        s[n - 1] ^= 0xff;
        let d = decode(&s, data.len(), flags::FILE);
        match &d.status {
            DecodeStatus::Corrupt(m) => assert!(m.contains("Adler-32"), "{m}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(d.data, data);
        // damaged deflate data is not called a checksum problem
        let mut bad = zlib(&data, 9);
        bad[10] ^= 0xff;
        if let DecodeStatus::Corrupt(m) = decode(&bad, data.len(), flags::FILE).status {
            assert!(!m.contains("Adler-32"), "{m}");
        }
    }

    #[test]
    fn markers_encrypted_and_empty() {
        assert_eq!(decode(&[], 0, 0).status, DecodeStatus::Ok);
        assert_eq!(decode(&[1, 2, 3], 5, 0x03).status, DecodeStatus::Encrypted);
        assert_eq!(decode(&[], 0, flags::FILE).status, DecodeStatus::Ok);
        assert!(matches!(
            decode(&[], 5, flags::FILE).status,
            DecodeStatus::Truncated {
                got: 0,
                expected: 5
            }
        ));
    }

    #[test]
    fn flags_zero_entries_with_data_are_stored_uncompressed() {
        // measured: the flags-0 entries of the 0x300 archives are PNGs with zsize == size
        let d = decode(b"\x89PNG data", 9, 0);
        assert_eq!(
            (d.status, d.data),
            (DecodeStatus::Ok, b"\x89PNG data".to_vec())
        );
        let short = decode(b"abc", 9, 0);
        assert_eq!(
            short.status,
            DecodeStatus::Truncated {
                got: 3,
                expected: 9
            }
        );
        assert_eq!(short.data, b"abc");
        let long = decode(b"abcdef", 3, 0);
        assert_eq!(
            long.status,
            DecodeStatus::SizeMismatch {
                got: 6,
                expected: 3
            }
        );
        assert_eq!(long.data, b"abc");
    }

    #[test]
    fn lzma_roundtrip() {
        for n in [1usize, 2_000, 400_000] {
            let data = sample(n);
            let d = decode(&lzma_stream(&data), n, flags::FILE);
            assert_eq!(d.status, DecodeStatus::Ok, "n={n}");
            assert_eq!(d.data, data);
        }
    }

    #[test]
    fn truncated_lzma_is_reported() {
        let data = sample(50_000);
        let s = lzma_stream(&data);
        let d = decode(&s[..s.len() / 2], data.len(), flags::FILE);
        assert!(!d.status.is_ok(), "{:?}", d.status);
    }

    #[test]
    fn sniffing() {
        assert_eq!(sniff(&[0x78, 1]), Some(Codec::Zlib));
        assert_eq!(sniff(&[0x00, 0x5d]), Some(Codec::Lzma));
        assert_eq!(sniff(&[]), None);
    }
}
