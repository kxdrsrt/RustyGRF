//! The "DES" of old GRF archives: Gravity's single-round, keyless variant of DES that scrambles
//! file names (versions 0x102/0x103) and file data (those versions, and entries with the
//! `HEADER_CRYPT`/`DATA_CRYPT` flags of any version).
//!
//! The tables and the block mixing follow GRF Editor's `DesDecryption` (Tokeiburu, used with
//! permission, see docs/05-licensing.md). Round trips (`encrypt` then `decrypt`) are tested, and
//! the real archives (`event.grf` of the original client, flags-3 entries of 0x200 archives) decode
//! to valid names and zlib streams, see docs/02-format-notes.md.

const IP: [u8; 64] = [
    58, 50, 42, 34, 26, 18, 10, 2, 60, 52, 44, 36, 28, 20, 12, 4, 62, 54, 46, 38, 30, 22, 14, 6,
    64, 56, 48, 40, 32, 24, 16, 8, 57, 49, 41, 33, 25, 17, 9, 1, 59, 51, 43, 35, 27, 19, 11, 3, 61,
    53, 45, 37, 29, 21, 13, 5, 63, 55, 47, 39, 31, 23, 15, 7,
];

const FP: [u8; 64] = [
    40, 8, 48, 16, 56, 24, 64, 32, 39, 7, 47, 15, 55, 23, 63, 31, 38, 6, 46, 14, 54, 22, 62, 30,
    37, 5, 45, 13, 53, 21, 61, 29, 36, 4, 44, 12, 52, 20, 60, 28, 35, 3, 43, 11, 51, 19, 59, 27,
    34, 2, 42, 10, 50, 18, 58, 26, 33, 1, 41, 9, 49, 17, 57, 25,
];

const TP: [u8; 32] = [
    16, 7, 20, 21, 29, 12, 28, 17, 1, 15, 23, 26, 5, 18, 31, 10, 2, 8, 24, 14, 32, 27, 3, 9, 19,
    13, 30, 6, 22, 11, 4, 25,
];

const S: [[u8; 64]; 4] = [
    [
        0xef, 0x03, 0x41, 0xfd, 0xd8, 0x74, 0x1e, 0x47, 0x26, 0xef, 0xfb, 0x22, 0xb3, 0xd8, 0x84,
        0x1e, 0x39, 0xac, 0xa7, 0x60, 0x62, 0xc1, 0xcd, 0xba, 0x5c, 0x96, 0x90, 0x59, 0x05, 0x3b,
        0x7a, 0x85, 0x40, 0xfd, 0x1e, 0xc8, 0xe7, 0x8a, 0x8b, 0x21, 0xda, 0x43, 0x64, 0x9f, 0x2d,
        0x14, 0xb1, 0x72, 0xf5, 0x5b, 0xc8, 0xb6, 0x9c, 0x37, 0x76, 0xec, 0x39, 0xa0, 0xa3, 0x05,
        0x52, 0x6e, 0x0f, 0xd9,
    ],
    [
        0xa7, 0xdd, 0x0d, 0x78, 0x9e, 0x0b, 0xe3, 0x95, 0x60, 0x36, 0x36, 0x4f, 0xf9, 0x60, 0x5a,
        0xa3, 0x11, 0x24, 0xd2, 0x87, 0xc8, 0x52, 0x75, 0xec, 0xbb, 0xc1, 0x4c, 0xba, 0x24, 0xfe,
        0x8f, 0x19, 0xda, 0x13, 0x66, 0xaf, 0x49, 0xd0, 0x90, 0x06, 0x8c, 0x6a, 0xfb, 0x91, 0x37,
        0x8d, 0x0d, 0x78, 0xbf, 0x49, 0x11, 0xf4, 0x23, 0xe5, 0xce, 0x3b, 0x55, 0xbc, 0xa2, 0x57,
        0xe8, 0x22, 0x74, 0xce,
    ],
    [
        0x2c, 0xea, 0xc1, 0xbf, 0x4a, 0x24, 0x1f, 0xc2, 0x79, 0x47, 0xa2, 0x7c, 0xb6, 0xd9, 0x68,
        0x15, 0x80, 0x56, 0x5d, 0x01, 0x33, 0xfd, 0xf4, 0xae, 0xde, 0x30, 0x07, 0x9b, 0xe5, 0x83,
        0x9b, 0x68, 0x49, 0xb4, 0x2e, 0x83, 0x1f, 0xc2, 0xb5, 0x7c, 0xa2, 0x19, 0xd8, 0xe5, 0x7c,
        0x2f, 0x83, 0xda, 0xf7, 0x6b, 0x90, 0xfe, 0xc4, 0x01, 0x5a, 0x97, 0x61, 0xa6, 0x3d, 0x40,
        0x0b, 0x58, 0xe6, 0x3d,
    ],
    [
        0x4d, 0xd1, 0xb2, 0x0f, 0x28, 0xbd, 0xe4, 0x78, 0xf6, 0x4a, 0x0f, 0x93, 0x8b, 0x17, 0xd1,
        0xa4, 0x3a, 0xec, 0xc9, 0x35, 0x93, 0x56, 0x7e, 0xcb, 0x55, 0x20, 0xa0, 0xfe, 0x6c, 0x89,
        0x17, 0x62, 0x17, 0x62, 0x4b, 0xb1, 0xb4, 0xde, 0xd1, 0x87, 0xc9, 0x14, 0x3c, 0x4a, 0x7e,
        0xa8, 0xe2, 0x7d, 0xa0, 0x9f, 0xf6, 0x5c, 0x6a, 0x09, 0x8d, 0xf0, 0x0f, 0xe3, 0x53, 0x25,
        0x95, 0x36, 0x28, 0xcb,
    ],
];

const MASK: [u8; 8] = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];

/// Pairs of byte values that the data shuffle swaps (each value maps to its partner).
const SWAP: [(u8, u8); 7] = [
    (0x00, 0x2b),
    (0x01, 0x68),
    (0x48, 0x77),
    (0x60, 0xff),
    (0x6c, 0x80),
    (0xb9, 0xc0),
    (0xeb, 0xfe),
];

fn swap_value(v: u8) -> u8 {
    SWAP.iter()
        .find_map(|&(a, b)| {
            if v == a {
                Some(b)
            } else if v == b {
                Some(a)
            } else {
                None
            }
        })
        .unwrap_or(v)
}

fn permute(src: &[u8; 8], table: &[u8; 64]) -> [u8; 8] {
    let mut out = [0u8; 8];
    for (i, &t) in table.iter().enumerate() {
        let j = usize::from(t) - 1;
        if src[(j >> 3) & 7] & MASK[j & 7] != 0 {
            out[(i >> 3) & 7] |= MASK[i & 7];
        }
    }
    out
}

fn round(src: &mut [u8; 8]) {
    let s = *src;
    let mut b = [0u8; 8];
    b[0] = ((s[7] << 5) | (s[4] >> 3)) & 0x3f;
    b[1] = ((s[4] << 1) | (s[5] >> 7)) & 0x3f;
    b[2] = ((s[4] << 5) | (s[5] >> 3)) & 0x3f;
    b[3] = ((s[5] << 1) | (s[6] >> 7)) & 0x3f;
    b[4] = ((s[5] << 5) | (s[6] >> 3)) & 0x3f;
    b[5] = ((s[6] << 1) | (s[7] >> 7)) & 0x3f;
    b[6] = ((s[6] << 5) | (s[7] >> 3)) & 0x3f;
    b[7] = ((s[7] << 1) | (s[4] >> 7)) & 0x3f;
    for i in 0..4 {
        b[i] = (S[i][usize::from(b[i * 2])] & 0xf0) | (S[i][usize::from(b[i * 2 + 1])] & 0x0f);
    }
    b[4..].fill(0);
    for (i, &t) in TP.iter().enumerate() {
        let j = usize::from(t) - 1;
        if b[j >> 3] & MASK[j & 7] != 0 {
            b[(i >> 3) + 4] |= MASK[i & 7];
        }
    }
    for i in 0..4 {
        src[i] ^= b[i + 4];
    }
}

/// One 8-byte block; the same operation encrypts and decrypts (single Feistel round).
fn block(data: &mut [u8; 8]) {
    let mut b = permute(data, &IP);
    round(&mut b);
    *data = permute(&b, &FP);
}

fn nibble_swap(data: &mut [u8; 8]) {
    for b in data {
        *b = b.rotate_left(4);
    }
}

/// The name bytes of a 0x102/0x103 table entry (a multiple of 8 bytes, as stored) decoded; the name
/// ends at the first NUL.
pub fn decode_name(stored: &[u8]) -> Vec<u8> {
    let mut v = stored.to_vec();
    for chunk in v.as_chunks_mut::<8>().0 {
        nibble_swap(chunk);
        block(chunk);
    }
    let end = v.iter().position(|&b| b == 0).unwrap_or(v.len());
    v.truncate(end);
    v
}

/// Inverse of [`decode_name`]: pads the name with NULs to a multiple of 8 bytes and encodes it.
pub fn encode_name(name: &[u8]) -> Vec<u8> {
    let mut v = name.to_vec();
    v.push(0);
    while !v.len().is_multiple_of(8) {
        v.push(0);
    }
    for chunk in v.as_chunks_mut::<8>().0 {
        block(chunk);
        nibble_swap(chunk);
    }
    v
}

/// How the data of an entry is scrambled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cycle {
    /// Only the first 20 blocks are encrypted (`DATA_CRYPT`, and the map/animation formats of
    /// 0x102/0x103 archives).
    Head,
    /// First 20 blocks, then every n-th block, with a shuffle of every 7th block in between;
    /// `n` is derived from the number of decimal digits of the stored size (`HEADER_CRYPT`).
    Mixed(u32),
}

impl Cycle {
    /// The cycle of an entry with `HEADER_CRYPT` and the given compressed size.
    pub fn for_size(zsize: u32) -> Self {
        let mut digits = 1;
        let mut i = 10u64;
        while u64::from(zsize) >= i {
            digits += 1;
            i *= 10;
        }
        Self::Mixed(digits)
    }
}

fn effective_cycle(c: u32) -> usize {
    (match c {
        0..=2 => 3,
        3..=4 => c + 1,
        5..=6 => c + 9,
        _ => c + 15,
    }) as usize
}

fn shuffle(chunk: &mut [u8; 8], decrypt: bool) {
    let t = *chunk;
    let order: [usize; 7] = if decrypt {
        [3, 4, 6, 0, 1, 2, 5]
    } else {
        [3, 4, 5, 0, 1, 6, 2]
    };
    for (i, &from) in order.iter().enumerate() {
        chunk[i] = t[from];
    }
    chunk[7] = swap_value(t[7]);
}

fn process(data: &mut [u8], cycle: Cycle, decrypt: bool) {
    let mixed = match cycle {
        Cycle::Head => None,
        Cycle::Mixed(n) => Some(effective_cycle(n)),
    };
    let mut count = 0;
    for (lop, chunk) in data.as_chunks_mut::<8>().0.iter_mut().enumerate() {
        if lop < 20 || mixed.is_some_and(|c| lop % c == 0) {
            block(chunk);
        } else {
            if mixed.is_some() && count == 7 {
                count = 0;
                shuffle(chunk, decrypt);
            }
            count += 1;
        }
    }
}

/// Decrypts `data` in place. Callers pass the stored bytes including the alignment padding (a
/// multiple of 8), as the real archives have them; a partial last block is processed as if padded
/// with zeros, which is not invertible by [`encrypt_data`] (the padding bytes are lost).
pub fn decrypt_data(data: &mut Vec<u8>, cycle: Cycle) {
    let len = data.len();
    data.resize(len.div_ceil(8) * 8, 0);
    process(data, cycle, true);
    data.truncate(len);
}

/// Inverse of [`decrypt_data`].
pub fn encrypt_data(data: &mut Vec<u8>, cycle: Cycle) {
    let len = data.len();
    data.resize(len.div_ceil(8) * 8, 0);
    process(data, cycle, false);
    data.truncate(len);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(n: usize) -> Vec<u8> {
        (0..n)
            .map(|i| (i.wrapping_mul(2654435761) >> 7) as u8)
            .collect()
    }

    #[test]
    fn a_block_is_its_own_inverse() {
        let original = *b"01234567";
        let mut b = original;
        block(&mut b);
        assert_ne!(b, original);
        block(&mut b);
        assert_eq!(b, original);
    }

    #[test]
    fn names_round_trip_and_are_padded_to_blocks() {
        for name in [&b"a.txt"[..], b"data\\sprite\\\xc0\xcc\xb8\xa7.spr", b"x"] {
            let stored = encode_name(name);
            assert_eq!(stored.len() % 8, 0);
            assert_ne!(stored[..name.len().min(8)], name[..name.len().min(8)]);
            assert_eq!(decode_name(&stored), name);
        }
        assert_eq!(decode_name(&encode_name(b"")), b"");
    }

    #[test]
    fn data_round_trips_for_every_cycle_and_length() {
        let cycles = [
            Cycle::Head,
            Cycle::Mixed(1),
            Cycle::Mixed(4),
            Cycle::Mixed(6),
            Cycle::Mixed(9),
        ];
        for cycle in cycles {
            // real entries are padded to whole blocks; a partial last block is not invertible
            for n in [0usize, 8, 160, 168, 1000, 4096, 100_000] {
                let original = sample(n);
                let mut d = original.clone();
                encrypt_data(&mut d, cycle);
                if n >= 8 {
                    assert_ne!(d, original, "{cycle:?} {n}");
                }
                decrypt_data(&mut d, cycle);
                assert_eq!(d, original, "{cycle:?} {n}");
            }
        }
    }

    #[test]
    fn head_cycle_leaves_everything_after_20_blocks() {
        let original = sample(400);
        let mut d = original.clone();
        encrypt_data(&mut d, Cycle::Head);
        assert_eq!(d[160..], original[160..]);
        assert_ne!(d[..160], original[..160]);
    }

    #[test]
    fn cycle_follows_the_digits_of_the_size() {
        assert_eq!(Cycle::for_size(0), Cycle::Mixed(1));
        assert_eq!(Cycle::for_size(9), Cycle::Mixed(1));
        assert_eq!(Cycle::for_size(10), Cycle::Mixed(2));
        assert_eq!(Cycle::for_size(99_999), Cycle::Mixed(5));
        assert_eq!(Cycle::for_size(4_221_513), Cycle::Mixed(7));
    }
}
