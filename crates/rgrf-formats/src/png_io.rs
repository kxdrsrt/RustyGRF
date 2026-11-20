//! PNG export/import of SPR frames.
//!
//! Indexed frames become 8-bit indexed PNGs (palette in `PLTE`, index 0 transparent via `tRNS`), so
//! a frame survives export and import without any colour change. True-colour frames become RGBA PNGs.

use std::io::Cursor;

use crate::spr::{IndexedImage, PALETTE_LEN, RgbaImage};

#[derive(Debug, thiserror::Error)]
pub enum PngError {
    #[error("PNG encoding failed: {0}")]
    Encode(#[from] png::EncodingError),
    #[error("PNG decoding failed: {0}")]
    Decode(#[from] png::DecodingError),
    #[error("image is {0} x {1}, SPR images are at most 65535 x 65535")]
    TooLarge(u32, u32),
    #[error("image size does not match its pixel data")]
    BadSize,
}

/// A frame read back from a PNG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// 8-bit indexed PNG; `palette` has the PNG's colours in R, G, B, 0 order.
    Indexed {
        image: IndexedImage,
        palette: Box<[u8; PALETTE_LEN]>,
    },
    Rgba(RgbaImage),
}

pub fn indexed_to_png(
    img: &IndexedImage,
    palette: &[u8; PALETTE_LEN],
) -> Result<Vec<u8>, PngError> {
    if img.pixels.len() != usize::from(img.width) * usize::from(img.height) {
        return Err(PngError::BadSize);
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, u32::from(img.width), u32::from(img.height));
    enc.set_color(png::ColorType::Indexed);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_palette(
        (0..256)
            .flat_map(|i| [palette[4 * i], palette[4 * i + 1], palette[4 * i + 2]])
            .collect::<Vec<u8>>(),
    );
    enc.set_trns(vec![0u8]);
    let mut w = enc.write_header()?;
    w.write_image_data(&img.pixels)?;
    w.finish()?;
    Ok(out)
}

pub fn rgba_to_png(img: &RgbaImage) -> Result<Vec<u8>, PngError> {
    encode_rgba(img, png::Compression::Balanced)
}

/// Like [`rgba_to_png`], with the fastest compression that still shrinks pictures well: for
/// pictures that are only shown (previews, map textures), where encoding time matters more than
/// a few percent of size.
pub fn rgba_to_png_fast(img: &RgbaImage) -> Result<Vec<u8>, PngError> {
    encode_rgba(img, png::Compression::Fast)
}

fn encode_rgba(img: &RgbaImage, level: png::Compression) -> Result<Vec<u8>, PngError> {
    if img.pixels.len() != usize::from(img.width) * usize::from(img.height) * 4 {
        return Err(PngError::BadSize);
    }
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, u32::from(img.width), u32::from(img.height));
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(level);
    let mut w = enc.write_header()?;
    w.write_image_data(&img.pixels)?;
    w.finish()?;
    Ok(out)
}

/// Reads a PNG as an SPR frame: 8-bit indexed stays indexed, everything else becomes RGBA.
pub fn frame_from_png(bytes: &[u8]) -> Result<Frame, PngError> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
    let info = reader.info();
    let size = (info.width, info.height);
    let (width, height) = match (u16::try_from(size.0), u16::try_from(size.1)) {
        (Ok(w), Ok(h)) => (w, h),
        _ => return Err(PngError::TooLarge(size.0, size.1)),
    };
    if info.color_type == png::ColorType::Indexed && info.bit_depth == png::BitDepth::Eight {
        let rgb = info.palette.as_deref().unwrap_or(&[]);
        let mut palette = Box::new([0u8; PALETTE_LEN]);
        for (i, c) in rgb.as_chunks::<3>().0.iter().take(256).enumerate() {
            palette[4 * i..4 * i + 3].copy_from_slice(c);
        }
        let mut pixels = vec![0u8; reader.output_buffer_size().ok_or(PngError::BadSize)?];
        let frame = reader.next_frame(&mut pixels)?;
        pixels.truncate(frame.buffer_size());
        return Ok(Frame::Indexed {
            image: IndexedImage {
                width,
                height,
                pixels,
            },
            palette,
        });
    }

    let mut dec = png::Decoder::new(Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info()?;
    let mut buf = vec![0u8; reader.output_buffer_size().ok_or(PngError::BadSize)?];
    let frame = reader.next_frame(&mut buf)?;
    buf.truncate(frame.buffer_size());
    let n = usize::from(width) * usize::from(height);
    let pixels = match frame.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(PngError::BadSize),
    };
    if pixels.len() != n * 4 {
        return Err(PngError::BadSize);
    }
    Ok(Frame::Rgba(RgbaImage {
        width,
        height,
        pixels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette() -> Box<[u8; PALETTE_LEN]> {
        let mut p = Box::new([0u8; PALETTE_LEN]);
        for i in 0..256 {
            p[4 * i] = i as u8;
            p[4 * i + 1] = (i * 3) as u8;
            p[4 * i + 2] = 200;
        }
        p
    }

    #[test]
    fn indexed_frame_survives_png() {
        let img = IndexedImage {
            width: 3,
            height: 2,
            pixels: vec![0, 1, 2, 255, 0, 7],
        };
        let png = indexed_to_png(&img, &palette()).unwrap();
        match frame_from_png(&png).unwrap() {
            Frame::Indexed { image, palette: p } => {
                assert_eq!(image, img);
                assert_eq!(p, palette());
            }
            other => panic!("expected indexed, got {other:?}"),
        }
    }

    #[test]
    fn rgba_frame_survives_png() {
        let img = RgbaImage {
            width: 2,
            height: 2,
            pixels: (0..16).collect(),
        };
        let png = rgba_to_png(&img).unwrap();
        assert_eq!(frame_from_png(&png).unwrap(), Frame::Rgba(img));
    }

    #[test]
    fn rgb_png_gets_opaque_alpha() {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, 1, 1);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[10, 20, 30]).unwrap();
        w.finish().unwrap();
        assert_eq!(
            frame_from_png(&out).unwrap(),
            Frame::Rgba(RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![10, 20, 30, 255]
            })
        );
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(frame_from_png(b"not a png").is_err());
        let bad = IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![0],
        };
        assert!(matches!(
            indexed_to_png(&bad, &palette()),
            Err(PngError::BadSize)
        ));
    }
}
