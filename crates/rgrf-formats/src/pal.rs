//! PAL palette files: 256 colours of R, G, B and an unused fourth byte (1024 bytes).
//!
//! Derived from GRF Editor (`GRF/FileFormats/PalFormat/Pal.cs`), used with permission,
//! see `docs/05-licensing.md`.

use crate::spr::{PALETTE_LEN, RgbaImage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pal {
    /// R, G, B, unused for each of the 256 colours, exactly as stored.
    pub colors: Box<[u8; PALETTE_LEN]>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PalError {
    #[error("a PAL file is {PALETTE_LEN} bytes, this one has {0}")]
    BadLength(usize),
    #[error(
        "a palette picture must be a multiple of 16 pixels wide and high (16 x 16 cells), this one is {0} x {1}"
    )]
    BadSwatch(u16, u16),
}

impl Pal {
    pub fn parse(bytes: &[u8]) -> Result<Self, PalError> {
        let colors: Box<[u8; PALETTE_LEN]> = bytes
            .to_vec()
            .into_boxed_slice()
            .try_into()
            .map_err(|_| PalError::BadLength(bytes.len()))?;
        Ok(Self { colors })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.colors.to_vec()
    }

    /// The palette as a picture of 16 x 16 cells (`cell` pixels each, index 0 top left, rows
    /// downwards). Every cell is opaque so that the colour of index 0 survives a round trip.
    pub fn swatch(&self, cell: u16) -> RgbaImage {
        let cell = cell.clamp(1, 64);
        let side = 16 * cell;
        let mut pixels = Vec::with_capacity(usize::from(side) * usize::from(side) * 4);
        for y in 0..side {
            for x in 0..side {
                let i = usize::from(y / cell) * 16 + usize::from(x / cell);
                pixels.extend_from_slice(&self.colors[4 * i..4 * i + 3]);
                pixels.push(255);
            }
        }
        RgbaImage {
            width: side,
            height: side,
            pixels,
        }
    }

    /// The reverse of [`Pal::swatch`]: a picture of 16 x 16 equal cells of any size; each colour
    /// is read at the centre of its cell.
    pub fn from_swatch(img: &RgbaImage) -> Result<Self, PalError> {
        let (w, h) = (usize::from(img.width), usize::from(img.height));
        if w == 0 || h == 0 || w % 16 != 0 || h % 16 != 0 || img.pixels.len() != w * h * 4 {
            return Err(PalError::BadSwatch(img.width, img.height));
        }
        let (cw, ch) = (w / 16, h / 16);
        let mut colors = Box::new([0u8; PALETTE_LEN]);
        for i in 0..256 {
            let (x, y) = ((i % 16) * cw + cw / 2, (i / 16) * ch + ch / 2);
            let at = (y * w + x) * 4;
            colors[4 * i..4 * i + 3].copy_from_slice(&img.pixels[at..at + 3]);
        }
        Ok(Self { colors })
    }

    pub fn set_color(&mut self, index: u8, rgb: [u8; 3]) {
        let i = 4 * usize::from(index);
        self.colors[i..i + 3].copy_from_slice(&rgb);
    }

    /// Shifts the hue by `hue` degrees and multiplies saturation and brightness (HSV) for the
    /// colours `from..=to`; results are clamped. The unused fourth byte is kept.
    pub fn adjust(&mut self, from: u8, to: u8, hue: f32, saturation: f32, brightness: f32) {
        for i in usize::from(from)..=usize::from(to) {
            let c = &mut self.colors[4 * i..4 * i + 3];
            let (h, sat, v) = rgb_to_hsv([c[0], c[1], c[2]]);
            let rgb = hsv_to_rgb(
                (h + hue).rem_euclid(360.0),
                (sat * saturation).clamp(0.0, 1.0),
                (v * brightness).clamp(0.0, 1.0),
            );
            c.copy_from_slice(&rgb);
        }
    }

    /// Colour `index` as RGBA with the client's rule: index 0 is transparent, the rest opaque.
    pub fn rgba(&self, index: u8) -> [u8; 4] {
        let i = usize::from(index);
        [
            self.colors[4 * i],
            self.colors[4 * i + 1],
            self.colors[4 * i + 2],
            if i == 0 { 0 } else { 255 },
        ]
    }
}

/// Hue in degrees, saturation and value from 0 to 1.
fn rgb_to_hsv(c: [u8; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|v| f32::from(v) / 255.0);
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, if max == 0.0 { 0.0 } else { d / max }, max)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    [r, g, b].map(|t| ((t + m) * 255.0).round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_length_check() {
        let bytes: Vec<u8> = (0..PALETTE_LEN).map(|i| (i % 251) as u8).collect();
        let p = Pal::parse(&bytes).unwrap();
        assert_eq!(p.to_bytes(), bytes);
        assert_eq!(p.rgba(0)[3], 0);
        assert_eq!(p.rgba(1), [bytes[4], bytes[5], bytes[6], 255]);
        assert_eq!(Pal::parse(&bytes[..100]), Err(PalError::BadLength(100)));
        assert_eq!(Pal::parse(&[]), Err(PalError::BadLength(0)));
    }

    #[test]
    fn swatch_round_trips_and_adjust_moves_colours() {
        let bytes: Vec<u8> = (0..PALETTE_LEN).map(|i| (i * 7 % 256) as u8).collect();
        let p = Pal::parse(&bytes).unwrap();
        for cell in [1, 5, 16] {
            let back = Pal::from_swatch(&p.swatch(cell)).unwrap();
            // the unused fourth byte is not in the picture
            for i in 0..256 {
                assert_eq!(back.colors[4 * i..4 * i + 3], p.colors[4 * i..4 * i + 3]);
            }
        }
        let bad = RgbaImage {
            width: 17,
            height: 16,
            pixels: vec![0; 17 * 16 * 4],
        };
        assert!(Pal::from_swatch(&bad).is_err());

        let mut q = Pal::parse(&[0u8; PALETTE_LEN]).unwrap();
        q.set_color(3, [255, 0, 0]);
        q.set_color(4, [10, 20, 30]);
        q.adjust(3, 3, 120.0, 1.0, 1.0); // red turns green
        assert_eq!(q.colors[12..15], [0, 255, 0]);
        q.adjust(4, 4, 0.0, 1.0, 2.0); // twice as bright, hue unchanged
        assert_eq!(q.colors[16..19], [20, 40, 60]);
        q.adjust(4, 4, 0.0, 0.0, 1.0); // no saturation: grey
        assert!(q.colors[16] == q.colors[17] && q.colors[17] == q.colors[18]);
        assert_eq!(q.colors[0..4], [0, 0, 0, 0]); // outside the range
    }
}
