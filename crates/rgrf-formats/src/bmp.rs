//! BMP reader (preview and effect textures): uncompressed 1/4/8/16/24/32-bit images, bottom-up or
//! top-down. RLE-compressed and JPEG/PNG-in-BMP files are reported as unsupported.
//!
//! Writing produces what the client's own files look like (measured on all 51,782 BMPs of kRO's
//! `data.grf`): a 40-byte header, rows bottom-up, 8-bit with a palette of only the colours used
//! (12,059 of the 21,461 8-bit files have fewer than 256 entries) or 24-bit.

use crate::quantize::{self, QuantizeError};
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

/// The header fields that decide what a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BmpInfo {
    pub width: u32,
    pub height: u32,
    pub bits: u16,
    pub compression: u32,
}

pub fn info(bytes: &[u8]) -> Result<BmpInfo, BmpError> {
    if bytes.len() < 26 || &bytes[..2] != b"BM" {
        return Err(BmpError::NotBmp);
    }
    let dib = u32_at(bytes, 14).ok_or(BmpError::Truncated)?;
    let t = || BmpError::Truncated;
    Ok(if dib == 12 {
        BmpInfo {
            width: u32::from(u16_at(bytes, 18).ok_or_else(t)?),
            height: u32::from(u16_at(bytes, 20).ok_or_else(t)?),
            bits: u16_at(bytes, 24).ok_or_else(t)?,
            compression: 0,
        }
    } else if dib >= 40 {
        BmpInfo {
            width: u32_at(bytes, 18).ok_or_else(t)?,
            height: (u32_at(bytes, 22).ok_or_else(t)? as i32).unsigned_abs(),
            bits: u16_at(bytes, 28).ok_or_else(t)?,
            compression: u32_at(bytes, 30).ok_or_else(t)?,
        }
    } else {
        return Err(BmpError::NotBmp);
    })
}

fn header(width: u16, height: u16, bits: u16, palette_len: usize, data_len: usize) -> Vec<u8> {
    let data_at = 54 + 4 * palette_len as u32;
    let mut v = Vec::with_capacity(data_at as usize + data_len);
    v.extend(b"BM");
    v.extend((data_at + data_len as u32).to_le_bytes());
    v.extend([0; 4]);
    v.extend(data_at.to_le_bytes());
    v.extend(40u32.to_le_bytes());
    v.extend(u32::from(width).to_le_bytes());
    v.extend(u32::from(height).to_le_bytes()); // positive: bottom-up
    v.extend(1u16.to_le_bytes());
    v.extend(bits.to_le_bytes());
    v.extend(0u32.to_le_bytes()); // BI_RGB
    v.extend((data_len as u32).to_le_bytes());
    v.extend(2835u32.to_le_bytes()); // 72 dpi
    v.extend(2835u32.to_le_bytes());
    v.extend((palette_len as u32).to_le_bytes());
    v.extend(0u32.to_le_bytes());
    v
}

/// An 8-bit BMP. `indices` holds one palette index per pixel, rows top-down.
pub fn write_indexed(width: u16, height: u16, palette: &[[u8; 3]], indices: &[u8]) -> Vec<u8> {
    let (w, h) = (usize::from(width), usize::from(height));
    assert!(palette.len() <= 256 && indices.len() == w * h);
    let stride = w.div_ceil(4) * 4;
    let mut v = header(width, height, 8, palette.len(), stride * h);
    for c in palette {
        v.extend([c[2], c[1], c[0], 0]);
    }
    for y in (0..h).rev() {
        v.extend(&indices[y * w..(y + 1) * w]);
        v.resize(v.len() + stride - w, 0);
    }
    v
}

/// A 24-bit BMP. Pixels with alpha below 128 become magenta, the client's transparent colour.
pub fn write_rgb(img: &RgbaImage) -> Vec<u8> {
    let (w, h) = (usize::from(img.width), usize::from(img.height));
    let stride = (w * 3).div_ceil(4) * 4;
    let mut v = header(img.width, img.height, 24, 0, stride * h);
    for y in (0..h).rev() {
        for p in img.pixels[y * w * 4..(y + 1) * w * 4].as_chunks::<4>().0 {
            v.extend(if p[3] < 128 {
                [255, 0, 255]
            } else {
                [p[2], p[1], p[0]]
            });
        }
        v.resize(v.len() + stride - w * 3, 0);
    }
    v
}

/// The opaque colours of an image; pixels with alpha below 128 count as magenta.
pub fn rgb_of(img: &RgbaImage) -> Vec<[u8; 3]> {
    img.pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if p[3] < 128 {
                [255, 0, 255]
            } else {
                [p[0], p[1], p[2]]
            }
        })
        .collect()
}

/// An image as an 8-bit BMP, with its PSNR (see [`quantize::quantize`]).
pub fn write_reduced(
    img: &RgbaImage,
    max_colors: usize,
) -> Result<(Vec<u8>, f64, usize), QuantizeError> {
    let q = quantize::quantize(&rgb_of(img), max_colors)?;
    Ok((
        write_indexed(img.width, img.height, &q.palette, &q.indices),
        q.psnr,
        q.palette.len(),
    ))
}

/// What [`reduce`] did with a file.
#[derive(Debug, Clone, PartialEq)]
pub enum Reduced {
    /// The 8-bit file, its colour count and PSNR (infinite when no colour changed).
    Done {
        bytes: Vec<u8>,
        colors: usize,
        psnr: f64,
    },
    /// Left alone, and why.
    Kept(Keep),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Keep {
    /// Not a BMP this reader understands.
    Unreadable(BmpError),
    /// Already 8 bits per pixel or fewer.
    Indexed(u16),
    /// A 32-bit file with a real alpha channel (8-bit BMPs have none).
    Alpha,
    /// Too noisy around magenta to keep the transparency exact.
    KeyColors(usize),
    /// The reduced image would differ too much: its PSNR.
    Quality(f64),
    /// The 8-bit file would not be smaller (tiny images, where the palette outweighs the pixels).
    NotSmaller,
}

impl Keep {
    /// A stable word for reports (JSON keys).
    pub fn reason(&self) -> &'static str {
        match self {
            Keep::Unreadable(_) => "unreadable",
            Keep::Indexed(_) => "already_8_bit",
            Keep::Alpha => "alpha",
            Keep::KeyColors(_) => "noisy_transparency",
            Keep::Quality(_) => "below_min_psnr",
            Keep::NotSmaller => "not_smaller",
        }
    }
}

/// A file of an archive that gets smaller as 8-bit BMP.
#[derive(Debug, Clone, PartialEq)]
pub struct Stored {
    pub bytes: Vec<u8>,
    pub psnr: f64,
    pub stored_before: u64,
    pub stored_after: u64,
}

/// [`reduce`] for a file of an archive: only worth it when the stored file gets smaller.
/// `stored_before` is its stored size now; `store` gives the stored size of new content (the
/// archive's compression). Flat 24-bit pictures often compress better than their 8-bit version.
pub fn reduce_for_storage(
    bytes: &[u8],
    min_psnr: f64,
    stored_before: u64,
    store: impl Fn(&[u8]) -> u64,
) -> Result<Stored, Keep> {
    match reduce(bytes, min_psnr) {
        Reduced::Kept(k) => Err(k),
        Reduced::Done { bytes, psnr, .. } => {
            let stored_after = store(&bytes);
            if stored_after >= stored_before {
                return Err(Keep::NotSmaller);
            }
            Ok(Stored {
                bytes,
                psnr,
                stored_before,
                stored_after,
            })
        }
    }
}

/// Turns a 16/24/32-bit BMP into an 8-bit one when the result keeps at least `min_psnr` dB.
/// Images with at most 256 colours always qualify, they lose nothing.
pub fn reduce(bytes: &[u8], min_psnr: f64) -> Reduced {
    let info = match info(bytes) {
        Ok(i) => i,
        Err(e) => return Reduced::Kept(Keep::Unreadable(e)),
    };
    if info.bits <= 8 {
        return Reduced::Kept(Keep::Indexed(info.bits));
    }
    let img = match parse(bytes) {
        Ok(i) => i,
        Err(e) => return Reduced::Kept(Keep::Unreadable(e)),
    };
    if img.pixels.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Reduced::Kept(Keep::Alpha);
    }
    match write_reduced(&img, 256) {
        Err(QuantizeError::KeyColors(n)) => Reduced::Kept(Keep::KeyColors(n)),
        // an image has pixels and 256 colours always leave room, so only the size can be wrong
        Err(_) => Reduced::Kept(Keep::Unreadable(BmpError::Size)),
        Ok((_, psnr, _)) if psnr < min_psnr => Reduced::Kept(Keep::Quality(psnr)),
        Ok((out, _, _)) if out.len() >= bytes.len() => Reduced::Kept(Keep::NotSmaller),
        Ok((bytes, psnr, colors)) => Reduced::Done {
            bytes,
            colors,
            psnr,
        },
    }
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

    fn image(w: u16, h: u16, f: impl Fn(usize, usize) -> [u8; 4]) -> RgbaImage {
        let mut pixels = Vec::new();
        for y in 0..usize::from(h) {
            for x in 0..usize::from(w) {
                pixels.extend(f(x, y));
            }
        }
        RgbaImage {
            width: w,
            height: h,
            pixels,
        }
    }

    #[test]
    fn written_files_read_back() {
        // odd width, so every row is padded
        let img = image(3, 2, |x, y| [x as u8 * 50, y as u8 * 90, 7, 255]);
        let bmp = write_rgb(&img);
        assert_eq!(
            info(&bmp).unwrap(),
            BmpInfo {
                width: 3,
                height: 2,
                bits: 24,
                compression: 0
            }
        );
        assert_eq!(parse(&bmp).unwrap(), img);
        let (bmp8, psnr, colors) = write_reduced(&img, 256).unwrap();
        assert_eq!((psnr, colors), (f64::INFINITY, 6));
        assert_eq!(info(&bmp8).unwrap().bits, 8);
        assert_eq!(parse(&bmp8).unwrap(), img);
    }

    #[test]
    fn transparent_pixels_become_magenta() {
        let img = image(
            2,
            1,
            |x, _| if x == 0 { [1, 2, 3, 0] } else { [4, 5, 6, 255] },
        );
        assert_eq!(
            parse(&write_rgb(&img)).unwrap().pixels,
            [255, 0, 255, 255, 4, 5, 6, 255]
        );
    }

    #[test]
    fn reduce_keeps_what_it_cannot_shrink() {
        let few = write_rgb(&image(64, 64, |x, y| {
            [(x / 8 * 30) as u8, (y / 8 * 30) as u8, 0, 255]
        }));
        match reduce(&few, 99.0) {
            Reduced::Done {
                bytes,
                colors,
                psnr,
            } => {
                assert_eq!((colors, psnr), (64, f64::INFINITY));
                assert!(bytes.len() < few.len() / 2);
                assert_eq!(parse(&bytes).unwrap(), parse(&few).unwrap());
            }
            other => panic!("{other:?}"),
        }
        // smooth gradient with many colours: fine at 30 dB, refused at 60 dB
        let many = write_rgb(&image(128, 128, |x, y| {
            [x as u8 * 2, y as u8 * 2, (x + y) as u8, 255]
        }));
        assert!(matches!(
            reduce(&many, 30.0),
            Reduced::Done { colors: 256, .. }
        ));
        assert!(matches!(reduce(&many, 60.0), Reduced::Kept(Keep::Quality(p)) if p < 60.0));
        // tiny: the palette outweighs the pixels
        let tiny = write_rgb(&image(2, 2, |x, y| [x as u8, y as u8, 0, 255]));
        assert_eq!(reduce(&tiny, 0.0), Reduced::Kept(Keep::NotSmaller));
        let (indexed, ..) = write_reduced(&image(8, 8, |_, _| [1, 1, 1, 255]), 256).unwrap();
        assert_eq!(reduce(&indexed, 0.0), Reduced::Kept(Keep::Indexed(8)));
        assert!(matches!(
            reduce(b"nope", 0.0),
            Reduced::Kept(Keep::Unreadable(_))
        ));
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
