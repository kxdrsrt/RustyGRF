//! TGA reader (preview only): colour-mapped, true-colour and grey images, raw or run-length
//! encoded, 8/15/16/24/32 bits, any origin. Reading is all that is needed; the client's TGA files
//! are never written by this tool.

use crate::spr::RgbaImage;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TgaError {
    #[error("the file is too short for a TGA image")]
    Truncated,
    #[error("unsupported TGA image type {0}")]
    Unsupported(u8),
    #[error("unsupported TGA pixel depth {0}")]
    Depth(u8),
    #[error("the TGA image has no pixels")]
    Empty,
}

struct Header {
    id_len: usize,
    map_type: u8,
    kind: u8,
    map_first: usize,
    map_len: usize,
    map_bits: u8,
    width: usize,
    height: usize,
    bits: u8,
    descriptor: u8,
}

fn u16_at(b: &[u8], i: usize) -> usize {
    usize::from(u16::from_le_bytes([b[i], b[i + 1]]))
}

/// One pixel of `bits` depth as RGBA.
fn pixel(raw: &[u8], bits: u8) -> [u8; 4] {
    match bits {
        15 | 16 => {
            let v = u16::from_le_bytes([raw[0], raw[1]]);
            let c = |s: u32| {
                let x = u32::from((v >> s) & 31);
                ((x << 3) | (x >> 2)) as u8
            };
            [
                c(10),
                c(5),
                c(0),
                if bits == 16 && v & 0x8000 == 0 {
                    0
                } else {
                    255
                },
            ]
        }
        24 => [raw[2], raw[1], raw[0], 255],
        _ => [raw[2], raw[1], raw[0], raw[3]],
    }
}

pub fn parse(bytes: &[u8]) -> Result<RgbaImage, TgaError> {
    if bytes.len() < 18 {
        return Err(TgaError::Truncated);
    }
    let h = Header {
        id_len: usize::from(bytes[0]),
        map_type: bytes[1],
        kind: bytes[2],
        map_first: u16_at(bytes, 3),
        map_len: u16_at(bytes, 5),
        map_bits: bytes[7],
        width: u16_at(bytes, 12),
        height: u16_at(bytes, 14),
        bits: bytes[16],
        descriptor: bytes[17],
    };
    if h.width == 0 || h.height == 0 {
        return Err(TgaError::Empty);
    }
    let rle = matches!(h.kind, 9..=11);
    let mapped = matches!(h.kind, 1 | 9);
    if !matches!(h.kind, 1..=3 | 9..=11) {
        return Err(TgaError::Unsupported(h.kind));
    }
    let depth_ok = if mapped {
        h.bits == 8
    } else if matches!(h.kind, 3 | 11) {
        h.bits == 8
    } else {
        matches!(h.bits, 15 | 16 | 24 | 32)
    };
    if !depth_ok {
        return Err(TgaError::Depth(h.bits));
    }
    let bpp = usize::from(h.bits).div_ceil(8);
    let mut at = 18 + h.id_len;

    let mut palette: Vec<[u8; 4]> = Vec::new();
    if h.map_type == 1 {
        if !matches!(h.map_bits, 15 | 16 | 24 | 32) {
            return Err(TgaError::Depth(h.map_bits));
        }
        let entry = usize::from(h.map_bits).div_ceil(8);
        let end = at + h.map_len * entry;
        let table = bytes.get(at..end).ok_or(TgaError::Truncated)?;
        palette = table
            .chunks_exact(entry)
            .map(|c| pixel(c, h.map_bits))
            .collect();
        at = end;
    }

    let count = h.width * h.height;
    let mut raw_pixels: Vec<[u8; 4]> = Vec::with_capacity(count);
    let mut one = |raw: &[u8]| -> [u8; 4] {
        if mapped {
            let i = usize::from(raw[0]).wrapping_sub(h.map_first);
            palette.get(i).copied().unwrap_or([0, 0, 0, 255])
        } else if matches!(h.kind, 3 | 11) {
            [raw[0], raw[0], raw[0], 255]
        } else {
            pixel(raw, h.bits)
        }
    };
    if rle {
        while raw_pixels.len() < count {
            let head = *bytes.get(at).ok_or(TgaError::Truncated)?;
            at += 1;
            let n = usize::from(head & 0x7f) + 1;
            if head & 0x80 != 0 {
                let p = one(bytes.get(at..at + bpp).ok_or(TgaError::Truncated)?);
                at += bpp;
                raw_pixels.extend(std::iter::repeat_n(p, n));
            } else {
                for _ in 0..n {
                    let p = one(bytes.get(at..at + bpp).ok_or(TgaError::Truncated)?);
                    at += bpp;
                    raw_pixels.push(p);
                }
            }
        }
        raw_pixels.truncate(count);
    } else {
        let data = bytes.get(at..at + count * bpp).ok_or(TgaError::Truncated)?;
        raw_pixels.extend(data.chunks_exact(bpp).map(&mut one));
    }

    // origin bit 5: set = top-down, clear = bottom-up; bit 4: right-to-left
    let top_down = h.descriptor & 0x20 != 0;
    let right_to_left = h.descriptor & 0x10 != 0;
    let mut pixels = Vec::with_capacity(count * 4);
    for y in 0..h.height {
        let sy = if top_down { y } else { h.height - 1 - y };
        let row = &raw_pixels[sy * h.width..(sy + 1) * h.width];
        if right_to_left {
            row.iter().rev().for_each(|p| pixels.extend(p));
        } else {
            row.iter().for_each(|p| pixels.extend(p));
        }
    }
    Ok(RgbaImage {
        width: h.width as u16,
        height: h.height as u16,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(kind: u8, w: u16, h: u16, bits: u8, descriptor: u8) -> Vec<u8> {
        let mut v = vec![0, 0, kind, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend([bits, descriptor]);
        v
    }

    #[test]
    fn raw_24_bit_bottom_up_is_flipped_and_bgr_becomes_rgb() {
        let mut t = header(2, 1, 2, 24, 0);
        t.extend([1, 2, 3]); // bottom row: B G R
        t.extend([4, 5, 6]); // top row
        let img = parse(&t).unwrap();
        assert_eq!(img.pixels, [6, 5, 4, 255, 3, 2, 1, 255]);
    }

    #[test]
    fn top_down_32_bit_keeps_alpha_and_rle_repeats() {
        let mut t = header(10, 3, 1, 32, 0x28);
        t.extend([0x82, 10, 20, 30, 40]); // 3 x the same pixel
        let img = parse(&t).unwrap();
        assert_eq!(img.pixels, [30, 20, 10, 40, 30, 20, 10, 40, 30, 20, 10, 40]);
    }

    #[test]
    fn colour_mapped_uses_the_table() {
        let mut t = header(1, 2, 1, 8, 0x20);
        t[1] = 1;
        t[5] = 2; // two table entries
        t[7] = 24;
        t.extend([0, 0, 255, 0, 255, 0]); // red, green (BGR)
        t.extend([1, 0]);
        let img = parse(&t).unwrap();
        assert_eq!(img.pixels, [0, 255, 0, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn bad_files_are_errors_not_panics() {
        assert_eq!(parse(&[0; 5]), Err(TgaError::Truncated));
        assert_eq!(parse(&header(2, 2, 2, 24, 0)), Err(TgaError::Truncated));
        assert_eq!(
            parse(&header(7, 1, 1, 24, 0)),
            Err(TgaError::Unsupported(7))
        );
        assert_eq!(parse(&header(2, 1, 1, 12, 0)), Err(TgaError::Depth(12)));
        assert_eq!(parse(&header(2, 0, 1, 24, 0)), Err(TgaError::Empty));
        let mut t = header(10, 4, 1, 24, 0);
        t.push(0x83); // run of 4, no pixel data
        assert_eq!(parse(&t), Err(TgaError::Truncated));
    }
}
