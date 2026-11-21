//! Gravity's protection of single entries (flag `0x80`, GRF 0x300) and the archive key block
//! behind the file table.
//!
//! Ported from GRF Editor by Tokeiburu (`GRF/Core/GrfCompression/ProtectedGrfDecoder.cs`,
//! `ProtectedGrfCiphers.cs`, `ProtectedJroDecoder.cs` and `JroArchiveProfiles.cs`, with
//! permission, see `docs/05-licensing.md`). Ported: the kRO variant (entry headers of type 1, 2, 15
//! and 113, body ciphers 1, 5, 7, 128 and 133, key blocks encrypted with CHAM or holding the kRO
//! default keys) and the jRO variant (header 241 with IDEA, bodies 259 XTEA-CFB, 260 TEA-CFB and
//! 269 RC5-OFB, key blocks GRF Editor recognises by their SHA-256); data stored, zlib or LZ4.
//! The Chinese variant (`ProtectedCnDecoder.cs`: entry header version 2 with a signature, body
//! cipher 9 with RC6 and an IDEA mask, one of two key blocks GRF Editor knows by their SHA-256) is
//! ported from code that GRF Editor itself cannot run: its entry header reader refuses the header
//! version 2 and its 64-byte keys fail the 60-byte check. Here the first 60 bytes of a key are
//! used and the Ed25519 signature of the header is not checked; the hash of the decoded content
//! still is, so a wrong reading is an error, never wrong data. **No real cnRO archive was
//! available:** only a round trip with an encoder written from the same code has been tried.
//! Checked against the 40 vectors GRF Editor ships and 24 jRO vectors made with GRF Editor's own
//! decoder (`tests/fixtures`).
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
/// An entry header of version 2 (Chinese variant) is followed by a 64-byte signature.
const CN_HEADER_SIZE: usize = 168;
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
    if let Some(keys) = jro_archive_keys(raw)? {
        return Ok(keys);
    }
    if let Some(keys) = cn_archive_keys(raw)? {
        return Ok(keys);
    }
    let size = word(raw, 12) as usize;
    if size == 0 || !size.is_multiple_of(64) || size / 64 > 65535 || size != raw.len() - 20 {
        return Err(GravityError::Unsupported(
            "archive key block of an unknown layout".into(),
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
    let cn = header[0] == 2;
    let header_size = if cn { CN_HEADER_SIZE } else { HEADER_SIZE };
    if payload.len() < header_size {
        return invalid("truncated entry header");
    }
    let padded = word(&header, 12) as usize;
    let compressed = word(&header, 16) as usize;
    let output = word(&header, 20) as usize;
    if output != expected || output > MAX_OUTPUT {
        return invalid("output size differs from the file table");
    }
    if padded > MAX_PACKED || padded > payload.len() - header_size || compressed > padded {
        return invalid("body size lies outside the stored data");
    }
    let cipher = u16::from_le_bytes([header[8], header[9]]);
    if !(matches!(cipher, 1 | 5 | 7 | 128 | 133 | 259 | 260 | 269) || (cn && cipher == 9)) {
        return invalid(format!("unsupported body cipher {cipher}"));
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
    let mut body = payload[header_size..header_size + padded].to_vec();
    if cn && cipher == 9 {
        let material: &[u8; 60] = params[..60].try_into().expect("60 bytes");
        body = cn_body(body, &header, raw_name, material);
    } else if matches!(cipher, 259 | 260 | 269) {
        let material: &[u8; 60] = params[..60].try_into().expect("60 bytes");
        body = jro_body(body, &header, raw_name, cipher, material)?;
    } else if compressed > 128 {
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
    if payload.len() < HEADER_SIZE || !matches!(payload[0], 1 | 2) {
        return invalid("unsupported or truncated entry header");
    }
    if payload[0] == 2 && !matches!(payload[1], 1 | 2) {
        return invalid("unsupported signed entry header cipher");
    }
    if payload[1] == 241 {
        return jro_header(payload);
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

// ---------- the Chinese variant ----------

/// The two key blocks (156 bytes) GRF Editor knows, by their SHA-256.
const CN_ENVELOPES: [&str; 2] = [
    "a09a4c27abdfbb473392e1e246b8def486ff820bdd51de7ad7206c279cc3b529",
    "2ef7f6c8fb08fa9d90b163de26c04a7323ec9231f59efcb7c783f1dee040053a",
];

/// The keys such a block stands for: two records of an id and 64 bytes.
const CN_KEYS: [u8; 136] = [
    0x01, 0x00, 0x00, 0x00, 0xbc, 0x66, 0xd8, 0x90, 0x8f, 0xa7, 0x19, 0x7b, 0x0f, 0x90, 0x22, 0xed,
    0x42, 0x6d, 0xdf, 0x1f, 0xdf, 0xcf, 0xef, 0xca, 0x44, 0xd0, 0xd6, 0xc9, 0xde, 0xc0, 0x66, 0x0f,
    0xa6, 0x01, 0x0e, 0x05, 0x72, 0x2a, 0x70, 0x8d, 0xc7, 0x1e, 0xf8, 0x67, 0xfb, 0x59, 0x64, 0x5d,
    0x47, 0x7c, 0xc4, 0x74, 0x8a, 0xf3, 0xa0, 0x5e, 0xcf, 0xbf, 0x42, 0x64, 0x6b, 0x04, 0xe9, 0x9c,
    0xe3, 0x9d, 0x21, 0xa7, 0x02, 0x00, 0x00, 0x00, 0xbd, 0x2a, 0x05, 0xe0, 0x5b, 0x87, 0x25, 0x5a,
    0x4e, 0x9d, 0x3f, 0xde, 0x38, 0x69, 0xa2, 0xb6, 0x45, 0x8f, 0xf7, 0x49, 0x12, 0x20, 0x9c, 0xf9,
    0x62, 0x23, 0x3e, 0x60, 0x44, 0x55, 0x4f, 0x6c, 0x34, 0x1c, 0xaf, 0x34, 0x97, 0x9c, 0x29, 0xa6,
    0x64, 0xd4, 0x2e, 0x5e, 0xeb, 0xa3, 0x56, 0x74, 0xa6, 0xe3, 0x19, 0x3e, 0x5e, 0xce, 0x9a, 0x7d,
    0xe1, 0x1b, 0x3d, 0x59, 0xe8, 0xd7, 0xe8, 0x78,
];

/// The keys of a key block that is one of the two known (the keys are not in the block itself).
fn cn_archive_keys(raw: &[u8]) -> Result<Option<ArchiveKeys>> {
    if raw.len() != 156 {
        return Ok(None);
    }
    let hash: String = Sha256::digest(raw)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if !CN_ENVELOPES.contains(&hash.as_str()) {
        return Ok(None);
    }
    if word(raw, 0) != 258 || word(raw, 12) != 136 {
        return invalid("invalid CN archive key block");
    }
    if crc32fast::hash(&CN_KEYS) != word(raw, 16) {
        return invalid("CN archive key checksum mismatch");
    }
    let mut keys = ArchiveKeys::new();
    for rec in CN_KEYS.as_chunks::<68>().0 {
        let id = word(rec, 0);
        if id == 0 || id > 65535 || keys.contains_key(&id) {
            return invalid("invalid CN key id");
        }
        // the records hold 64 bytes, a key is used with 60
        keys.insert(id, rec[4..64].try_into().expect("60 bytes"));
    }
    Ok(Some(keys))
}

/// Body cipher 9: RC6 with feedback over a window (the first KiB behind 1152 bytes of the body,
/// else everything from byte 128 in whole blocks), then a 16-byte IDEA mask over all from byte 128.
fn cn_body(
    mut body: Vec<u8>,
    header: &[u8; HEADER_SIZE],
    name: &[u8],
    material: &[u8; 60],
) -> Vec<u8> {
    if body.len() <= 128 {
        return body;
    }
    let rest = body.len() - 128;
    let (at, n) = if rest >= 2048 {
        (1152, 1024)
    } else {
        (128, rest & !15)
    };
    let keys = rc6_keys(&material[16..32]);
    let mut feedback: [u8; 16] = material[44..60].try_into().expect("16 bytes");
    for off in (at..at + n).step_by(16) {
        let stream = rc6_encrypt(&feedback, &keys);
        let cipher: [u8; 16] = body[off..off + 16].try_into().expect("16 bytes");
        for i in 0..16 {
            body[off + i] = cipher[i] ^ stream[i];
        }
        feedback = cipher;
    }
    let name_hash = native_hash(name, 0);
    let header_hash: [u8; 16] = header[24..40].try_into().expect("16 bytes");
    let mask = idea_mask(&name_hash, &header_hash, &material[50..58]);
    xor_mask(&mut body[128..], &mask);
    body
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

// ---- jRO variant (`ProtectedJroDecoder.cs`, `JroArchiveProfiles.cs`) ----

/// SHA-256 of the archive key blocks of jRO archives that GRF Editor knows; only those stand for
/// the keys below.
const JRO_PROFILES: [&str; 59] = [
    "8a9a46ee197b2b2d25477e3e2ce26bd29743708876da93af2c925cc1c068234c",
    "339dd2684dba8b0891ca9a844b83b058f289306538218ae95c26c557ceaceb1d",
    "ffa05695215712a114f42d4f0fef6d86637fe17bcc19b45ab569cfd0585c2a64",
    "f380962ffbc567c042cf184fb78f0f5ea2b1cc1c3a97af3e1c59d9102a5253b8",
    "8f61c45431931d665fe3d3b684e153e01d7479da260929a473a566313d766431",
    "207712f8feba4c8caa80f890091b955223d4e614397f5bd4de3ad5e32959b232",
    "cba6060a8286e2c8a9149c80b4bb50af48b965ee651c520c6389f85aa0dbb1d4",
    "e5cc90f1c7374f3db9156ea0f03ef52a90022c437574be11aed3c0f8dd8a4c5a",
    "7e53096d2ec7bea38d431d2f86a5a59afef23b067a3f0e082278f52de227c366",
    "befe0beed6ee15228e49a13d0138efb39c21ebce43a38512a934030107b718ef",
    "41efcc11a18f89d2db2a730342f7abda60eb1bfc8b3812421c965c7178fca06f",
    "2b6983d7259731973b24589b7f6ecd424001600ded5f77a5542b9efc99dfe264",
    "34d6e536d7ed6fd43fdd61c8650a8713c6fb0846255a668fe2f565d94b3ab48a",
    "df3e6c9ee8d40c20edd5d73b865b2ef2bd82263c173485a85b70d61644e555a8",
    "528789bb71559699d22c8d54ca46495e50191dbaab6ccb5c69525c01b92e7d2e",
    "702e03f4e962b405435b84b61bfbbf9cc78b7c78f7014080f1e997bc38b7c1a4",
    "a9bf5983797858d1b4f4fe2b7f2a845221a37336e739407d7fbc1feaf21ae216",
    "a0799230b1112580c02908dcac0365cabdf6fc8447a34850356a8f76050504b9",
    "b9cde4e50366af90f959c93aa5938cea69115fe3d99d1b6115c570273516758d",
    "5d7142c159af8b49f8014c413ca0caecc39de5c6369db69118fa467e96edf4bf",
    "d68f249a940230b44ae30ca5938718ad31200debbbb5db2460a8d9dd817ccc01",
    "f4267e762541a2fb09028b2f1514f8a18fd825291817f1752069ab8dabb5751b",
    "0c6fe00a49d9d3eb3ab85295bf011c7214d71915bad029022818594fec64e2c1",
    "3648ddcc2865bc2ef8f04f2f04bf5e8cbd2d1a2b4eb9bd9816a790dc12801f90",
    "cc636f8084295b2272e7cc10771b4fcf3e3b1889bb8b22a4d2c2a18b4d197575",
    "b66feec0b438bfff4e5d88c2aeedfe8d6df57aa6961cdca162278fc12ecc2d27",
    "4afcf4bdaf9ce3e44d70b56e6c4018e4c517c966bfc1c2b5e69c21bb0ff1becf",
    "438cc2e29834ce62352170eb48e2a688820d7c142d80ff838c008ad0851c004b",
    "54ad6e64af2818cc7dfe885dcbd06a4285eb4121cc004dcaae3277f35932cef9",
    "bf3035c4594a516ff6376c3dbf33a0ab50cbf787b89eea35e0062c3d7ac877ec",
    "08c9f274a2389e8610f6534bbfd22f4be447328d29fae870be5fee87dd2f6ab2",
    "2da9e6d108ef153873c8da82db7b985aa7255a86d72ba24d99af3b92258357a7",
    "1af0c37bb2efbc7ed19f08ecf6f29156476e95a144d08585ef45cb865c78859c",
    "f4294137e2edd6cd24a985a854adbaaaad831a503f1c391b529b2750ae928014",
    "0a66e9f99572586fc854330285bbaf7d1eeb9ebf81fa3954bd12b40ab0514aac",
    "4df44e7ee74829a3725654a4eda66734572f12620202ada24c3e9d10cc762015",
    "d071e72579ab6af3ad5decb99fb4bb3f8ccb3fe6fd66c60a1b02421dfbe3f932",
    "ba08f3a06cd970ddc2c654f15b0f559783ae346c1ae71a15bc7ac246e15177fa",
    "ac5760d4fde02601d40b9cf46e8deabf15e3e29f520f5719436ee8362e958941",
    "f14d9b94576b35ffc67367eaf951112f803233c930267a1d5faf5496ce0ca431",
    "9007481ede0bfab52ab2a5611e87fd643fd57aab3244ef84f388342c9ba669fb",
    "3dead688f25e7ca8e51c67f073bf496fcb91d94967155f7be163bd05730706b1",
    "ffcdb9f6e4c285fe16150e34e7989d7cb79f2f4a4972907d70f649ba42fb04f2",
    "c52c6aa91e6175aaaa83894a294e9563566d7d3427950848a97c0c7fd2eff420",
    "4f2c9f0e256c7de35a6a2626766076788078d1f0815977b4599e1d34d664a31c",
    "dfa1b839b18f8ae5cfd83c6a0f02cd93d68044818fbe8b758a6b405c696e50d7",
    "455d37dab1b57e13dd8229589c5bc67780fbf47b7e803b21689d2ed05d9f2591",
    "df175a48d280e64d899f7827345916991db3635611f86d1dc92153e5f4107b14",
    "f84df52839b92e206e6e302512ec9031cc0238b76b29c165cd81b070cbc0820f",
    "5a89c68aedc3e0ff887d95df96ad14c740fb2c8394aa42e99e5552707549a3fe",
    "0cbbcac4090506c9bfb3389b7ea8cee67b553cd7b06bb12e7255fff14d521fc3",
    "456a4e52b30965ab45e7a13670dadbb81f3219ec6d1c295002ebe480d7af6047",
    "b8db7ff719440061fa40b3471711d324e337cd9076d6fcd5436f6308143e8aa0",
    "39cac3868800c4fa1308a5c6ee41787b18b8ec503b07fad22e72bfac407bc80f",
    "8eb62476abe9859bcebea5b101ed61de89e0c6d67bdb310b1b6a1851293adfa3",
    "ad804545419047cfc57c77d9c7af9c8cfe7b93c79c98c442eda883700b35fa60",
    "38295a7239c790215ceb53601685e6379484da94879b2db91e0f5f2f31c34e65",
    "345ab8280c5e3f62e61450f94b085d015241d9fd145f0ab0732b480d30413087",
    "34c3ac066164d6f96bd88e9c3ce841e45791113b86896ffeb8b634ed9ab28e7b",
];

/// The jRO archive keys as GRF Editor embeds them: three 64-byte records (`u16` id, two unused
/// bytes, 60 bytes of material) with the ids 3, 1 and 2.
const JRO_KEY_BLOCK: [u8; 192] = [
    0x03, 0x00, 0x01, 0x00, 0xe4, 0xe6, 0x9f, 0xe3, 0x9b, 0x8c, 0xd0, 0x79, 0xaa, 0xe0, 0x2e, 0x7c,
    0x8f, 0xfc, 0xf2, 0xf4, 0xa8, 0xcc, 0xfa, 0xb1, 0xa9, 0x4b, 0xd2, 0xca, 0xae, 0x5f, 0x74, 0x7c,
    0x94, 0x5f, 0x8f, 0x80, 0xab, 0xe1, 0xe2, 0xf8, 0xfe, 0xb4, 0x3e, 0x22, 0xdf, 0xe8, 0x96, 0x58,
    0x03, 0x89, 0x50, 0x32, 0xd4, 0x76, 0x9a, 0x35, 0x51, 0x74, 0x19, 0xed, 0xff, 0x0b, 0xfa, 0x97,
    0x01, 0x00, 0x00, 0x00, 0xfb, 0x04, 0x3c, 0x63, 0x2c, 0x60, 0xa9, 0x33, 0xe4, 0x00, 0x0d, 0xf9,
    0x4d, 0xd7, 0x72, 0xf4, 0xc6, 0xe7, 0x89, 0xe0, 0xb3, 0x20, 0x77, 0xb0, 0x01, 0x5e, 0x38, 0x12,
    0x3d, 0x19, 0xe9, 0x63, 0xb1, 0xd3, 0x05, 0x78, 0x74, 0x99, 0x0d, 0x62, 0x6a, 0x58, 0x4b, 0xe0,
    0xa9, 0x90, 0xcb, 0x8f, 0x1c, 0x43, 0x6f, 0x53, 0xc3, 0xc1, 0xc8, 0x8f, 0xb9, 0x07, 0x3b, 0xd1,
    0x02, 0x00, 0x00, 0x00, 0x4f, 0x7a, 0xef, 0xb5, 0xbb, 0xae, 0x61, 0x67, 0x52, 0x96, 0xaa, 0x46,
    0x6d, 0x2f, 0x5d, 0x7c, 0xfb, 0x6a, 0x79, 0xcf, 0xee, 0x46, 0x04, 0x81, 0xae, 0xb2, 0x30, 0x67,
    0x87, 0x4a, 0x94, 0x8b, 0x6f, 0x36, 0xb7, 0xcc, 0x76, 0x20, 0xe2, 0x0f, 0xc3, 0x96, 0x0e, 0xfd,
    0x29, 0x8b, 0x3e, 0xca, 0xba, 0x0b, 0x8f, 0xe7, 0x62, 0x6d, 0xd2, 0xb2, 0x93, 0xdc, 0xee, 0x17,
];

/// IDEA key and CBC start value of the entry header type 241.
const JRO_HEADER_KEY: [u8; 16] = [
    0x56, 0xfd, 0x7e, 0x71, 0x2f, 0x6d, 0x36, 0xf5, 0x5c, 0xa5, 0x99, 0xe6, 0x7b, 0x18, 0x10, 0x7a,
];
const JRO_HEADER_IV: [u8; 8] = [0x4c, 0xe8, 0xa1, 0xae, 0x45, 0xb3, 0x21, 0x60];

const TEA_DELTA: u32 = 0x9e37_79b9;

/// The keys of a jRO archive key block, `None` when the block is not one GRF Editor knows.
fn jro_archive_keys(raw: &[u8]) -> Result<Option<ArchiveKeys>> {
    if !matches!(raw.len(), 84 | 148 | 212)
        || !matches!(word(raw, 0), 1 | 257)
        || word(raw, 12) as usize != raw.len() - 20
    {
        return Ok(None);
    }
    let digest = Sha256::digest(raw);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if !JRO_PROFILES.contains(&hex.as_str()) {
        return Ok(None);
    }
    let records: Vec<u8> = match word(raw, 12) {
        64 => JRO_KEY_BLOCK[64..128].to_vec(),
        128 => [&JRO_KEY_BLOCK[128..192], &JRO_KEY_BLOCK[64..128]].concat(),
        _ => JRO_KEY_BLOCK.to_vec(),
    };
    if xxh32(&records) != word(raw, 16) {
        return invalid("jRO archive key checksum mismatch");
    }
    let mut keys = ArchiveKeys::new();
    for rec in records.as_chunks::<64>().0 {
        let id = u32::from(u16::from_le_bytes([rec[0], rec[1]]));
        if id == 0 || keys.contains_key(&id) {
            return invalid("invalid jRO key id");
        }
        keys.insert(id, rec[4..64].try_into().expect("60 bytes"));
    }
    Ok(Some(keys))
}

/// Entry header type 241: bytes 8 to 104 IDEA-CBC encrypted, checksum as for the others.
fn jro_header(payload: &[u8]) -> Result<[u8; HEADER_SIZE]> {
    let mut header = [0u8; HEADER_SIZE];
    header.copy_from_slice(&payload[..HEADER_SIZE]);
    let keys = idea_decrypt_keys(&idea_keys(&JRO_HEADER_KEY));
    let mut prev = JRO_HEADER_IV;
    for block in header[8..104].as_chunks_mut::<8>().0 {
        let cipher = *block;
        let plain = idea_encrypt(&cipher, &keys);
        for i in 0..8 {
            block[i] = plain[i] ^ prev[i];
        }
        prev = cipher;
    }
    if xxh32(&header[8..104]) != word(&header, 4) {
        return invalid("entry header checksum mismatch");
    }
    Ok(header)
}

/// Multiplicative inverse modulo 65537, with 0 standing for 65536 (IDEA's convention).
fn idea_mul_inv(x: u16) -> u16 {
    if x <= 1 {
        return x;
    }
    // 65537 is prime: x^(65537 - 2)
    let (mut base, mut exp, mut acc) = (u64::from(x), 65535u32, 1u64);
    while exp > 0 {
        if exp & 1 == 1 {
            acc = acc * base % 65537;
        }
        base = base * base % 65537;
        exp >>= 1;
    }
    acc as u16
}

/// The decryption subkeys of IDEA from the encryption subkeys; [`idea_encrypt`] with them decrypts.
fn idea_decrypt_keys(ek: &[u16; 52]) -> [u16; 52] {
    let mut dk = [0u16; 52];
    for r in 0..9 {
        let e = 48 - 6 * r;
        let (b, c) = if r == 0 || r == 8 { (1, 2) } else { (2, 1) };
        dk[6 * r] = idea_mul_inv(ek[e]);
        dk[6 * r + 1] = ek[e + b].wrapping_neg();
        dk[6 * r + 2] = ek[e + c].wrapping_neg();
        dk[6 * r + 3] = idea_mul_inv(ek[e + 3]);
        if r < 8 {
            dk[6 * r + 4] = ek[e - 2];
            dk[6 * r + 5] = ek[e - 1];
        }
    }
    dk
}

/// GRF Editor's TEA and XTEA with 16 cycles (not the usual 32), big-endian words.
fn tea_encrypt(key: &[u32; 4], block: &[u8; 8], xtea: bool) -> [u8; 8] {
    let (mut l, mut r) = (big_word(block, 0), big_word(block, 4));
    let mut sum = 0u32;
    for _ in 0..16 {
        if xtea {
            l = l.wrapping_add(
                ((r << 4) ^ (r >> 5)).wrapping_add(r) ^ sum.wrapping_add(key[(sum & 3) as usize]),
            );
            sum = sum.wrapping_add(TEA_DELTA);
            r = r.wrapping_add(
                ((l << 4) ^ (l >> 5)).wrapping_add(l)
                    ^ sum.wrapping_add(key[((sum >> 11) & 3) as usize]),
            );
        } else {
            sum = sum.wrapping_add(TEA_DELTA);
            l = l.wrapping_add(
                (r << 4).wrapping_add(key[0]) ^ r.wrapping_add(sum) ^ (r >> 5).wrapping_add(key[1]),
            );
            r = r.wrapping_add(
                (l << 4).wrapping_add(key[2]) ^ l.wrapping_add(sum) ^ (l >> 5).wrapping_add(key[3]),
            );
        }
    }
    let mut out = [0u8; 8];
    put_big(&mut out, 0, l);
    put_big(&mut out, 4, r);
    out
}

/// CFB decryption with (X)TEA over `data` (whole 8-byte blocks).
fn tea_cfb_decrypt(data: &mut [u8], material: &[u8; 60], key_at: usize, iv_at: usize, xtea: bool) {
    let key = [0, 1, 2, 3].map(|i| big_word(material, key_at + i * 4));
    let mut feedback: [u8; 8] = material[iv_at..iv_at + 8].try_into().expect("8 bytes");
    for block in data.as_chunks_mut::<8>().0 {
        let cipher = *block;
        let stream = tea_encrypt(&key, &feedback, xtea);
        for i in 0..8 {
            block[i] ^= stream[i];
        }
        feedback = cipher;
    }
}

/// RC5-32 with 20 rounds in OFB mode: key bytes 16..32 of the material, start value bytes 0..8.
fn rc5_ofb(data: &mut [u8], material: &[u8; 60]) {
    let mut s = [0u32; 42];
    s[0] = 0xb7e1_5163;
    for i in 1..42 {
        s[i] = s[i - 1].wrapping_add(TEA_DELTA);
    }
    let mut l = [0, 1, 2, 3].map(|i| word(material, 16 + i * 4));
    let (mut a, mut b) = (0u32, 0u32);
    for k in 0..126 {
        let (i, j) = (k % 42, k % 4);
        a = s[i].wrapping_add(a).wrapping_add(b).rotate_left(3);
        s[i] = a;
        b = l[j]
            .wrapping_add(a)
            .wrapping_add(b)
            .rotate_left(a.wrapping_add(b));
        l[j] = b;
    }
    let mut feedback: [u8; 8] = material[..8].try_into().expect("8 bytes");
    for block in data.as_chunks_mut::<8>().0 {
        let mut x = word(&feedback, 0).wrapping_add(s[0]);
        let mut y = word(&feedback, 4).wrapping_add(s[1]);
        for r in 1..=20 {
            x = (x ^ y).rotate_left(y).wrapping_add(s[2 * r]);
            y = (y ^ x).rotate_left(x).wrapping_add(s[2 * r + 1]);
        }
        put_le(&mut feedback, 0, x);
        put_le(&mut feedback, 4, y);
        for i in 0..8 {
            block[i] ^= feedback[i];
        }
    }
}

/// The 16-byte mask of a jRO body: IDEA-CBC of one hash under the other, start value from the
/// material.
fn jro_mask(
    material: &[u8; 60],
    header: &[u8],
    name: &[u8],
    iv_at: usize,
    checksum_as_key: bool,
) -> [u8; 16] {
    let name_hash = native_hash(name, 0);
    let header_hash: [u8; 16] = header[24..40].try_into().expect("16 bytes");
    let iv = &material[iv_at..iv_at + 8];
    if checksum_as_key {
        idea_mask(&header_hash, &name_hash, iv)
    } else {
        idea_mask(&name_hash, &header_hash, iv)
    }
}

fn xor_mask(data: &mut [u8], mask: &[u8; 16]) {
    for (i, x) in data.iter_mut().enumerate() {
        *x ^= mask[i & 15];
    }
}

/// Body ciphers 259, 260 and 269 of the jRO variant; the first 128 bytes stay plain.
fn jro_body(
    mut body: Vec<u8>,
    header: &[u8; HEADER_SIZE],
    name: &[u8],
    cipher: u16,
    material: &[u8; 60],
) -> Result<Vec<u8>> {
    if body.len() <= 128 {
        return Ok(body);
    }
    match cipher {
        259 => {
            let n = ((body.len() - 128) & !7).min(1024);
            tea_cfb_decrypt(&mut body[128..128 + n], material, 0, 32, true);
            xor_mask(
                &mut body[128..],
                &jro_mask(material, header, name, 50, true),
            );
            Ok(body)
        }
        260 => {
            let rest = body.len() - 128;
            let (at, n) = if rest >= 3072 {
                (2176, 1024)
            } else {
                (128, rest & !7)
            };
            tea_cfb_decrypt(&mut body[at..at + n], material, 32, 0, false);
            xor_mask(
                &mut body[128..],
                &jro_mask(material, header, name, 48, false),
            );
            Ok(body)
        }
        _ => {
            let decoded = word(header, 16) as usize;
            let mask = jro_mask(material, header, name, 50, false);
            if decoded <= 2176 {
                if decoded != body.len() {
                    return invalid("body 269 length mismatch");
                }
                xor_mask(&mut body[128..], &mask);
                return Ok(body);
            }
            let n = (decoded - 2176).min(1024);
            let pad = 8 - (n & 7);
            if body.len() != decoded + pad || body.len() < 2176 + n + pad {
                return invalid("body 269 padded length mismatch");
            }
            let mut window = body[2176..2176 + n + pad].to_vec();
            rc5_ofb(&mut window, material);
            if window[n..].iter().any(|&b| b as usize != pad) {
                return invalid("body 269 PKCS#7 padding mismatch");
            }
            let mut out = Vec::with_capacity(decoded);
            out.extend_from_slice(&body[..2176]);
            xor_mask(&mut out[128..2176], &mask);
            out.extend_from_slice(&window[..n]);
            let tail = out.len();
            out.extend_from_slice(&body[2176 + n + pad..]);
            xor_mask(&mut out[tail..], &mask);
            Ok(out)
        }
    }
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
        read_bundle(include_bytes!("../tests/fixtures/protected-grf.bin"), 40)
    }

    fn read_bundle(d: &[u8], expected: usize) -> Vec<Vector> {
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
            (expected, d.len()),
            "fixture bundle is incomplete"
        );
        out
    }

    /// jRO entries (header 241, bodies 259, 260 and 269; stored, zlib and LZ4; 100 bytes to 9 KB,
    /// with and without the material flag) made by `tools/jro-vectors` from GRF Editor's own
    /// decoder sources and checked there with `ProtectedGrfDecoder.Decode`: 24 decode, 5 are
    /// rejected (header checksum, output hash, 269 length and padding, unknown cipher).
    #[test]
    fn jro_vectors_decode_or_are_rejected_like_in_grf_editor() {
        let vectors = read_bundle(
            include_bytes!("../tests/fixtures/protected-grf-jro.bin"),
            29,
        );
        let keys = ArchiveKeys::new();
        let (mut accepted, mut rejected) = (0, 0);
        for v in vectors {
            match decode(&v.payload, &v.name, v.size, &keys) {
                Ok(out) => {
                    assert!(
                        v.tag.starts_with("jro-ok-"),
                        "malformed vector accepted: {}",
                        v.tag
                    );
                    assert_eq!(out, v.output, "{}", v.tag);
                    accepted += 1;
                }
                Err(e) => {
                    assert!(v.tag.starts_with("jro-bad-"), "{}: {e}", v.tag);
                    assert!(matches!(e, GravityError::Invalid(_)), "{}: {e:?}", v.tag);
                    rejected += 1;
                }
            }
        }
        assert_eq!((accepted, rejected), (24, 5));
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
    fn idea_decryption_undoes_encryption() {
        let mut x = 0x1234_5678u32;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as u8
        };
        for _ in 0..200 {
            let key: [u8; 16] = std::array::from_fn(|_| next());
            let block: [u8; 8] = std::array::from_fn(|_| next());
            let ek = idea_keys(&key);
            let back = idea_encrypt(&idea_encrypt(&block, &ek), &idea_decrypt_keys(&ek));
            assert_eq!(back, block);
        }
        assert_eq!(idea_mul(idea_mul_inv(0), 0), 1);
        assert_eq!(idea_mul(idea_mul_inv(12345), 12345), 1);
    }

    #[test]
    fn the_embedded_jro_keys_have_the_ids_grf_editor_reads() {
        let ids: Vec<u16> = JRO_KEY_BLOCK
            .as_chunks::<64>()
            .0
            .iter()
            .map(|r| u16::from_le_bytes([r[0], r[1]]))
            .collect();
        assert_eq!(ids, [3, 1, 2]);
        // a block of the right shape but unknown content is not taken for jRO
        let mut raw = vec![0u8; 84];
        raw[0] = 1;
        raw[12] = 64;
        assert_eq!(jro_archive_keys(&raw), Ok(None));
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
    /// An entry of the Chinese variant made by an encoder written from the decoder's code: the
    /// round trip shows the plumbing (header version 2, 168-byte header, the two body windows,
    /// the mask), not that real cnRO archives read.
    fn cn_entry(data: &[u8], name: &[u8], header_cipher: u8) -> Vec<u8> {
        use aes::cipher::BlockCipherEncrypt;
        let mut seed = 0x1234_5678u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        };
        let material: [u8; 60] = std::array::from_fn(|_| next());
        let compressed = crate::codec::tests_support::zlib_level(data, 1);
        let padded = compressed.len().div_ceil(16) * 16;
        let mut body = compressed.clone();
        body.resize(padded, 0);
        let out_hash = native_hash(data, 0);
        let mut header = [0u8; HEADER_SIZE];
        header[0] = 2;
        header[1] = header_cipher;
        header[8..10].copy_from_slice(&9u16.to_le_bytes());
        header[10] = 1;
        put_le(&mut header, 12, padded as u32);
        put_le(&mut header, 16, compressed.len() as u32);
        put_le(&mut header, 20, data.len() as u32);
        header[24..40].copy_from_slice(&out_hash);
        header[44..104].copy_from_slice(&material);
        let sum = xxh32(&header[8..104]);
        put_le(&mut header, 4, sum);

        // body: mask over all from byte 128, then the window in CFB with the plain feedback
        let mask = idea_mask(&native_hash(name, 0), &out_hash, &material[50..58]);
        // a body of up to 128 bytes stays as it is
        let rest = body.len().saturating_sub(128);
        if rest > 0 {
            xor_mask(&mut body[128..], &mask);
        }
        let (at, n) = if rest >= 2048 {
            (1152, 1024)
        } else {
            (128, rest & !15)
        };
        let keys = rc6_keys(&material[16..32]);
        let mut feedback: [u8; 16] = material[44..60].try_into().unwrap();
        for off in (at..at + n).step_by(16) {
            let stream = rc6_encrypt(&feedback, &keys);
            for i in 0..16 {
                body[off + i] ^= stream[i];
            }
            feedback = body[off..off + 16].try_into().unwrap();
        }
        // header: AES-CBC with the fixed key and the name hash (type 2) or the first constant (1)
        let key = [
            0x56, 0xfd, 0x7e, 0x71, 0x2f, 0x6d, 0x36, 0xf5, 0x5c, 0xa5, 0x99, 0xe6, 0x7b, 0x18,
            0x10, 0x7a,
        ];
        let mut prev: [u8; 16] = if header_cipher == 2 {
            native_hash(name, 0)
        } else {
            [
                0x4c, 0xe8, 0xa1, 0xae, 0x45, 0xb3, 0x21, 0x60, 0x7e, 0x4f, 0x26, 0xa4, 0xbb, 0x14,
                0x0c, 0x96,
            ]
        };
        let aes = Aes128::new(&Array::from(key));
        for block in header[8..104].as_chunks_mut::<16>().0 {
            for i in 0..16 {
                block[i] ^= prev[i];
            }
            let mut b = Array::from(*block);
            aes.encrypt_block(&mut b);
            block.copy_from_slice(&b);
            prev = *block;
        }
        let mut payload = header.to_vec();
        payload.extend([0u8; 64]); // the signature, which is not checked
        payload.extend(body);
        payload
    }

    #[test]
    fn an_entry_of_the_chinese_variant_round_trips() {
        let mut seed = 99u32;
        // small bodies keep the whole tail in the window, big ones use the 1 KiB window at 1152
        for (len, noisy) in [(3000usize, false), (200usize, false), (40_000usize, true)] {
            let data: Vec<u8> = (0..len)
                .map(|i| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                    if noisy {
                        (seed >> 16) as u8
                    } else {
                        b"abc def "[i % 8]
                    }
                })
                .collect();
            for cipher in [1, 2] {
                let name = b"data\\texture\\cn_a.bmp";
                let payload = cn_entry(&data, name, cipher);
                let got = decode(&payload, name, data.len(), &ArchiveKeys::new()).unwrap();
                assert_eq!(got, data, "{len} bytes, header type {cipher}");
                // another size or a changed byte of the body is refused; another name too, as soon as
                // the body has a part that the name masks
                if noisy {
                    assert!(
                        decode(&payload, b"data\\x.bmp", data.len(), &ArchiveKeys::new()).is_err()
                    );
                }
                assert!(decode(&payload, name, data.len() + 1, &ArchiveKeys::new()).is_err());
                let mut bad = payload.clone();
                bad[CN_HEADER_SIZE + 3] ^= 1;
                assert!(decode(&bad, name, data.len(), &ArchiveKeys::new()).is_err());
            }
        }
        // cipher 9 needs the version 2 header
        let name = b"x";
        let mut p = cn_entry(&[7u8; 500], name, 2);
        assert!(decode(&p, name, 500, &ArchiveKeys::new()).is_ok());
        p[0] = 1;
        assert!(decode(&p, name, 500, &ArchiveKeys::new()).is_err());
    }

    #[test]
    fn the_chinese_key_block_is_only_taken_for_the_two_known_ones() {
        assert_eq!(cn_archive_keys(&[0u8; 156]), Ok(None));
        assert_eq!(cn_archive_keys(&[0u8; 20]), Ok(None));
        let ids: Vec<u32> = CN_KEYS
            .as_chunks::<68>()
            .0
            .iter()
            .map(|r| word(r, 0))
            .collect();
        assert_eq!(ids, [1, 2]);
    }
}
