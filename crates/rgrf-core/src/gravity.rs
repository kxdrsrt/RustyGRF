//! Gravity's protection of single entries (flag `0x80`, GRF 0x300) and the archive key block
//! behind the file table.
//!
//! Ported from GRF Editor by Tokeiburu (`GRF/Core/GrfCompression/ProtectedGrfDecoder.cs` and
//! `ProtectedGrfCiphers.cs`, with permission, see `docs/05-licensing.md`). The kRO variant is
//! ported: entry headers of type 1, 2, 15 and 113, body ciphers 1, 5, 7, 128 and 133, data stored,
//! zlib or LZ4, key blocks encrypted with CHAM or holding the kRO default keys. Not ported: the
//! jRO (header 241, bodies 259, 260, 269) and Chinese variants; such entries are reported as
//! unsupported. Checked against the 40 vectors GRF Editor ships (`tests/fixtures`).
//!
//! An entry is a 104-byte header (encrypted, with an XXH32 checksum) and a body whose first 128
//! bytes are plain and whose rest is encrypted with keys derived from the raw file name bytes,
//! the output hash (XXH3-128) and key material from the header or the archive key block.

use std::collections::HashMap;

use aes::Aes128;
use aes::cipher::{Array, BlockCipherDecrypt, KeyInit};
use sha2::{Digest, Sha256};
use twox_hash::{XxHash3_128, XxHash32};

pub const HEADER_SIZE: usize = 104;
const MAX_OUTPUT: usize = 256 << 20;
const MAX_PACKED: usize = 258 << 20;

/// Key material by key id, from an archive's key block.
pub type ArchiveKeys = HashMap<u32, [u8; 60]>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GravityError {
    /// The data contradicts itself (sizes, checksums, padding).
    #[error("{0}")]
    Invalid(String),
    /// A variant GRF Editor can read but that is not ported.
    #[error("{0}")]
    Unsupported(String),
}

type Result<T> = std::result::Result<T, GravityError>;

fn invalid<T>(msg: impl Into<String>) -> Result<T> {
    Err(GravityError::Invalid(msg.into()))
}

/// Gravity's kRO archive keys 1 to 4, as GRF Editor embeds them (`GrfHeader.KRODefaultProtectedKeys`).
const KRO_DEFAULT_KEYS: [[u8; 60]; 4] = [
    [
        0x2c, 0x94, 0x21, 0x96, 0x40, 0x67, 0x35, 0xfe, 0xd0, 0xc6, 0xfc, 0x9e, 0x15, 0xb9, 0x48,
        0x06, 0x17, 0x19, 0x6b, 0x53, 0x4e, 0xaf, 0x25, 0xac, 0x03, 0x50, 0x81, 0xf2, 0xb6, 0xd4,
        0xa3, 0x71, 0x51, 0xcd, 0xaf, 0x68, 0x47, 0x78, 0x6d, 0xd3, 0xe4, 0x04, 0x03, 0x2f, 0x86,
        0x03, 0x38, 0x3e, 0xc7, 0x49, 0x20, 0x03, 0x8a, 0x9e, 0x63, 0x04, 0x07, 0x57, 0x7e, 0x0b,
    ],
    [
        0x2a, 0x08, 0xda, 0x55, 0x41, 0x0a, 0x5d, 0xd3, 0x4a, 0xb5, 0xc1, 0xa4, 0x04, 0xb1, 0x67,
        0xd9, 0xcd, 0x6b, 0x60, 0xe3, 0x49, 0xfa, 0x8a, 0x60, 0x25, 0xbd, 0x0d, 0x28, 0x2c, 0x05,
        0x50, 0xbd, 0x4c, 0x60, 0x67, 0x59, 0xc3, 0xdc, 0x9e, 0xb2, 0x56, 0xb1, 0x45, 0x5b, 0x74,
        0x3e, 0x81, 0xf9, 0xbc, 0x8a, 0x06, 0x6e, 0x76, 0xb6, 0x05, 0x4f, 0x11, 0xc1, 0xa3, 0x01,
    ],
    [
        0x1e, 0x5a, 0xb5, 0x22, 0xd9, 0x73, 0x34, 0xc9, 0x9b, 0xe5, 0x41, 0x33, 0x36, 0x17, 0x7a,
        0xbe, 0x59, 0xe6, 0x77, 0x8c, 0xe6, 0xd5, 0x4a, 0xa3, 0xdf, 0x2b, 0x26, 0x56, 0xb1, 0x80,
        0xde, 0xfa, 0xe6, 0x7c, 0x0a, 0x11, 0xf5, 0x54, 0xce, 0x76, 0x15, 0xbf, 0xe6, 0xc4, 0x13,
        0x02, 0xfe, 0x05, 0x9c, 0xe0, 0xec, 0x6f, 0x59, 0xec, 0x7e, 0xd3, 0x49, 0x3b, 0x3d, 0x93,
    ],
    [
        0x58, 0x15, 0x32, 0x06, 0x74, 0x9f, 0x6d, 0x2a, 0x33, 0xa2, 0x0d, 0x4e, 0x51, 0xc7, 0xf5,
        0x65, 0xb3, 0xc8, 0x04, 0xc8, 0xd7, 0x61, 0x71, 0x76, 0x64, 0xdb, 0x6b, 0x81, 0xdd, 0xbf,
        0x54, 0xe1, 0x4b, 0x07, 0x7c, 0x72, 0x8b, 0xad, 0x59, 0x0a, 0x69, 0xe7, 0x6e, 0xa0, 0xd3,
        0x80, 0x5c, 0xcb, 0x17, 0x28, 0x35, 0xd2, 0x1a, 0xa6, 0xec, 0x46, 0xfd, 0x95, 0x29, 0x8c,
    ],
];

fn word(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn big_word(d: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}

fn put_big(d: &mut [u8], at: usize, v: u32) {
    d[at..at + 4].copy_from_slice(&v.to_be_bytes());
}

fn put_le(d: &mut [u8], at: usize, v: u32) {
    d[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

/// XXH3-128 with `seed`, bytes as the archive stores them (low half first, little-endian).
pub fn native_hash(data: &[u8], seed: u64) -> [u8; 16] {
    XxHash3_128::oneshot_with_seed(seed, data).to_le_bytes()
}

fn xxh32(data: &[u8]) -> u32 {
    XxHash32::oneshot(0, data)
}

/// Reads the key block behind the file table of a 0x300 archive (empty block: no keys).
pub fn decode_archive_keys(raw: &[u8]) -> Result<ArchiveKeys> {
    let mut keys = ArchiveKeys::new();
    if raw.is_empty() {
        return Ok(keys);
    }
    if raw.len() < 20 {
        return invalid("unsupported or truncated archive key block");
    }
    let size = word(raw, 12) as usize;
    if size == 0 || !size.is_multiple_of(64) || size / 64 > 65535 || size != raw.len() - 20 {
        return Err(GravityError::Unsupported(
            "archive key block of an unknown layout (perhaps the jRO or Chinese variant, not supported)"
                .into(),
        ));
    }
    let mut decoded = raw[20..20 + size].to_vec();
    if raw[..4] != [1, 1, 0, 0] {
        // a block that stands for the kRO default keys: valid when its checksum matches them
        let n = KRO_DEFAULT_KEYS.len().min(decoded.len() / 64);
        for (i, key) in KRO_DEFAULT_KEYS.iter().enumerate().take(n) {
            put_le(&mut decoded, 64 * i, i as u32 + 1);
            decoded[64 * i + 4..64 * i + 64].copy_from_slice(key);
        }
        if xxh32(&decoded) != word(raw, 16) {
            return Err(GravityError::Unsupported(
                "archive key block not recognised".into(),
            ));
        }
        return Ok(KRO_DEFAULT_KEYS
            .iter()
            .enumerate()
            .map(|(i, k)| (i as u32 + 1, *k))
            .collect());
    }
    let digest = Sha256::digest(b"uf8E8qAwHthrLxiS");
    cham_cbc_decrypt(&mut decoded, &digest[..16], &digest[16..32])?;
    if xxh32(&decoded) != word(raw, 16) {
        return invalid("archive key block checksum mismatch");
    }
    for rec in decoded.as_chunks::<64>().0 {
        let id = word(rec, 0);
        if id == 0 || id > 65535 || keys.contains_key(&id) {
            return invalid("invalid or duplicate record in the archive key block");
        }
        let mut k = [0u8; 60];
        k.copy_from_slice(&rec[4..64]);
        keys.insert(id, k);
    }
    Ok(keys)
}

/// Decodes the stored bytes (`zsize` long) of a protected entry. `raw_name` are the name bytes as
/// stored in the file table, `expected` the decoded size from the table.
pub fn decode(
    payload: &[u8],
    raw_name: &[u8],
    expected: usize,
    keys: &ArchiveKeys,
) -> Result<Vec<u8>> {
    if expected > MAX_OUTPUT {
        return invalid("protected entry larger than the limit");
    }
    let name_hash = native_hash(raw_name, 0);
    let header = decode_header(payload, &name_hash)?;
    let padded = word(&header, 12) as usize;
    let compressed = word(&header, 16) as usize;
    let output = word(&header, 20) as usize;
    if output != expected || output > MAX_OUTPUT {
        return invalid("output size differs from the file table");
    }
    if padded > MAX_PACKED || padded > payload.len() - HEADER_SIZE || compressed > padded {
        return invalid("body size lies outside the stored data");
    }
    let cipher = u16::from_le_bytes([header[8], header[9]]);
    match cipher {
        1 | 5 | 7 | 128 | 133 => {}
        259 | 260 | 269 => {
            return Err(GravityError::Unsupported(format!(
                "body cipher {cipher} of the jRO variant (not supported)"
            )));
        }
        _ => return invalid(format!("unsupported body cipher {cipher}")),
    }
    let compression = header[10];
    if compression > 2 {
        return invalid(format!("unsupported compression {compression}"));
    }
    if header[11] & !1 != 0 {
        return invalid(format!("unsupported parameter flags {}", header[11]));
    }
    let key_id = word(&header, 40);
    let material: [u8; 60] = if key_id == 0 {
        header[44..104].try_into().expect("60 bytes")
    } else {
        *keys
            .get(&key_id)
            .filter(|_| key_id <= 65535)
            .ok_or_else(|| GravityError::Invalid(format!("missing archive key {key_id}")))?
    };
    let mut output_hash = [0u8; 16];
    output_hash.copy_from_slice(&header[24..40]);
    let mut params = [0u8; 64];
    for i in 0..60 {
        params[i] = material[i]
            ^ if header[11] & 1 != 0 {
                output_hash[i % 16]
            } else {
                0
            };
    }
    let mut body = payload[HEADER_SIZE..HEADER_SIZE + padded].to_vec();
    if compressed > 128 {
        match cipher {
            1 => decrypt_cham_tail(
                &params,
                &native_hash(raw_name, expected as u64),
                &output_hash,
                &mut body,
                128,
            )?,
            5 | 7 => decrypt_legacy_tail(
                cipher,
                &params,
                &name_hash,
                &output_hash,
                &mut body,
                128,
                compressed - 128,
            )?,
            _ => decrypt_body_tail(cipher, &params, &name_hash, &output_hash, &mut body, 128)?,
        }
    }
    if compressed > body.len() {
        return invalid("decoded body shorter than declared");
    }
    let out = match compression {
        0 => {
            if compressed != expected {
                return invalid("stored output size mismatch");
            }
            body.truncate(expected);
            body
        }
        1 => inflate_exact(&body[..compressed], expected)?,
        _ => {
            let mut out = vec![0u8; expected];
            let ok = if expected == 0 {
                compressed == 1 && body[0] == 0
            } else {
                lz4_flex::block::decompress_into(&body[..compressed], &mut out)
                    .is_ok_and(|n| n == expected)
            };
            if !ok {
                return invalid("invalid LZ4 stream or output size mismatch");
            }
            out
        }
    };
    if native_hash(&out, 0) != output_hash {
        return invalid("output hash mismatch");
    }
    Ok(out)
}

fn decode_header(payload: &[u8], name_hash: &[u8; 16]) -> Result<[u8; HEADER_SIZE]> {
    if payload.len() < HEADER_SIZE || payload[0] != 1 {
        return invalid("unsupported or truncated entry header");
    }
    if payload[1] == 241 {
        return Err(GravityError::Unsupported(
            "entry header of the jRO variant (not supported)".into(),
        ));
    }
    let mut header = [0u8; HEADER_SIZE];
    header.copy_from_slice(&payload[..HEADER_SIZE]);
    match header[1] {
        1 | 2 | 15 => {
            const KEY: [u8; 16] = [
                0x56, 0xfd, 0x7e, 0x71, 0x2f, 0x6d, 0x36, 0xf5, 0x5c, 0xa5, 0x99, 0xe6, 0x7b, 0x18,
                0x10, 0x7a,
            ];
            const IV1: [u8; 16] = [
                0x4c, 0xe8, 0xa1, 0xae, 0x45, 0xb3, 0x21, 0x60, 0x7e, 0x4f, 0x26, 0xa4, 0xbb, 0x14,
                0x0c, 0x96,
            ];
            let iv = match header[1] {
                1 => IV1,
                2 => *name_hash,
                _ => native_hash(&header[4..8], 0),
            };
            aes_cbc_decrypt(&mut header[8..104], &KEY, &iv)?;
        }
        113 => three_way_cfb_decrypt(
            &mut header[8..104],
            &[
                0xad, 0x61, 0xae, 0xda, 0xbf, 0x20, 0x3e, 0x1d, 0xfe, 0x1d, 0xb7, 0x2b,
            ],
            &[
                0xa0, 0x95, 0xdf, 0xcf, 0xd0, 0x4b, 0x74, 0xdb, 0x02, 0x7e, 0xde, 0x09,
            ],
        )?,
        t => return invalid(format!("unsupported entry header type {t}")),
    }
    if xxh32(&header[8..104]) != word(&header, 4) {
        return invalid("entry header checksum mismatch");
    }
    Ok(header)
}

/// zlib with an exact output size; a stream that is cut, damaged or longer is an error.
fn inflate_exact(stream: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = vec![0u8; expected + 1];
    let mut z = flate2::Decompress::new(true);
    let status = z.decompress(stream, &mut out, flate2::FlushDecompress::Finish);
    if !matches!(status, Ok(flate2::Status::StreamEnd))
        || z.total_in() != stream.len() as u64
        || z.total_out() != expected as u64
    {
        return invalid("invalid zlib stream: cut, checksum failed or size differs");
    }
    out.truncate(expected);
    Ok(out)
}

// ---------- ciphers ----------

fn aes_cbc_decrypt(data: &mut [u8], key: &[u8], iv: &[u8]) -> Result<()> {
    if !data.len().is_multiple_of(16) {
        return invalid("incomplete AES block");
    }
    let key: [u8; 16] = key.try_into().expect("16-byte key");
    let aes = Aes128::new(&Array::from(key));
    let mut prev: [u8; 16] = iv.try_into().expect("16-byte iv");
    for block in data.as_chunks_mut::<16>().0 {
        let cipher_text: [u8; 16] = *block;
        let mut b = Array::from(cipher_text);
        aes.decrypt_block(&mut b);
        for i in 0..16 {
            block[i] = b[i] ^ prev[i];
        }
        prev = cipher_text;
    }
    Ok(())
}

/// CHAM-128/128 (80 rounds, big-endian words) in CBC mode.
fn cham_cbc_decrypt(data: &mut [u8], key: &[u8], iv: &[u8]) -> Result<()> {
    if !data.len().is_multiple_of(16) {
        return invalid("incomplete CHAM block");
    }
    let mut keys = [0u32; 8];
    for i in 0..4 {
        let v = big_word(key, i * 4);
        keys[i] = v ^ v.rotate_left(1) ^ v.rotate_left(8);
        keys[(i + 4) ^ 1] = v ^ v.rotate_left(1) ^ v.rotate_left(11);
    }
    let mut prev: [u8; 16] = iv.try_into().expect("16-byte iv");
    for block in data.as_chunks_mut::<16>().0 {
        let enc: [u8; 16] = *block;
        let mut w = [0u32; 4];
        for (i, x) in w.iter_mut().enumerate() {
            *x = big_word(&enc, i * 4);
        }
        for round in (0..80u32).rev() {
            let i = (round % 4) as usize;
            let odd = round & 1 != 0;
            let next =
                w[(i + 1) % 4].rotate_left(if odd { 8 } else { 1 }) ^ keys[(round % 8) as usize];
            w[i] = w[i]
                .rotate_right(if odd { 1 } else { 8 })
                .wrapping_sub(next)
                ^ round;
        }
        for (i, x) in w.iter().enumerate() {
            put_big(block, i * 4, *x);
        }
        for i in 0..16 {
            block[i] ^= prev[i];
        }
        prev = enc;
    }
    Ok(())
}

fn three_way_theta(a: &mut u32, b: &mut u32, c: &mut u32) {
    let mix = (*a ^ *b ^ *c).rotate_left(16) ^ (*a ^ *b ^ *c).rotate_left(8);
    let u = (*a << 24) ^ (*c >> 8) ^ (*b << 8) ^ (*a >> 24);
    let v = (*b << 24) ^ (*a >> 8) ^ (*c << 8) ^ (*b >> 24);
    *a ^= mix ^ u;
    *b ^= mix ^ v;
    *c ^= mix ^ (u >> 16) ^ (v << 16);
}

/// ThreeWay (11 rounds), as in Crypto++'s public-domain implementation.
fn three_way_encrypt(key: &[u8; 12], block: &[u8; 12]) -> [u8; 12] {
    let (k0, k1, k2) = (big_word(key, 0), big_word(key, 4), big_word(key, 8));
    let (mut a, mut b, mut c) = (big_word(block, 0), big_word(block, 4), big_word(block, 8));
    let mut rc: u32 = 0x0b0b;
    for _ in 0..11 {
        a ^= k0 ^ (rc << 16);
        b ^= k1;
        c ^= k2 ^ rc;
        three_way_theta(&mut a, &mut b, &mut c);
        let (u, v) = (a.rotate_left(22), c.rotate_left(1));
        a = (u ^ (b | !v)).rotate_left(1);
        c = (v ^ (u | !b)).rotate_left(22);
        b ^= v | !u;
        rc <<= 1;
        if rc & 0x10000 != 0 {
            rc ^= 0x11011;
        }
    }
    a ^= k0 ^ (rc << 16);
    b ^= k1;
    c ^= k2 ^ rc;
    three_way_theta(&mut a, &mut b, &mut c);
    let mut out = [0u8; 12];
    put_big(&mut out, 0, a);
    put_big(&mut out, 4, b);
    put_big(&mut out, 8, c);
    out
}

fn three_way_cfb_decrypt(data: &mut [u8], key: &[u8; 12], iv: &[u8; 12]) -> Result<()> {
    if !data.len().is_multiple_of(12) {
        return invalid("incomplete ThreeWay block");
    }
    let mut prev = *iv;
    for block in data.as_chunks_mut::<12>().0 {
        let enc: [u8; 12] = *block;
        let stream = three_way_encrypt(key, &prev);
        for i in 0..12 {
            block[i] ^= stream[i];
        }
        prev = enc;
    }
    Ok(())
}

fn idea_mul(a: u16, b: u16) -> u16 {
    let l: u64 = if a == 0 { 65536 } else { u64::from(a) };
    let r: u64 = if b == 0 { 65536 } else { u64::from(b) };
    (l * r % 65537) as u16
}

fn idea_keys(key: &[u8; 16]) -> [u16; 52] {
    let mut value = *key;
    let mut keys = [0u16; 52];
    for (i, k) in keys.iter_mut().enumerate() {
        if i > 0 && i % 8 == 0 {
            // the 128-bit big-endian key rotated left by 25
            let mut rotated = [0u8; 16];
            for (j, r) in rotated.iter_mut().enumerate() {
                *r = (value[(j + 3) % 16] << 1) | (value[(j + 4) % 16] >> 7);
            }
            value = rotated;
        }
        let o = i % 8 * 2;
        *k = u16::from_be_bytes([value[o], value[o + 1]]);
    }
    keys
}

fn idea_encrypt(block: &[u8; 8], keys: &[u16; 52]) -> [u8; 8] {
    let rd = |i: usize| u16::from_be_bytes([block[i], block[i + 1]]);
    let (mut a, mut b, mut c, mut d) = (rd(0), rd(2), rd(4), rd(6));
    for i in (0..48).step_by(6) {
        a = idea_mul(a, keys[i]);
        b = b.wrapping_add(keys[i + 1]);
        c = c.wrapping_add(keys[i + 2]);
        d = idea_mul(d, keys[i + 3]);
        let e = idea_mul(a ^ c, keys[i + 4]);
        let f = idea_mul((b ^ d).wrapping_add(e), keys[i + 5]);
        let g = e.wrapping_add(f);
        let old_b = b;
        a ^= f;
        b = c ^ f;
        c = old_b ^ g;
        d ^= g;
    }
    let words = [
        idea_mul(a, keys[48]),
        c.wrapping_add(keys[49]),
        b.wrapping_add(keys[50]),
        idea_mul(d, keys[51]),
    ];
    let mut out = [0u8; 8];
    for (i, w) in words.iter().enumerate() {
        out[i * 2..i * 2 + 2].copy_from_slice(&w.to_be_bytes());
    }
    out
}

/// Two IDEA-CBC blocks of `input` under `key`: the 16-byte XOR mask of a body tail.
fn idea_mask(key: &[u8; 16], input: &[u8; 16], iv: &[u8]) -> [u8; 16] {
    let keys = idea_keys(key);
    let mut block = [0u8; 8];
    for i in 0..8 {
        block[i] = input[i] ^ iv[i];
    }
    let first = idea_encrypt(&block, &keys);
    for i in 0..8 {
        block[i] = input[8 + i] ^ first[i];
    }
    let second = idea_encrypt(&block, &keys);
    let mut mask = [0u8; 16];
    mask[..8].copy_from_slice(&first);
    mask[8..].copy_from_slice(&second);
    mask
}

fn rc6_keys(key: &[u8]) -> [u32; 44] {
    let mut words = [0u32; 4];
    for (n, w) in words.iter_mut().enumerate() {
        *w = word(key, n * 4);
    }
    let mut keys = [0u32; 44];
    for (n, k) in keys.iter_mut().enumerate() {
        *k = 0xb7e1_5163u32.wrapping_add((n as u32).wrapping_mul(0x9e37_79b9));
    }
    let (mut a, mut b) = (0u32, 0u32);
    let (mut i, mut j) = (0usize, 0usize);
    for _ in 0..132 {
        keys[i] = keys[i].wrapping_add(a).wrapping_add(b).rotate_left(3);
        a = keys[i];
        words[j] = words[j]
            .wrapping_add(a)
            .wrapping_add(b)
            .rotate_left(a.wrapping_add(b) & 31);
        b = words[j];
        i = (i + 1) % 44;
        j = (j + 1) % 4;
    }
    keys
}

fn rc6_encrypt(block: &[u8; 16], keys: &[u32; 44]) -> [u8; 16] {
    let mut a = word(block, 0);
    let mut b = word(block, 4).wrapping_add(keys[0]);
    let mut c = word(block, 8);
    let mut d = word(block, 12).wrapping_add(keys[1]);
    for i in 1..=20 {
        let t = b
            .wrapping_mul(b.wrapping_mul(2).wrapping_add(1))
            .rotate_left(5);
        let u = d
            .wrapping_mul(d.wrapping_mul(2).wrapping_add(1))
            .rotate_left(5);
        a = (a ^ t).rotate_left(u & 31).wrapping_add(keys[2 * i]);
        c = (c ^ u).rotate_left(t & 31).wrapping_add(keys[2 * i + 1]);
        let old_a = a;
        a = b;
        b = c;
        c = d;
        d = old_a;
    }
    let mut out = [0u8; 16];
    put_le(&mut out, 0, a.wrapping_add(keys[42]));
    put_le(&mut out, 4, b);
    put_le(&mut out, 8, c.wrapping_add(keys[43]));
    put_le(&mut out, 12, d);
    out
}

/// Body ciphers 128 (AES in a window) and 133 (RC6-CFB over the first KiB), then a 16-byte mask.
fn decrypt_body_tail(
    cipher: u16,
    params: &[u8; 64],
    name_hash: &[u8; 16],
    output_hash: &[u8; 16],
    data: &mut [u8],
    offset: usize,
) -> Result<()> {
    let mask = if cipher == 128 {
        idea_mask(output_hash, name_hash, &params[49..57])
    } else {
        idea_mask(name_hash, output_hash, &params[49..57])
    };
    let length = data.len() - offset;
    if cipher == 128 {
        let (start, size) = if length < 3072 {
            (0, length / 16 * 16)
        } else {
            (2048, 1024)
        };
        aes_cbc_decrypt(
            &mut data[offset + start..offset + start + size],
            &params[44..60],
            &params[28..44],
        )?;
    } else {
        let keys = rc6_keys(&params[24..40]);
        let mut prev: [u8; 16] = params[40..56].try_into().expect("16 bytes");
        let size = (length / 16 * 16).min(1024);
        for block in data[offset..offset + size].as_chunks_mut::<16>().0 {
            let enc: [u8; 16] = *block;
            let stream = rc6_encrypt(&prev, &keys);
            for i in 0..16 {
                block[i] ^= stream[i];
            }
            prev = enc;
        }
    }
    for (i, x) in data[offset..].iter_mut().enumerate() {
        *x ^= mask[i % 16];
    }
    Ok(())
}

/// Body ciphers 5 and 7: AES-CBC with PKCS#7 padding over at most one KiB, then a 16-byte mask over
/// the rest (which follows the padded block).
fn decrypt_legacy_tail(
    cipher: u16,
    params: &[u8; 64],
    name_hash: &[u8; 16],
    output_hash: &[u8; 16],
    data: &mut [u8],
    offset: usize,
    compressed_len: usize,
) -> Result<()> {
    let length = compressed_len.min(1024);
    let enc_len = (length / 16 + 1) * 16;
    let tail = data.len() - offset;
    if tail < enc_len || tail - enc_len != compressed_len - length {
        return invalid("AES body padding size mismatch");
    }
    let (key, iv, mask) = if cipher == 5 {
        (
            &params[0..16],
            &params[16..32],
            idea_mask(name_hash, output_hash, &params[48..56]),
        )
    } else {
        (
            &params[28..44],
            &params[44..60],
            idea_mask(output_hash, name_hash, &params[52..60]),
        )
    };
    aes_cbc_decrypt(&mut data[offset..offset + enc_len], key, iv)?;
    let padding = data[offset + enc_len - 1] as usize;
    if !(1..=16).contains(&padding)
        || enc_len - padding != length
        || data[offset + length..offset + enc_len]
            .iter()
            .any(|&b| b as usize != padding)
    {
        return invalid("invalid AES body PKCS#7 padding");
    }
    data.copy_within(offset + enc_len..offset + tail, offset + length);
    for i in 0..compressed_len - length {
        data[offset + length + i] ^= mask[i % 16];
    }
    Ok(())
}

/// Body cipher 1: CHAM-CBC over at most one KiB, then a chained XOR with a rotated name hash.
fn decrypt_cham_tail(
    params: &[u8; 64],
    seeded_name_hash: &[u8; 16],
    output_hash: &[u8; 16],
    data: &mut [u8],
    offset: usize,
) -> Result<()> {
    let seed = u64::from(word(seeded_name_hash, 8)) | u64::from(word(seeded_name_hash, 12)) << 32;
    let key = native_hash(&params[..60], seed);
    let length = data.len() - offset;
    let full = length / 16 * 16;
    cham_cbc_decrypt(
        &mut data[offset..offset + full.min(1024)],
        &key,
        output_hash,
    )?;
    let mut prev = *output_hash;
    for block in data[offset..offset + full].as_chunks_mut::<16>().0 {
        let original: [u8; 16] = *block;
        for i in 0..16 {
            let r = (i + 12) % 16;
            block[i] = original[r] ^ seeded_name_hash[r] ^ prev[i];
        }
        prev = original;
    }
    for i in 0..length - full {
        data[offset + full + i] ^= seeded_name_hash[i] ^ prev[i];
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) struct Vector {
        pub tag: String,
        pub name: Vec<u8>,
        pub keys: Vec<u8>,
        pub payload: Vec<u8>,
        pub output: Vec<u8>,
        pub size: usize,
    }

    /// GRF Editor's vector bundle (`Tests/fixtures/protected-grf.bin`): generated bytes only, no
    /// client files. A record count, then per record five length-prefixed fields and the size.
    pub(crate) fn read_vectors() -> Vec<Vector> {
        let d: &[u8] = include_bytes!("../tests/fixtures/protected-grf.bin");
        let int = |p: &mut usize| {
            let v = u32::from_le_bytes(d[*p..*p + 4].try_into().unwrap()) as usize;
            *p += 4;
            v
        };
        let bytes = |p: &mut usize| {
            let n = int(p);
            let b = d[*p..*p + n].to_vec();
            *p += n;
            b
        };
        let mut p = 0;
        let count = int(&mut p);
        let mut out = Vec::new();
        for _ in 0..count {
            out.push(Vector {
                tag: String::from_utf8(bytes(&mut p)).unwrap(),
                name: bytes(&mut p),
                keys: bytes(&mut p),
                payload: bytes(&mut p),
                output: bytes(&mut p),
                size: int(&mut p),
            });
        }
        assert_eq!(
            (out.len(), p),
            (40, d.len()),
            "fixture bundle is incomplete"
        );
        out
    }

    /// Same classification as GRF Editor's `CheckDecoder`: 25 vectors decode to their output, the
    /// 15 malformed ones are rejected as invalid data.
    #[test]
    fn every_vector_of_grf_editor_decodes_or_is_rejected_like_there() {
        let (mut accepted, mut rejected) = (0, 0);
        for v in read_vectors() {
            let positive = v.tag.starts_with("boundary-")
                || [
                    "raw-name",
                    "zlib-rc6-footer",
                    "lz4-aes-checksum",
                    "threeway-partial",
                    "body1-lz4",
                ]
                .contains(&v.tag.as_str());
            let got = decode_archive_keys(&v.keys).and_then(|keys| {
                if v.payload.is_empty() {
                    Ok(Vec::new())
                } else {
                    decode(&v.payload, &v.name, v.size, &keys)
                }
            });
            match got {
                Ok(out) => {
                    assert!(positive, "malformed vector accepted: {}", v.tag);
                    assert_eq!(out, v.output, "{}", v.tag);
                    accepted += 1;
                }
                Err(e) => {
                    assert!(!positive, "{}: {e}", v.tag);
                    assert!(matches!(e, GravityError::Invalid(_)), "{}: {e:?}", v.tag);
                    rejected += 1;
                }
            }
        }
        assert_eq!((accepted, rejected), (25, 15));
    }

    #[test]
    fn a_changed_name_or_size_does_not_decode() {
        let v = read_vectors()
            .into_iter()
            .find(|v| v.tag == "zlib-rc6-footer")
            .unwrap();
        let keys = decode_archive_keys(&v.keys).unwrap();
        assert!(!keys.is_empty());
        let mut name = v.name.clone();
        name[0] ^= 1;
        assert!(decode(&v.payload, &name, v.size, &keys).is_err());
        assert!(decode(&v.payload, &v.name, v.size + 1, &keys).is_err());
        assert!(decode(&v.payload, &v.name, v.size, &ArchiveKeys::new()).is_err());
    }
}
