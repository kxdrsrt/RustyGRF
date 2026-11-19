//! Thumbnails for the gallery view: decode an image or the first image of a sprite, shrink it to
//! fit a square, encode as PNG.

use rgrf_formats::bmp;
use rgrf_formats::png_io::{Frame, frame_from_png, rgba_to_png};
use rgrf_formats::spr::{RgbaImage, Spr};
use rgrf_formats::tga;

/// A finished thumbnail: encoded image and the size of the original.
pub struct Thumb {
    pub mime: &'static str,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Raw JPEG/GIF files up to this size are passed on unchanged (no decoder in the backend).
const RAW_LIMIT: usize = 300 * 1024;

pub fn make(ext: &str, data: &[u8], side: u32) -> Result<Thumb, String> {
    let img = match ext {
        "bmp" => {
            let mut img = bmp::parse(data).map_err(|e| e.to_string())?;
            // magenta is the client's transparent colour
            for p in img.pixels.as_chunks_mut::<4>().0 {
                if p[0] > 230 && p[1] < 20 && p[2] > 230 {
                    p[3] = 0;
                }
            }
            img
        }
        "tga" => tga::parse(data).map_err(|e| e.to_string())?,
        "spr" => first_sprite_image(data)?,
        "png" => png_image(data)?,
        "jpg" | "jpeg" | "gif" => {
            if data.len() > RAW_LIMIT {
                return Err("too large for a thumbnail".into());
            }
            return Ok(Thumb {
                mime: if ext == "gif" {
                    "image/gif"
                } else {
                    "image/jpeg"
                },
                bytes: data.to_vec(),
                width: 0,
                height: 0,
            });
        }
        _ => return Err("no thumbnail for this kind".into()),
    };
    let (width, height) = (u32::from(img.width), u32::from(img.height));
    let small = shrink(&img, side);
    Ok(Thumb {
        mime: "image/png",
        bytes: rgba_to_png(&small).map_err(|e| e.to_string())?,
        width,
        height,
    })
}

fn first_sprite_image(data: &[u8]) -> Result<RgbaImage, String> {
    let spr = Spr::parse(data).map_err(|e| e.to_string())?;
    spr.frames_rgba()
        .into_iter()
        .find(|f| f.width > 0 && f.height > 0)
        .ok_or_else(|| "the sprite has no image".to_string())
}

fn png_image(data: &[u8]) -> Result<RgbaImage, String> {
    match frame_from_png(data).map_err(|e| e.to_string())? {
        Frame::Rgba(img) => Ok(img),
        Frame::Indexed { image, palette } => {
            let mut pixels = Vec::with_capacity(image.pixels.len() * 4);
            for &i in &image.pixels {
                let c = &palette[4 * usize::from(i)..4 * usize::from(i) + 3];
                pixels.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
            Ok(RgbaImage {
                width: image.width,
                height: image.height,
                pixels,
            })
        }
    }
}

/// Averages blocks of pixels (weighted by alpha) so that the longer side is at most `side`;
/// smaller images stay as they are.
fn shrink(img: &RgbaImage, side: u32) -> RgbaImage {
    let (w, h) = (usize::from(img.width), usize::from(img.height));
    let longest = w.max(h);
    if w == 0 || h == 0 || longest <= side as usize {
        return img.clone();
    }
    let nw = (w * side as usize / longest).max(1);
    let nh = (h * side as usize / longest).max(1);
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            let (mut r, mut g, mut b, mut a, mut n) = (0u64, 0u64, 0u64, 0u64, 0u64);
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let p = &img.pixels[(sy * w + sx) * 4..][..4];
                    let al = u64::from(p[3]);
                    r += u64::from(p[0]) * al;
                    g += u64::from(p[1]) * al;
                    b += u64::from(p[2]) * al;
                    a += al;
                    n += 1;
                }
            }
            let o = &mut out[(y * nw + x) * 4..][..4];
            if let (Some(r), Some(g), Some(b), Some(al)) = (
                r.checked_div(a),
                g.checked_div(a),
                b.checked_div(a),
                a.checked_div(n),
            ) {
                o.copy_from_slice(&[r as u8, g as u8, b as u8, al as u8]);
            }
        }
    }
    RgbaImage {
        width: nw as u16,
        height: nh as u16,
        pixels: out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shrinks_to_the_square_and_keeps_the_ratio() {
        let img = RgbaImage {
            width: 400,
            height: 200,
            pixels: [200u8, 100, 50, 255].repeat(400 * 200),
        };
        let s = shrink(&img, 100);
        assert_eq!((s.width, s.height), (100, 50));
        assert_eq!(&s.pixels[..4], &[200, 100, 50, 255]);
    }

    #[test]
    fn small_images_and_transparency_stay() {
        let img = RgbaImage {
            width: 2,
            height: 1,
            pixels: vec![255, 0, 255, 0, 1, 2, 3, 255],
        };
        assert_eq!(shrink(&img, 160), img);
        let big = RgbaImage {
            width: 4,
            height: 4,
            pixels: [255u8, 0, 255, 0].repeat(16),
        };
        let s = shrink(&big, 2);
        assert_eq!((s.width, s.height), (2, 2));
        assert_eq!(s.pixels[3], 0);
    }
}
