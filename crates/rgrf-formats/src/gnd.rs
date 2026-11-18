//! GND: the ground mesh of a map. Only the header and the texture list are read here (enough to
//! know which textures a map needs); tiles, lightmaps and the cube grid are skipped.
//!
//! Layout (from GRF Editor's `GndHeader.cs`/`Gnd.cs`): `"GRGN"`, major, minor, `i32` width, `i32`
//! height, `f32` size of a cell, `i32` number of textures, `i32` bytes per texture path (80), then
//! the NUL-terminated texture paths (relative to `data\texture\`).
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::version::Version;

const HEADER_LEN: usize = 26;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GndError {
    #[error("not a GND file (missing \"GRGN\" signature)")]
    BadMagic,
    #[error("GND data ends too early (needed {need} bytes, file has {len})")]
    Truncated { need: usize, len: usize },
    #[error("GND header is not valid ({0})")]
    BadHeader(&'static str),
}

#[derive(Debug, Clone, PartialEq)]
pub struct GndSummary {
    pub version: Version,
    pub width: i32,
    pub height: i32,
    /// Size of one ground cell in map units (5 game cells = 10 in practice).
    pub cell_size: f32,
    /// Texture paths below `data\texture\`, as stored (Latin-1 view of the bytes).
    pub textures: Vec<Vec<u8>>,
}

impl GndSummary {
    pub fn parse(bytes: &[u8]) -> Result<Self, GndError> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != b"GRGN" {
            return Err(GndError::BadMagic);
        }
        let i = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let (width, height) = (i(6), i(10));
        let cell_size = f32::from_le_bytes(bytes[14..18].try_into().unwrap());
        let (count, path_size) = (i(18), i(22));
        if !(0..=100_000).contains(&count) {
            return Err(GndError::BadHeader("texture count"));
        }
        if !(1..=1024).contains(&path_size) {
            return Err(GndError::BadHeader("texture path size"));
        }
        let (count, path_size) = (count as usize, path_size as usize);
        let need = HEADER_LEN + count * path_size;
        if bytes.len() < need {
            return Err(GndError::Truncated {
                need,
                len: bytes.len(),
            });
        }
        let textures = (0..count)
            .map(|k| {
                let raw = &bytes[HEADER_LEN + k * path_size..HEADER_LEN + (k + 1) * path_size];
                raw.iter().take_while(|&&b| b != 0).copied().collect()
            })
            .collect();
        Ok(Self {
            version: Version::new(bytes[4], bytes[5]),
            width,
            height,
            cell_size,
            textures,
        })
    }

    /// Texture paths as text (Latin-1 view; names with Korean letters show as in `rgrf ls`).
    pub fn texture_names(&self) -> Vec<String> {
        self.textures
            .iter()
            .map(|t| t.iter().map(|&b| char::from(b)).collect())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(textures: &[&[u8]], path_size: usize) -> Vec<u8> {
        let mut b = b"GRGN\x01\x07".to_vec();
        b.extend(10i32.to_le_bytes());
        b.extend(20i32.to_le_bytes());
        b.extend(10.0f32.to_le_bytes());
        b.extend((textures.len() as i32).to_le_bytes());
        b.extend((path_size as i32).to_le_bytes());
        for t in textures {
            let mut p = vec![0u8; path_size];
            p[..t.len()].copy_from_slice(t);
            b.extend(p);
        }
        b.extend([9u8; 50]); // tiles etc., ignored
        b
    }

    #[test]
    fn reads_header_and_textures() {
        let g = GndSummary::parse(&file(&[b"a\\one.bmp", b"\xbf\xb9.tga"], 80)).unwrap();
        assert_eq!(g.version, Version::new(1, 7));
        assert_eq!((g.width, g.height, g.cell_size), (10, 20, 10.0));
        assert_eq!(g.textures[0], b"a\\one.bmp");
        assert_eq!(g.texture_names()[1], "\u{bf}\u{b9}.tga");
        assert_eq!(GndSummary::parse(&file(&[], 80)).unwrap().textures.len(), 0);
    }

    #[test]
    fn refuses_bad_input_without_panic() {
        assert_eq!(GndSummary::parse(b""), Err(GndError::BadMagic));
        assert_eq!(
            GndSummary::parse(b"GRATxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
            Err(GndError::BadMagic)
        );
        let good = file(&[b"x.bmp"], 80);
        for cut in 0..HEADER_LEN + 80 {
            assert!(GndSummary::parse(&good[..cut]).is_err(), "prefix {cut}");
        }
        let mut huge = good.clone();
        huge[18..22].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(
            GndSummary::parse(&huge),
            Err(GndError::BadHeader(_))
        ));
        let mut zero = good;
        zero[22..26].copy_from_slice(&0i32.to_le_bytes());
        assert!(matches!(
            GndSummary::parse(&zero),
            Err(GndError::BadHeader(_))
        ));
    }
}
