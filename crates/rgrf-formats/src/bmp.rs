//! BMP reader (preview and effect textures): uncompressed 1/4/8/16/24/32-bit images, bottom-up or
//! top-down. RLE-compressed and JPEG/PNG-in-BMP files are reported as unsupported.

use crate::spr::RgbaImage;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum BmpError {
    #[error("not a BMP file")]
    NotBmp,
    #[error("the BMP is truncated")]
    Truncated,
    #[error("unsupported BMP (compression {compression}, {bits} bits per pixel)")]
    Unsupported { compression: u32, bits: u16 },
    #[error("the BMP has an invalid size")]
    Size,
}

fn u16_at(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}

fn u32_at(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([
        *b.get(i)?,
        *b.get(i + 1)?,
        *b.get(i + 2)?,
        *b.get(i + 3)?,
    ]))
}

/// Pixels as RGBA, rows top-down; the alpha is 255 (or the file's own alpha for 32-bit images that
/// carry one).
pub fn parse(bytes: &[u8]) -> Result<RgbaImage, BmpError> {
    if bytes.len() < 26 || &bytes[..2] != b"BM" {
        return Err(BmpError::NotBmp);
    }
    let data_at = u32_at(bytes, 10).ok_or(BmpError::Truncated)? as usize;
    let dib = u32_at(bytes, 14).ok_or(BmpError::Truncated)? as usize;
    let (width, height_raw, bits, compression, colors) = if dib == 12 {
        (
            i32::from(u16_at(bytes, 18).ok_or(BmpError::Truncated)?),
            i32::from(u16_at(bytes, 20).ok_or(BmpError::Truncated)?),
            u16_at(bytes, 24).ok_or(BmpError::Truncated)?,
            0,
            0usize,
        )
    } else if dib >= 40 {
        (
            u32_at(bytes, 18).ok_or(BmpError::Truncated)? as i32,
            u32_at(bytes, 22).ok_or(BmpError::Truncated)? as i32,
            u16_at(bytes, 28).ok_or(BmpError::Truncated)?,
            u32_at(bytes, 30).ok_or(BmpError::Truncated)?,
            u32_at(bytes, 46).ok_or(BmpError::Truncated)? as usize,
        )
    } else {
        return Err(BmpError::NotBmp);
    };
    let bitfields = compression == 3;
    if !(compression == 0 || (bitfields && matches!(bits, 16 | 32)))
        || !matches!(bits, 1 | 4 | 8 | 16 | 24 | 32)
    {
        return Err(BmpError::Unsupported { compression, bits });
    }
    let top_down = height_raw < 0;
    let (w, h) = (width as i64, i64::from(height_raw).abs());
    if w <= 0 || h <= 0 || w > i64::from(u16::MAX) || h > i64::from(u16::MAX) {
        return Err(BmpError::Size);
    }
    let (w, h) = (w as usize, h as usize);

    let entry = if dib == 12 { 3 } else { 4 };
    let palette_at = 14 + dib + if bitfields && dib == 40 { 12 } else { 0 };
    let n_colors = if bits <= 8 {
        if colors == 0 {
            1usize << bits
        } else {
            colors.min(256)
        }
    } else {
        0
    };
    let mut palette = vec![[0u8, 0, 0, 255]; n_colors];
    for (i, p) in palette.iter_mut().enumerate() {
        let at = palette_at + i * entry;
        let c = bytes.get(at..at + 3).ok_or(BmpError::Truncated)?;
        *p = [c[2], c[1], c[0], 255];
    }

    // channel masks for 16 and 32 bit
    let masks = if bitfields {
        let at = 14 + dib.min(40);
        if dib >= 52 || dib == 40 {
            [
                u32_at(bytes, at).ok_or(BmpError::Truncated)?,
                u32_at(bytes, at + 4).ok_or(BmpError::Truncated)?,
                u32_at(bytes, at + 8).ok_or(BmpError::Truncated)?,
                if dib >= 56 {
                    u32_at(bytes, at + 12).unwrap_or(0)
                } else {
                    0
                },
            ]
        } else {
            [0xff0000, 0xff00, 0xff, 0]
        }
    } else if bits == 16 {
        [0x7c00, 0x03e0, 0x001f, 0]
    } else {
        [0xff0000, 0xff00, 0xff, 0]
    };
    let channel = |v: u32, mask: u32| -> u8 {
        if mask == 0 {
            return 0;
        }
        let shift = mask.trailing_zeros();
        let max = mask >> shift;
        (((v & mask) >> shift) * 255 / max.max(1)) as u8
    };

    let stride = (w * usize::from(bits)).div_ceil(32) * 4;
    let rows = bytes
        .get(data_at..data_at + stride * h)
        .ok_or(BmpError::Truncated)?;
    let mut pixels = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        let src = &rows[y * stride..(y + 1) * stride];
        let dy = if top_down { y } else { h - 1 - y };
        let dst = &mut pixels[dy * w * 4..(dy + 1) * w * 4];
        for x in 0..w {
            let px = match bits {
                1 => palette
                    .get(usize::from(src[x / 8] >> (7 - x % 8) & 1))
                    .copied()
                    .unwrap_or([0, 0, 0, 255]),
                4 => palette
                    .get(usize::from(if x % 2 == 0 {
                        src[x / 2] >> 4
                    } else {
                        src[x / 2] & 15
                    }))
                    .copied()
                    .unwrap_or([0, 0, 0, 255]),
                8 => palette
                    .get(usize::from(src[x]))
                    .copied()
                    .unwrap_or([0, 0, 0, 255]),
                16 => {
                    let v = u32::from(u16::from_le_bytes([src[2 * x], src[2 * x + 1]]));
                    [
                        channel(v, masks[0]),
                        channel(v, masks[1]),
                        channel(v, masks[2]),
                        255,
                    ]
                }
                24 => [src[3 * x + 2], src[3 * x + 1], src[3 * x], 255],
                _ => {
                    let v = u32::from_le_bytes([
                        src[4 * x],
                        src[4 * x + 1],
                        src[4 * x + 2],
                        src[4 * x + 3],
                    ]);
                    let a = if masks[3] != 0 {
                        channel(v, masks[3])
                    } else {
                        255
                    };
                    any_alpha |= a != 0;
                    [
                        channel(v, masks[0]),
                        channel(v, masks[1]),
                        channel(v, masks[2]),
                        a,
                    ]
                }
            };
            dst[4 * x..4 * x + 4].copy_from_slice(&px);
        }
    }
    // a 32-bit file whose alpha channel is all zero does not use alpha
    if bits == 32 && !any_alpha {
        pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    Ok(RgbaImage {
        width: w as u16,
        height: h as u16,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bmp(bits: u16, w: u32, h: i32, palette: &[[u8; 3]], data: &[u8]) -> Vec<u8> {
        let pal_len = palette.len() * 4;
        let mut v = b"BM".to_vec();
        v.extend((54 + pal_len as u32 + data.len() as u32).to_le_bytes());
        v.extend([0; 4]);
        v.extend((54 + pal_len as u32).to_le_bytes());
        v.extend(40u32.to_le_bytes());
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(bits.to_le_bytes());
        v.extend([0; 4]); // BI_RGB
        v.extend([0; 12]);
        v.extend((palette.len() as u32).to_le_bytes());
        v.extend([0; 4]);
        for c in palette {
            v.extend([c[2], c[1], c[0], 0]);
        }
        v.extend(data);
        v
    }

    #[test]
    fn reads_24_bit_bottom_up_and_top_down() {
        // 1 x 2, rows padded to 4 bytes: bottom row first
        let up = bmp(24, 1, 2, &[], &[1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(parse(&up).unwrap().pixels, [6, 5, 4, 255, 3, 2, 1, 255]);
        let down = bmp(24, 1, -2, &[], &[1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(parse(&down).unwrap().pixels, [3, 2, 1, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn reads_palette_images() {
        let pal = [[255, 0, 0], [0, 255, 0]];
        // 8 bit: two pixels in one 4-byte row
        let img = parse(&bmp(8, 2, 1, &pal, &[1, 0, 0, 0])).unwrap();
        assert_eq!(img.pixels, [0, 255, 0, 255, 255, 0, 0, 255]);
        // 1 bit: pixels 1, 0, 1
        let img = parse(&bmp(1, 3, 1, &pal, &[0b1010_0000, 0, 0, 0])).unwrap();
        assert_eq!(img.pixels[0..4], [0, 255, 0, 255]);
        assert_eq!(img.pixels[4..8], [255, 0, 0, 255]);
        // 4 bit: pixels 0, 1
        let img = parse(&bmp(4, 2, 1, &pal, &[0x01, 0, 0, 0])).unwrap();
        assert_eq!(img.pixels[4..8], [0, 255, 0, 255]);
    }

    #[test]
    fn bad_files_are_errors() {
        assert_eq!(parse(b"nope"), Err(BmpError::NotBmp));
        assert_eq!(
            parse(&bmp(24, 4, 4, &[], &[0; 8])),
            Err(BmpError::Truncated)
        );
        let mut rle = bmp(8, 1, 1, &[[0, 0, 0]], &[0; 4]);
        rle[30] = 1; // BI_RLE8
        assert!(matches!(parse(&rle), Err(BmpError::Unsupported { .. })));
        assert_eq!(parse(&bmp(24, 0, 1, &[], &[])), Err(BmpError::Size));
    }
}
