//! SPR sprite files: a list of indexed (palette) images and/or true-colour images.
//!
//! Layout (read from GRF Editor's `SprLoader.cs`/`SprWriter.cs`, checked against real files, see
//! `docs/09-formats.md`): `"SP"`, minor, major, `u16` number of indexed images, from version 2.0 a
//! `u16` number of RGBA images. Indexed images are `u16 width, u16 height` and either raw pixels
//! (before 2.1) or `u16 size` + RLE data (2.1). RGBA images are `width * height * 4` bytes stored as
//! A, B, G, R per pixel with the rows bottom-up. The 1024-byte palette (256 x R, G, B, unused) is
//! the last part of the file, index 0 is transparent.
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use std::fmt;

use crate::rle;

/// Palette size in bytes (256 entries of R, G, B, unused).
pub const PALETTE_LEN: usize = 1024;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SprError {
    #[error("not an SPR file (missing \"SP\" signature)")]
    BadMagic,
    #[error("SPR data ends too early (needed {need} bytes at offset {at}, file has {len})")]
    Truncated { at: usize, need: usize, len: usize },
    #[error("unsupported SPR version {0}")]
    UnsupportedVersion(Version),
    #[error("image {index} is {width} x {height} but holds {got} bytes instead of {expected}")]
    BadImage {
        index: usize,
        width: u16,
        height: u16,
        got: usize,
        expected: usize,
    },
    #[error("version {0} cannot hold true-colour images (needs 2.0 or later)")]
    RgbaNeedsV2(Version),
    #[error("indexed images need a palette")]
    NoPalette,
    #[error(
        "indexed image {index} compresses to {size} bytes, more than the 65535 that version 2.1 allows (use version 2.0)"
    )]
    RleTooLarge { index: usize, size: usize },
    #[error("too many images ({0}, at most 65535 of each kind)")]
    TooManyImages(usize),
}

/// SPR version as stored in the file (major, minor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u8,
    pub minor: u8,
}

impl Version {
    pub const V1_0: Self = Self::new(1, 0);
    pub const V1_1: Self = Self::new(1, 1);
    pub const V2_0: Self = Self::new(2, 0);
    pub const V2_1: Self = Self::new(2, 1);

    pub const fn new(major: u8, minor: u8) -> Self {
        Self { major, minor }
    }

    fn is_supported(self) -> bool {
        matches!(self, Self::V1_0 | Self::V1_1 | Self::V2_0 | Self::V2_1)
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

impl std::str::FromStr for Version {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("version must look like 2.1, got {s:?}");
        let (a, b) = s.split_once('.').ok_or_else(bad)?;
        Ok(Self::new(
            a.parse().map_err(|_| bad())?,
            b.parse().map_err(|_| bad())?,
        ))
    }
}

/// Palette image: one byte per pixel, rows top-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

/// True-colour image: R, G, B, A per pixel, rows top-down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spr {
    pub version: Version,
    pub indexed: Vec<IndexedImage>,
    pub rgba: Vec<RgbaImage>,
    /// The palette exactly as stored (R, G, B, unused). `None` without indexed images.
    pub palette: Option<Box<[u8; PALETTE_LEN]>>,
    /// Things the reader tolerated (image data cut off, RLE of the wrong length, ...).
    pub warnings: Vec<String>,
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], SprError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let Some(end) = end else {
            return Err(SprError::Truncated {
                at: self.pos,
                need: n,
                len: self.data.len(),
            });
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u16(&mut self) -> Result<u16, SprError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
}

impl Spr {
    /// Reads an SPR file. Fails on a bad signature, an unknown version or data that is cut off in
    /// the middle of an image; milder damage ends up in [`Spr::warnings`].
    pub fn parse(bytes: &[u8]) -> Result<Self, SprError> {
        if bytes.len() < 4 || &bytes[..2] != b"SP" {
            return Err(SprError::BadMagic);
        }
        let version = Version::new(bytes[3], bytes[2]);
        if !version.is_supported() {
            return Err(SprError::UnsupportedVersion(version));
        }
        let mut r = Reader {
            data: bytes,
            pos: 4,
        };
        let n_indexed = usize::from(r.u16()?);
        let n_rgba = if version >= Version::V2_0 {
            usize::from(r.u16()?)
        } else {
            0
        };

        let mut warnings = Vec::new();
        // The client treats the last 1024 bytes as palette whenever there are indexed images.
        let pal_start = if n_indexed > 0 {
            bytes.len().saturating_sub(PALETTE_LEN)
        } else {
            bytes.len()
        };
        let mut indexed = Vec::with_capacity(n_indexed.min(4096));
        for i in 0..n_indexed {
            if r.pos >= pal_start {
                warnings.push(format!(
                    "header announces {n_indexed} indexed images, only {i} fit before the palette"
                ));
                break;
            }
            let width = r.u16()?;
            let height = r.u16()?;
            let want = usize::from(width) * usize::from(height);
            let pixels = if version >= Version::V2_1 {
                let size = usize::from(r.u16()?);
                let d = rle::decode(r.take(size)?, want);
                if d.short {
                    warnings.push(format!("indexed image {i}: RLE data ends early"));
                }
                if d.overflow {
                    warnings.push(format!(
                        "indexed image {i}: RLE data is longer than the image"
                    ));
                }
                d.pixels
            } else {
                r.take(want)?.to_vec()
            };
            indexed.push(IndexedImage {
                width,
                height,
                pixels,
            });
        }

        let mut rgba = Vec::with_capacity(n_rgba.min(4096));
        for i in 0..n_rgba {
            if r.pos >= pal_start {
                warnings.push(format!(
                    "header announces {n_rgba} true-colour images, only {i} fit before the palette"
                ));
                break;
            }
            let width = r.u16()?;
            let height = r.u16()?;
            let (w, h) = (usize::from(width), usize::from(height));
            let raw = r.take(w * h * 4)?;
            let mut pixels = vec![0u8; w * h * 4];
            // Stored as A, B, G, R with the bottom row first.
            for y in 0..h {
                for x in 0..w {
                    let s = 4 * ((h - 1 - y) * w + x);
                    let d = 4 * (y * w + x);
                    pixels[d] = raw[s + 3];
                    pixels[d + 1] = raw[s + 2];
                    pixels[d + 2] = raw[s + 1];
                    pixels[d + 3] = raw[s];
                }
            }
            rgba.push(RgbaImage {
                width,
                height,
                pixels,
            });
        }

        // Files with only true-colour images often carry a palette anyway; keep it for a faithful rewrite.
        let palette = if indexed.is_empty() && bytes.len() - r.pos != PALETTE_LEN {
            None
        } else {
            let tail = bytes
                .len()
                .checked_sub(PALETTE_LEN)
                .ok_or(SprError::Truncated {
                    at: r.pos,
                    need: PALETTE_LEN,
                    len: bytes.len(),
                })?;
            let mut p = Box::new([0u8; PALETTE_LEN]);
            p.copy_from_slice(&bytes[tail..]);
            if tail > r.pos {
                warnings.push(format!(
                    "{} unused bytes between the images and the palette",
                    tail - r.pos
                ));
            }
            Some(p)
        };

        Ok(Self {
            version,
            indexed,
            rgba,
            palette,
            warnings,
        })
    }

    /// Serialises as `version` (2.1 is what the client files use; 2.0 stores indexed images raw).
    pub fn to_bytes(&self, version: Version) -> Result<Vec<u8>, SprError> {
        if !version.is_supported() {
            return Err(SprError::UnsupportedVersion(version));
        }
        if !self.rgba.is_empty() && version < Version::V2_0 {
            return Err(SprError::RgbaNeedsV2(version));
        }
        if self.indexed.len() > usize::from(u16::MAX) {
            return Err(SprError::TooManyImages(self.indexed.len()));
        }
        if self.rgba.len() > usize::from(u16::MAX) {
            return Err(SprError::TooManyImages(self.rgba.len()));
        }
        if !self.indexed.is_empty() && self.palette.is_none() {
            return Err(SprError::NoPalette);
        }

        let mut out = vec![b'S', b'P', version.minor, version.major];
        out.extend((self.indexed.len() as u16).to_le_bytes());
        if version >= Version::V2_0 {
            out.extend((self.rgba.len() as u16).to_le_bytes());
        }
        for (i, img) in self.indexed.iter().enumerate() {
            let expected = usize::from(img.width) * usize::from(img.height);
            if img.pixels.len() != expected {
                return Err(SprError::BadImage {
                    index: i,
                    width: img.width,
                    height: img.height,
                    got: img.pixels.len(),
                    expected,
                });
            }
            out.extend(img.width.to_le_bytes());
            out.extend(img.height.to_le_bytes());
            if version >= Version::V2_1 {
                let packed = rle::encode(&img.pixels);
                let size = u16::try_from(packed.len()).map_err(|_| SprError::RleTooLarge {
                    index: i,
                    size: packed.len(),
                })?;
                out.extend(size.to_le_bytes());
                out.extend(packed);
            } else {
                out.extend(&img.pixels);
            }
        }
        for (i, img) in self.rgba.iter().enumerate() {
            let (w, h) = (usize::from(img.width), usize::from(img.height));
            if img.pixels.len() != w * h * 4 {
                return Err(SprError::BadImage {
                    index: self.indexed.len() + i,
                    width: img.width,
                    height: img.height,
                    got: img.pixels.len(),
                    expected: w * h * 4,
                });
            }
            out.extend(img.width.to_le_bytes());
            out.extend(img.height.to_le_bytes());
            let mut raw = vec![0u8; w * h * 4];
            for y in 0..h {
                for x in 0..w {
                    let s = 4 * (y * w + x);
                    let d = 4 * ((h - 1 - y) * w + x);
                    raw[d] = img.pixels[s + 3];
                    raw[d + 1] = img.pixels[s + 2];
                    raw[d + 2] = img.pixels[s + 1];
                    raw[d + 3] = img.pixels[s];
                }
            }
            out.extend(raw);
        }
        if let Some(p) = &self.palette {
            out.extend(p.as_slice());
        }
        Ok(out)
    }

    /// Palette entries as RGBA with the client's rule: index 0 is transparent, all others opaque.
    pub fn palette_rgba(&self) -> Option<[[u8; 4]; 256]> {
        let p = self.palette.as_ref()?;
        let mut out = [[0u8; 4]; 256];
        for (i, c) in out.iter_mut().enumerate() {
            *c = [
                p[4 * i],
                p[4 * i + 1],
                p[4 * i + 2],
                if i == 0 { 0 } else { 255 },
            ];
        }
        Some(out)
    }

    /// Indexed image `index` as true colour.
    pub fn indexed_to_rgba(&self, index: usize) -> Option<RgbaImage> {
        let img = self.indexed.get(index)?;
        let pal = self.palette_rgba()?;
        let mut pixels = Vec::with_capacity(img.pixels.len() * 4);
        for &i in &img.pixels {
            pixels.extend(pal[usize::from(i)]);
        }
        Some(RgbaImage {
            width: img.width,
            height: img.height,
            pixels,
        })
    }

    /// Every frame as true colour, indexed ones first (the order of the files).
    pub fn frames_rgba(&self) -> Vec<RgbaImage> {
        let mut v: Vec<RgbaImage> = (0..self.indexed.len())
            .filter_map(|i| self.indexed_to_rgba(i))
            .collect();
        v.extend(self.rgba.iter().cloned());
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Spr {
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        for i in 0..256 {
            pal[4 * i] = i as u8;
            pal[4 * i + 1] = 255 - i as u8;
            pal[4 * i + 2] = 7;
        }
        let a = IndexedImage {
            width: 3,
            height: 2,
            pixels: vec![0, 0, 5, 0, 9, 0],
        };
        let b = IndexedImage {
            width: 1,
            height: 1,
            pixels: vec![0],
        };
        let c = RgbaImage {
            width: 2,
            height: 2,
            pixels: (0..16).collect(),
        };
        Spr {
            version: Version::V2_1,
            indexed: vec![a, b],
            rgba: vec![c],
            palette: Some(pal),
            warnings: vec![],
        }
    }

    #[test]
    fn round_trip_all_versions() {
        let s = sample();
        for v in [Version::V2_0, Version::V2_1] {
            let bytes = s.to_bytes(v).unwrap();
            let back = Spr::parse(&bytes).unwrap();
            assert_eq!(back.version, v);
            assert_eq!(back.indexed, s.indexed);
            assert_eq!(back.rgba, s.rgba);
            assert_eq!(back.palette, s.palette);
            assert!(back.warnings.is_empty(), "{:?}", back.warnings);
            assert_eq!(back.to_bytes(v).unwrap(), bytes, "writing is stable");
        }
        let mut only_indexed = sample();
        only_indexed.rgba.clear();
        let bytes = only_indexed.to_bytes(Version::V1_1).unwrap();
        assert_eq!(&bytes[..4], b"SP\x01\x01");
        assert_eq!(Spr::parse(&bytes).unwrap().indexed, only_indexed.indexed);
    }

    #[test]
    fn rgba_is_stored_abgr_bottom_up() {
        let s = Spr {
            version: Version::V2_0,
            indexed: vec![],
            rgba: vec![RgbaImage {
                width: 1,
                height: 2,
                // top pixel R1 G2 B3 A4, bottom pixel R5 G6 B7 A8
                pixels: vec![1, 2, 3, 4, 5, 6, 7, 8],
            }],
            palette: None,
            warnings: vec![],
        };
        let b = s.to_bytes(Version::V2_0).unwrap();
        // header 8 bytes, w, h, then the bottom pixel first as A B G R
        assert_eq!(&b[12..], &[8, 7, 6, 5, 4, 3, 2, 1]);
    }

    #[test]
    fn rgba_only_file_keeps_its_trailing_palette() {
        let mut s = sample();
        s.indexed.clear();
        let bytes = s.to_bytes(Version::V2_1).unwrap();
        let back = Spr::parse(&bytes).unwrap();
        assert_eq!(back.palette, s.palette);
        assert_eq!(back.to_bytes(Version::V2_1).unwrap(), bytes);
        // without a palette none is invented
        s.palette = None;
        let bytes = s.to_bytes(Version::V2_1).unwrap();
        assert_eq!(Spr::parse(&bytes).unwrap().palette, None);
    }

    #[test]
    fn palette_rule_index_zero_transparent() {
        let s = sample();
        let p = s.palette_rgba().unwrap();
        assert_eq!(p[0][3], 0);
        assert_eq!(p[1][3], 255);
        let img = s.indexed_to_rgba(0).unwrap();
        assert_eq!(img.pixels[3], 0);
        assert_eq!(img.pixels[2 * 4 + 3], 255);
        assert_eq!(s.frames_rgba().len(), 3);
    }

    #[test]
    fn rejects_garbage_without_panic() {
        assert_eq!(Spr::parse(b""), Err(SprError::BadMagic));
        assert_eq!(Spr::parse(b"XX\x01\x02aaaa"), Err(SprError::BadMagic));
        assert!(matches!(
            Spr::parse(b"SP\x09\x09\x00\x00"),
            Err(SprError::UnsupportedVersion(_))
        ));
        let good = sample().to_bytes(Version::V2_1).unwrap();
        for cut in 0..good.len() {
            // every prefix must give an error or a result, never a panic
            let _ = Spr::parse(&good[..cut]);
        }
    }

    #[test]
    fn writer_validates() {
        let mut s = sample();
        assert_eq!(
            s.to_bytes(Version::V1_1),
            Err(SprError::RgbaNeedsV2(Version::V1_1))
        );
        s.rgba.clear();
        s.indexed[0].pixels.pop();
        assert!(matches!(
            s.to_bytes(Version::V2_1),
            Err(SprError::BadImage { index: 0, .. })
        ));
        let mut s = sample();
        s.palette = None;
        assert_eq!(s.to_bytes(Version::V2_1), Err(SprError::NoPalette));
        // worst case for RLE: alternating 0, 1 takes 3 bytes per 2 pixels
        let w = 300u16;
        let big = IndexedImage {
            width: w,
            height: w,
            pixels: (0..usize::from(w) * usize::from(w))
                .map(|i| (i % 2) as u8)
                .collect(),
        };
        let mut s = sample();
        s.indexed = vec![big];
        s.rgba.clear();
        assert!(matches!(
            s.to_bytes(Version::V2_1),
            Err(SprError::RleTooLarge { .. })
        ));
        assert!(s.to_bytes(Version::V2_0).is_ok());
    }

    #[test]
    fn version_parse_and_display() {
        assert_eq!("2.1".parse::<Version>().unwrap(), Version::V2_1);
        assert!("2".parse::<Version>().is_err());
        assert_eq!(Version::V1_1.to_string(), "1.1");
    }
}
