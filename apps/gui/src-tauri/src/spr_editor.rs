//! Pixel editing of a sprite's palette image: the editor works on the palette indices, so the
//! colours stay exactly those of the palette. Works on file bytes, so `backend` can apply it to a
//! staged copy.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rgrf_formats::spr::{IndexedImage, Spr};
use serde::Serialize;

/// One palette image as the editor needs it.
#[derive(Serialize, Debug)]
pub struct SprImageView {
    pub width: u16,
    pub height: u16,
    /// One palette index per pixel, rows top-down, base64.
    pub pixels: String,
    /// The 256 palette colours as `rrggbb`.
    pub palette: Vec<String>,
}

pub fn image(data: &[u8], frame: usize) -> Result<SprImageView, String> {
    let spr = Spr::parse(data).map_err(|e| e.to_string())?;
    let img = spr.indexed.get(frame).ok_or_else(|| {
        if frame < spr.indexed.len() + spr.rgba.len() {
            "this is a true-colour image: use Replace… with a PNG".to_string()
        } else {
            format!("the sprite has no image {frame}")
        }
    })?;
    let pal = spr.palette.as_ref().ok_or_else(|| err!("err-no-palette"))?;
    Ok(SprImageView {
        width: img.width,
        height: img.height,
        pixels: B64.encode(&img.pixels),
        palette: (0..256)
            .map(|i| {
                format!(
                    "{:02x}{:02x}{:02x}",
                    pal[4 * i],
                    pal[4 * i + 1],
                    pal[4 * i + 2]
                )
            })
            .collect(),
    })
}

/// Writes edited indices back; the size of the image cannot change here.
pub fn apply(data: &[u8], frame: usize, pixels_b64: &str) -> Result<Vec<u8>, String> {
    let mut spr = Spr::parse(data).map_err(|e| e.to_string())?;
    let old = spr
        .indexed
        .get(frame)
        .ok_or_else(|| format!("the sprite has no palette image {frame}"))?;
    let pixels = B64
        .decode(pixels_b64)
        .map_err(|_| "the pixels are not valid base64".to_string())?;
    if pixels.len() != usize::from(old.width) * usize::from(old.height) {
        return Err(err!("err-spr-size", "w": old.width, "h": old.height, "n": pixels.len()));
    }
    let img = IndexedImage {
        width: old.width,
        height: old.height,
        pixels,
    };
    spr.replace_indexed(frame, img).map_err(|e| e.to_string())?;
    spr.to_bytes(spr.version).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgrf_formats::spr::{PALETTE_LEN, Version};

    fn sample() -> Vec<u8> {
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[255, 0, 0, 0]);
        Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 3,
                height: 2,
                pixels: vec![0, 1, 0, 1, 1, 0],
            }],
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        }
        .to_bytes(Version::V2_1)
        .unwrap()
    }

    #[test]
    fn pixels_are_read_edited_and_checked() {
        let v = image(&sample(), 0).unwrap();
        assert_eq!((v.width, v.height), (3, 2));
        assert_eq!(v.palette[1], "ff0000");
        assert_eq!(B64.decode(&v.pixels).unwrap(), [0, 1, 0, 1, 1, 0]);

        let out = apply(&sample(), 0, &B64.encode([2, 2, 2, 0, 0, 0])).unwrap();
        let back = Spr::parse(&out).unwrap();
        assert_eq!(back.indexed[0].pixels, [2, 2, 2, 0, 0, 0]);
        assert_eq!(back.palette, Spr::parse(&sample()).unwrap().palette);

        // wrong size, bad data, missing image are errors
        assert!(apply(&sample(), 0, &B64.encode([1, 2, 3])).is_err());
        assert!(apply(&sample(), 0, "***").is_err());
        assert!(apply(&sample(), 4, &B64.encode([0; 6])).is_err());
        assert!(image(&sample(), 4).is_err());
    }
}
