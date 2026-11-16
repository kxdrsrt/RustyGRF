//! Encoders for entry streams: zlib (standard), Zopfli (zlib-compatible, smaller, slow) and the
//! unofficial LZMA layout of LessGRF/`cps.dll` (see docs/02-format-notes.md).

use flate2::{Compression, write::ZlibEncoder};
use std::io::Write;

use crate::codec::Codec;

/// LZMA dictionary size of the measured reference entries (3 MiB).
pub const LZMA_DICT_SIZE: u32 = 3 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Standard zlib, level 0..=9. Readable by every client.
    Zlib { level: u32 },
    /// Zopfli with this many iterations; the result is a plain zlib stream.
    Zopfli { iterations: u64 },
    /// Unofficial LZMA (needs the LZMA-capable `cps.dll` in the client), preset 0..=9.
    Lzma { preset: u32 },
}

impl Method {
    pub const DEFAULT: Self = Self::Zlib { level: 9 };

    /// Codec of the streams this method produces (Zopfli makes zlib streams).
    pub fn codec(self) -> Codec {
        match self {
            Self::Zlib { .. } | Self::Zopfli { .. } => Codec::Zlib,
            Self::Lzma { .. } => Codec::Lzma,
        }
    }
}

impl Default for Method {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn zlib(data: &[u8], level: u32) -> Vec<u8> {
    let mut e = ZlibEncoder::new(
        Vec::with_capacity(data.len() / 2 + 64),
        Compression::new(level.min(9)),
    );
    e.write_all(data).expect("writing to a Vec cannot fail");
    e.finish().expect("writing to a Vec cannot fail")
}

fn zopfli(data: &[u8], iterations: u64) -> Vec<u8> {
    let options = zopfli::Options {
        iteration_count: std::num::NonZeroU64::new(iterations.max(1)).expect("non-zero"),
        ..zopfli::Options::default()
    };
    let mut out = Vec::with_capacity(data.len() / 2 + 64);
    zopfli::compress(options, zopfli::Format::Zlib, data, &mut out)
        .expect("writing to a Vec cannot fail");
    out
}

/// `00`, LZMA properties, dictionary size (u32 LE), raw LZMA stream without end marker; the
/// unpacked size lives in the file table.
fn lzma(data: &[u8], preset: u32) -> Vec<u8> {
    let mut opts = lzma_rust2::LzmaOptions::with_preset(preset.min(9));
    opts.dict_size = LZMA_DICT_SIZE;
    if preset >= 9 {
        // presets 6-9 are identical in this encoder; the longest match length gives -1.0 % on
        // real entries (cdata, measured) for 1.9x the time. lc/lp/pb stay at the reference 3/0/2.
        opts.nice_len = lzma_rust2::LzmaOptions::NICE_LEN_MAX;
    }
    let mut raw = Vec::with_capacity(data.len() / 3 + 64);
    {
        let mut w = lzma_rust2::LzmaWriter::new_no_header(&mut raw, &opts, false)
            .expect("valid LZMA options");
        w.write_all(data).expect("writing to a Vec cannot fail");
        w.finish().expect("writing to a Vec cannot fail");
    }
    let mut s = Vec::with_capacity(raw.len() + 6);
    s.push(0x00);
    s.push(opts.get_props());
    s.extend_from_slice(&opts.dict_size.to_le_bytes());
    s.extend_from_slice(&raw);
    s
}

/// Encodes `data`. Never returns something larger than a stored zlib stream: incompressible data
/// (PNG, WAV, ...) falls back to stored blocks, and for LZMA also empty and tiny inputs fall back
/// to zlib (zlib and LZMA entries can be mixed in one archive).
pub fn encode(method: Method, data: &[u8]) -> Vec<u8> {
    let best = match method {
        Method::Zlib { level } => zlib(data, level),
        Method::Zopfli { iterations } => zopfli(data, iterations),
        Method::Lzma { .. } if data.is_empty() => zlib(data, 9),
        Method::Lzma { preset } => lzma(data, preset),
    };
    if best.len() >= data.len() {
        let stored = zlib(data, 0);
        if stored.len() < best.len() {
            return stored;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::{DecodeStatus, decode, sniff};
    use crate::container::flags;

    fn text(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| b"the quick brown fox jumps over the lazy dog "[i % 44])
            .collect()
    }

    fn noise(n: usize) -> Vec<u8> {
        let mut x = 0x2545_F491_4F6C_DD1Du64;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                (x >> 24) as u8
            })
            .collect()
    }

    const METHODS: [Method; 4] = [
        Method::Zlib { level: 1 },
        Method::Zlib { level: 9 },
        Method::Zopfli { iterations: 3 },
        Method::Lzma { preset: 6 },
    ];

    #[test]
    fn every_method_roundtrips_through_the_reader() {
        for m in METHODS {
            for data in [
                Vec::new(),
                vec![7u8],
                text(5000),
                noise(3000),
                text(400_000),
            ] {
                let s = encode(m, &data);
                let d = decode(&s, data.len(), flags::FILE);
                assert_eq!(d.status, DecodeStatus::Ok, "{m:?} n={}", data.len());
                assert_eq!(d.data, data, "{m:?} n={}", data.len());
            }
        }
    }

    #[test]
    fn streams_have_the_right_codec_and_layout() {
        let data = text(20_000);
        assert_eq!(
            sniff(&encode(Method::Zlib { level: 9 }, &data)),
            Some(Codec::Zlib)
        );
        assert_eq!(
            sniff(&encode(Method::Zopfli { iterations: 2 }, &data)),
            Some(Codec::Zlib)
        );
        let l = encode(Method::Lzma { preset: 6 }, &data);
        assert_eq!(sniff(&l), Some(Codec::Lzma));
        assert_eq!(l[1], 0x5d, "lc3 lp0 pb2 like the reference entries");
        assert_eq!(&l[2..6], &[0x00, 0x00, 0x30, 0x00], "3 MiB dictionary");
        assert_eq!(l[6], 0x00, "raw LZMA stream starts with 00");
    }

    #[test]
    fn incompressible_data_never_grows_beyond_stored_zlib() {
        let data = noise(100_000);
        for m in METHODS {
            let s = encode(m, &data);
            assert_eq!(sniff(&s), Some(Codec::Zlib), "{m:?} falls back to zlib");
            // stored zlib: 6 bytes of header/checksum plus 5 per block (blocks of at least 16 KiB)
            assert!(
                s.len() <= data.len() + 6 + 5 * data.len().div_ceil(16_384),
                "{m:?} {}",
                s.len()
            );
        }
    }

    #[test]
    fn empty_input_is_zlib_even_for_lzma() {
        assert_eq!(
            sniff(&encode(Method::Lzma { preset: 9 }, &[])),
            Some(Codec::Zlib)
        );
    }

    #[test]
    fn zopfli_is_smaller_than_zlib_and_output_is_deterministic() {
        let data = text(60_000);
        let z = encode(Method::Zlib { level: 9 }, &data);
        let p = encode(Method::Zopfli { iterations: 5 }, &data);
        assert!(p.len() <= z.len());
        assert_eq!(p, encode(Method::Zopfli { iterations: 5 }, &data));
        let l = encode(Method::Lzma { preset: 6 }, &data);
        assert_eq!(l, encode(Method::Lzma { preset: 6 }, &data));
    }

    #[test]
    fn method_reports_its_codec() {
        assert_eq!(Method::DEFAULT.codec(), Codec::Zlib);
        assert_eq!(Method::Zopfli { iterations: 1 }.codec(), Codec::Zlib);
        assert_eq!(Method::Lzma { preset: 9 }.codec(), Codec::Lzma);
    }
}
