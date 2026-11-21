//! ICO and CUR files: the pictures of an icon (GRF Editor's `Ico`, with the permission of its
//! author, see `docs/05-licensing.md`). Pictures stored as PNG are handed on as they are,
//! the others (1, 4, 8, 24 and 32 bits per pixel, with the transparency mask) are decoded.

use crate::spr::RgbaImage;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IcoError {
    #[error("this is not an ICO or CUR file")]
    Header,
    #[error("the picture {0} lies outside the file")]
    Outside(usize),
    #[error("the picture {index} is stored in a way that is not supported ({what})")]
    Unsupported { index: usize, what: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IcoPicture {
    Png(Vec<u8>),
    Rgba(RgbaImage),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IcoImage {
    pub width: u32,
    pub height: u32,
    pub bits: u16,
    pub picture: IcoPicture,
}

fn le16(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}

fn le32(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize
}

/// All pictures of the file, in the order of its directory.
pub fn parse(bytes: &[u8]) -> Result<Vec<IcoImage>, IcoError> {
    if bytes.len() < 6 || le16(bytes, 0) != 0 || !(1..=2).contains(&le16(bytes, 2)) {
        return Err(IcoError::Header);
    }
    let count = le16(bytes, 4);
    if count == 0 || 6 + count * 16 > bytes.len() {
        return Err(IcoError::Header);
    }
    (0..count)
        .map(|i| {
            let e = 6 + i * 16;
            let (w, h) = (usize::from(bytes[e]), usize::from(bytes[e + 1]));
            let (size, offset) = (le32(bytes, e + 8), le32(bytes, e + 12));
            let data = offset
                .checked_add(size)
                .and_then(|end| bytes.get(offset..end))
                .ok_or(IcoError::Outside(i))?;
            let (width, height) = (if w == 0 { 256 } else { w }, if h == 0 { 256 } else { h });
            if data.starts_with(b"\x89PNG") {
                return Ok(IcoImage {
                    width: width as u32,
                    height: height as u32,
                    bits: le16(bytes, e + 6) as u16,
                    picture: IcoPicture::Png(data.to_vec()),
                });
            }
            let img = decode_dib(data).map_err(|what| IcoError::Unsupported { index: i, what })?;
            Ok(IcoImage {
                width: u32::from(img.0.width),
                height: u32::from(img.0.height),
                bits: img.1,
                picture: IcoPicture::Rgba(img.0),
            })
        })
        .collect()
}

/// A bitmap without file header, its height counting the picture and the mask.
fn decode_dib(d: &[u8]) -> Result<(RgbaImage, u16), String> {
    if d.len() < 40 || le32(d, 0) < 40 {
        return Err("no bitmap header".into());
    }
    let head = le32(d, 0);
    let w = i32::from_le_bytes([d[4], d[5], d[6], d[7]]);
    let h2 = i32::from_le_bytes([d[8], d[9], d[10], d[11]]);
    let bits = le16(d, 14) as u16;
    let compression = le32(d, 16);
    if compression != 0 {
        return Err(format!("compression {compression}"));
    }
    if !(1..=1024).contains(&w) || h2 == 0 || h2.unsigned_abs() > 2048 {
        return Err("size".into());
    }
    let (w, h) = (w as usize, (h2.unsigned_abs() / 2) as usize);
    let used = le32(d, 32);
    let colors = match bits {
        1 | 4 | 8 => {
            if used == 0 {
                1usize << bits
            } else {
                used.min(1 << bits)
            }
        }
        24 | 32 => 0,
        b => return Err(format!("{b} bits per pixel")),
    };
    let pal_at = head;
    let xor_at = pal_at + colors * 4;
    let stride = (w * usize::from(bits)).div_ceil(32) * 4;
    let mask_at = xor_at + stride * h;
    let mask_stride = w.div_ceil(32) * 4;
    if d.len() < xor_at + stride * h {
        return Err("the picture is cut off".into());
    }
    let mask = d.get(mask_at..mask_at + mask_stride * h);
    let mut px = vec![0u8; w * h * 4];
    let mut any_alpha = false;
    for y in 0..h {
        // rows are stored bottom to top
        let row = &d[xor_at + stride * (h - 1 - y)..][..stride];
        for x in 0..w {
            let o = (y * w + x) * 4;
            let rgba: [u8; 4] = match bits {
                32 => {
                    let p = &row[x * 4..x * 4 + 4];
                    any_alpha |= p[3] != 0;
                    [p[2], p[1], p[0], p[3]]
                }
                24 => {
                    let p = &row[x * 3..x * 3 + 3];
                    [p[2], p[1], p[0], 255]
                }
                _ => {
                    let per = 8 / usize::from(bits);
                    let byte = row[x / per];
                    let shift = (8 - usize::from(bits)) - (x % per) * usize::from(bits);
                    let i = usize::from(byte >> shift) & ((1 << bits) - 1);
                    let c = d
                        .get(pal_at + i * 4..pal_at + i * 4 + 3)
                        .unwrap_or(&[0, 0, 0]);
                    [c[2], c[1], c[0], 255]
                }
            };
            px[o..o + 4].copy_from_slice(&rgba);
        }
    }
    // the mask makes pixels transparent (and replaces the alpha of a picture that has none)
    if let Some(m) = mask
        && (bits != 32 || !any_alpha)
    {
        for y in 0..h {
            let row = &m[mask_stride * (h - 1 - y)..][..mask_stride];
            for x in 0..w {
                if row[x / 8] >> (7 - x % 8) & 1 == 1 {
                    px[(y * w + x) * 4 + 3] = 0;
                } else if bits == 32 {
                    px[(y * w + x) * 4 + 3] = 255;
                }
            }
        }
    }
    Ok((
        RgbaImage {
            width: w as u16,
            height: h as u16,
            pixels: px,
        },
        bits,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2x2 icon of 8 bits: palette red and blue, mask hiding the top right pixel.
    fn icon8() -> Vec<u8> {
        let mut dib = Vec::new();
        for v in [40u32, 2, 4, 0x0008_0001, 0, 0, 0, 0, 2, 0] {
            dib.extend(v.to_le_bytes());
        }
        dib.extend([0, 0, 255, 0, 255, 0, 0, 0]); // BGRA: red, blue
        dib.extend([0, 1, 0, 0, 1, 0, 0, 0]); // bottom row: 0 1; top row: 1 0
        dib.extend([0, 0, 0, 0, 0b0100_0000, 0, 0, 0]); // mask: bottom row clear; top row: second pixel hidden
        let mut f = vec![0, 0, 1, 0, 1, 0, 2, 2, 2, 0, 1, 0, 8, 0];
        f.extend((dib.len() as u32).to_le_bytes());
        f.extend(22u32.to_le_bytes());
        f.extend(dib);
        f
    }

    #[test]
    fn a_palette_icon_is_decoded_with_its_mask() {
        let i = parse(&icon8()).unwrap();
        assert_eq!((i.len(), i[0].width, i[0].height, i[0].bits), (1, 2, 2, 8));
        let IcoPicture::Rgba(img) = &i[0].picture else {
            panic!()
        };
        // top row (stored last): blue, red(hidden by the mask); bottom row: red, blue
        assert_eq!(&img.pixels[0..4], &[0, 0, 255, 255]);
        assert_eq!(&img.pixels[4..8], &[255, 0, 0, 0]);
        assert_eq!(&img.pixels[8..12], &[255, 0, 0, 255]);
        assert_eq!(&img.pixels[12..16], &[0, 0, 255, 255]);
    }

    #[test]
    fn png_pictures_are_handed_on_and_bad_files_are_refused() {
        let mut f = vec![0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 1, 0, 32, 0];
        let png = b"\x89PNG\r\n\x1a\nrest";
        f.extend((png.len() as u32).to_le_bytes());
        f.extend(22u32.to_le_bytes());
        f.extend(png);
        let i = parse(&f).unwrap();
        assert_eq!((i[0].width, i[0].height), (256, 256));
        assert_eq!(i[0].picture, IcoPicture::Png(png.to_vec()));
        assert_eq!(parse(b"nope"), Err(IcoError::Header));
        let cut = &icon8()[..40];
        assert!(matches!(parse(cut), Err(IcoError::Outside(0))));
    }
}
