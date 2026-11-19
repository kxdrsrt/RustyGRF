//! GRF Editor's encryption of GRF entries (the scheme a patched client with its `cps.dll` reads).
//!
//! Interface and behaviour follow GRF Editor (Tokeiburu, used with permission, see
//! docs/05-licensing.md): the key is a 256-byte RC4 state made from a password, an entry is
//! encrypted as a whole (compressed stream plus alignment padding) with an RC4 whose counter
//! starts at the decompressed size, and the archive gets a hidden entry `__encryption.info` with
//! the CRC32 of the key. There is no flag in the file table: an entry counts as encrypted when
//! the archive has that hidden entry and the stream does not look like zlib or LZMA. The
//! algorithm was read from the IL of GRF Editor's `Encryption.dll` (an 8 KB assembly) and checked
//! against files that GrfCL writes (docs/02-format-notes.md).

use crate::error::{Error, Result};

/// Length of the key (an RC4 state).
pub const KEY_LEN: usize = 256;
/// Length of a `.grfkey` file: 4 bytes of seed and the scrambled key.
pub const KEY_FILE_LEN: usize = 4 + KEY_LEN;
/// Name of the hidden entry that marks an encrypted archive; it holds the CRC32 of the key.
pub const MARKER_NAME: &str = "__encryption.info";

pub type Key = [u8; KEY_LEN];

/// The key a password makes: RC4's key schedule over the UTF-16 code units of the password
/// (4 to 255 of them).
pub fn key_from_password(password: &str) -> Result<Key> {
    let units: Vec<u16> = password.encode_utf16().collect();
    if units.len() < 4 || units.len() >= 256 {
        return Err(Error::InvalidInput(
            "the password must have at least 4 and fewer than 256 characters".into(),
        ));
    }
    let mut s: Key = std::array::from_fn(|i| i as u8);
    let mut j = 0usize;
    for i in 0..KEY_LEN {
        j = (j + usize::from(s[i]) + usize::from(units[i % units.len()])) % 256;
        s.swap(i, j);
    }
    Ok(s)
}

/// Scrambles (or unscrambles) a key with a seed: byte `i` is XORed with the low byte of
/// `seed * mult^i` (32-bit, wrapping).
fn scramble(bytes: &mut [u8], mut seed: i32, mult: i32) {
    for b in bytes {
        *b ^= seed as u8;
        seed = seed.wrapping_mul(mult);
    }
}

/// The content of a `.grfkey` file for `key`: seed, then the key scrambled with it. A random seed
/// makes two files for the same password differ; both give the same key.
pub fn key_file(key: &Key, seed: i32) -> [u8; KEY_FILE_LEN] {
    let mut out = [0u8; KEY_FILE_LEN];
    out[..4].copy_from_slice(&seed.to_le_bytes());
    out[4..].copy_from_slice(key);
    scramble(&mut out[4..], seed, 17);
    out
}

/// A seed for [`key_file`] (not secret, only to vary files).
pub fn random_seed() -> i32 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u8(1);
    (h.finish() & 0x7fff_ffff) as i32
}

/// The key inside a `.grfkey` file.
pub fn key_from_file(bytes: &[u8]) -> Result<Key> {
    if bytes.len() < KEY_FILE_LEN {
        return Err(Error::InvalidInput(format!(
            "a key file has {KEY_FILE_LEN} bytes, this one has {}",
            bytes.len()
        )));
    }
    let seed = i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let mut key: Key = bytes[4..KEY_FILE_LEN].try_into().expect("256 bytes");
    scramble(&mut key, seed, 17);
    Ok(key)
}

/// CRC32 of the key, as stored in the hidden entry.
pub fn key_crc(key: &Key) -> u32 {
    crc32fast::hash(key)
}

/// RC4 with the counter starting at `uncompressed_len` instead of 0: encrypts and decrypts `data`
/// in place. `uncompressed_len` is the decompressed size of the entry (of the file table: its
/// size plus 8).
pub fn crypt(key: &Key, data: &mut [u8], uncompressed_len: u64) {
    let mut s = *key;
    let mut i = uncompressed_len as usize % KEY_LEN;
    let mut j = 0usize;
    for b in data {
        i = (i + 1) % KEY_LEN;
        j = (j + usize::from(s[i])) % KEY_LEN;
        s.swap(i, j);
        *b ^= s[(usize::from(s[i]) + usize::from(s[j])) % KEY_LEN];
    }
}

/// A zlib stream header (78 9C, 78 01, 78 DA, 78 5E).
pub fn looks_zlib(data: &[u8]) -> bool {
    data.len() > 2 && data[0] == 0x78 && matches!(data[1], 0x9c | 0x01 | 0xda | 0x5e)
}

/// Stored bytes of an entry of an encrypted archive that were encrypted: not a zlib stream and not
/// an LZMA stream (those start with a zero byte).
pub fn looks_encrypted(data: &[u8]) -> bool {
    data.len() > 2 && !looks_zlib(data) && data[0] != 0
}

/// Encrypts the aligned stream of an entry the way GRF Editor does: only streams that look like
/// zlib or LZMA, and only if the result cannot be mistaken for one (else the stream stays plain).
/// Returns whether it was encrypted.
pub fn encrypt_stream(key: &Key, stream: &mut [u8], uncompressed_len: u64) -> bool {
    if !(looks_zlib(stream) || (stream.len() > 2 && stream[0] == 0)) {
        return false;
    }
    let mut trial = stream.to_vec();
    crypt(key, &mut trial, uncompressed_len);
    if looks_zlib(&trial) || trial[0] == 0 {
        return false;
    }
    stream.copy_from_slice(&trial);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypt_is_its_own_inverse_and_depends_on_the_size() {
        let key = key_from_password("secret").unwrap();
        let plain: Vec<u8> = (0..1000u32).map(|i| (i * 7) as u8).collect();
        let mut a = plain.clone();
        crypt(&key, &mut a, 5000);
        assert_ne!(a, plain);
        let mut b = a.clone();
        crypt(&key, &mut b, 5000);
        assert_eq!(b, plain);
        let mut c = plain.clone();
        crypt(&key, &mut c, 5001);
        assert_ne!(a, c);
    }

    #[test]
    fn key_files_round_trip_with_any_seed() {
        let key = key_from_password("another password").unwrap();
        for seed in [0, 1, 17, 123456789, -5, i32::MAX] {
            let f = key_file(&key, seed);
            assert_eq!(key_from_file(&f).unwrap(), key, "seed {seed}");
        }
        assert!(key_from_file(&[0; 10]).is_err());
        assert!(key_from_password("abc").is_err());
    }

    #[test]
    fn only_zlib_and_lzma_streams_are_encrypted_and_only_unambiguously() {
        let key = key_from_password("secret").unwrap();
        let mut s = vec![0x78, 0x9c, 1, 2, 3, 4, 5, 6];
        let plain = s.clone();
        assert!(encrypt_stream(&key, &mut s, 100));
        assert!(looks_encrypted(&s));
        crypt(&key, &mut s, 100);
        assert_eq!(s, plain);
        // not a compressed stream: left alone
        let mut raw = vec![1, 2, 3, 4, 5, 6, 7, 8];
        assert!(!encrypt_stream(&key, &mut raw, 100));
        assert_eq!(raw, [1, 2, 3, 4, 5, 6, 7, 8]);
        // an LZMA stream (first byte 0)
        let mut z = vec![0, 9, 9, 9, 9, 9, 9, 9];
        assert!(encrypt_stream(&key, &mut z, 100) || z == [0, 9, 9, 9, 9, 9, 9, 9]);
    }
}
