//! GAT: the height and walkability grid of a map (one cell per game cell).
//!
//! Layout (read from GRF Editor's `GatHeader.cs`/`Cell.cs`, checked against real files, see
//! `docs/09-formats.md`): `"GRAT"`, major, minor, `i32` width, `i32` height, then width x height
//! cells of four `f32` heights (corners: bottom-left, bottom-right, top-left, top-right) and an
//! `i32` type. Row 0 is the southern edge of the map.
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::spr::RgbaImage;
use crate::version::Version;

const HEADER_LEN: usize = 14;
const CELL_LEN: usize = 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GatError {
    #[error("not a GAT file (missing \"GRAT\" signature)")]
    BadMagic,
    #[error(
        "GAT data ends too early (header announces {width} x {height} cells, file has {len} bytes)"
    )]
    Truncated { width: i32, height: i32, len: usize },
    #[error("GAT size {0} x {1} is not valid")]
    BadSize(i32, i32),
    #[error("{0} bytes of unused data after the cells")]
    Trailing(usize),
    #[error("cell ({x}, {y}) is outside the {width} x {height} map")]
    OutOfMap {
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    /// Corner heights: bottom-left, bottom-right, top-left, top-right.
    pub heights: [f32; 4],
    /// Raw type; see [`type_name`]. Values with the high bit set exist in some files and are kept.
    pub kind: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Gat {
    pub version: Version,
    pub width: u32,
    pub height: u32,
    /// Row-major, row 0 = south.
    pub cells: Vec<Cell>,
}

/// Name of a cell type as GRF Editor calls it.
pub fn type_name(kind: i32) -> &'static str {
    match kind {
        0 => "walkable",
        1 => "not walkable",
        2 => "not walkable, not snipable",
        3 => "walkable (water)",
        4 => "unknown",
        5 => "not walkable, snipable",
        6 => "walkable",
        k if k < 0 => "weird (high bit set)",
        _ => "unknown",
    }
}

/// Cell types 0, 3 and 6 are the walkable ones.
pub fn is_walkable(kind: i32) -> bool {
    matches!(kind, 0 | 3 | 6)
}

impl Gat {
    pub fn parse(bytes: &[u8]) -> Result<Self, GatError> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != b"GRAT" {
            return Err(GatError::BadMagic);
        }
        let version = Version::new(bytes[4], bytes[5]);
        let le = |o: usize| i32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let (w, h) = (le(6), le(10));
        if w <= 0 || h <= 0 || i64::from(w) * i64::from(h) > 64 * 1024 * 1024 {
            return Err(GatError::BadSize(w, h));
        }
        let n = w as usize * h as usize;
        let need = HEADER_LEN + n * CELL_LEN;
        if bytes.len() < need {
            return Err(GatError::Truncated {
                width: w,
                height: h,
                len: bytes.len(),
            });
        }
        let f = |o: usize| f32::from_le_bytes(bytes[o..o + 4].try_into().unwrap());
        let cells = (0..n)
            .map(|i| {
                let o = HEADER_LEN + i * CELL_LEN;
                Cell {
                    heights: [f(o), f(o + 4), f(o + 8), f(o + 12)],
                    kind: le(o + 16),
                }
            })
            .collect();
        if bytes.len() > need {
            return Err(GatError::Trailing(bytes.len() - need));
        }
        Ok(Self {
            version,
            width: w as u32,
            height: h as u32,
            cells,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.cells.len() * CELL_LEN);
        out.extend(b"GRAT");
        out.extend([self.version.major, self.version.minor]);
        out.extend((self.width as i32).to_le_bytes());
        out.extend((self.height as i32).to_le_bytes());
        for c in &self.cells {
            for h in c.heights {
                out.extend(h.to_le_bytes());
            }
            out.extend(c.kind.to_le_bytes());
        }
        out
    }

    /// Cell at game coordinates (x east, y north, origin south-west).
    pub fn cell(&self, x: u32, y: u32) -> Option<&Cell> {
        (x < self.width && y < self.height).then(|| &self.cells[(y * self.width + x) as usize])
    }

    /// Sets the type and/or the four corner heights of the cells in the rectangle `x0..=x1`,
    /// `y0..=y1` (game coordinates, corners in any order). Returns how many cells changed. A
    /// rectangle outside the map is an error; one that sticks out is cut to the map.
    pub fn set_cells(
        &mut self,
        (x0, y0): (u32, u32),
        (x1, y1): (u32, u32),
        kind: Option<i32>,
        height: Option<f32>,
    ) -> Result<usize, GatError> {
        let (xa, xb) = (x0.min(x1), x0.max(x1));
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        if xa >= self.width || ya >= self.height {
            return Err(GatError::OutOfMap {
                x: xa,
                y: ya,
                width: self.width,
                height: self.height,
            });
        }
        let mut changed = 0;
        for y in ya..=yb.min(self.height - 1) {
            for x in xa..=xb.min(self.width - 1) {
                let c = &mut self.cells[(y * self.width + x) as usize];
                let before = *c;
                if let Some(k) = kind {
                    c.kind = k;
                }
                if let Some(h) = height {
                    c.heights = [h; 4];
                }
                changed += usize::from(*c != before);
            }
        }
        Ok(changed)
    }

    /// Counts of each cell type, sorted by type.
    pub fn type_counts(&self) -> Vec<(i32, usize)> {
        let mut m = std::collections::BTreeMap::new();
        for c in &self.cells {
            *m.entry(c.kind).or_insert(0usize) += 1;
        }
        m.into_iter().collect()
    }

    /// Lowest and highest corner height.
    pub fn height_range(&self) -> (f32, f32) {
        self.cells
            .iter()
            .flat_map(|c| c.heights)
            .filter(|h| h.is_finite())
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), h| {
                (lo.min(h), hi.max(h))
            })
    }

    /// Picture of the map, north at the top, one pixel per cell. Walkable cells are green (darker
    /// the lower they lie), water is blue, walls dark grey, cliffs you can shoot over lighter grey,
    /// everything else red.
    pub fn to_image(&self) -> Option<RgbaImage> {
        let (w, h) = (
            u16::try_from(self.width).ok()?,
            u16::try_from(self.height).ok()?,
        );
        let (lo, hi) = self.height_range();
        let span = (hi - lo).max(1.0);
        let mut px = Vec::with_capacity(self.cells.len() * 4);
        for y in (0..self.height).rev() {
            for x in 0..self.width {
                let c = &self.cells[(y * self.width + x) as usize];
                let shade = if c.heights[0].is_finite() {
                    (((c.heights[0] - lo) / span).clamp(0.0, 1.0) * 80.0) as u8
                } else {
                    0
                };
                let rgb = match c.kind {
                    0 | 6 => [60 + shade, 150 + shade / 2, 60 + shade],
                    3 => [50, 110 + shade, 200],
                    1 | 2 => [45, 45, 50],
                    5 => [120, 120, 130],
                    _ => [200, 50, 50],
                };
                px.extend([rgb[0], rgb[1], rgb[2], 255]);
            }
        }
        Some(RgbaImage {
            width: w,
            height: h,
            pixels: px,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Gat {
        let cell = |h: f32, kind: i32| Cell {
            heights: [h, h + 1.0, h + 2.0, h + 3.0],
            kind,
        };
        Gat {
            version: Version::new(1, 2),
            width: 3,
            height: 2,
            cells: vec![
                cell(0.0, 0),
                cell(1.0, 1),
                cell(2.0, 3),
                cell(3.0, 5),
                cell(4.0, 6),
                cell(5.0, -2147483648),
            ],
        }
    }

    #[test]
    fn set_cells_changes_a_rectangle_and_cuts_it_to_the_map() {
        let mut g = sample();
        // corners in any order, sticking out of the map
        assert_eq!(g.set_cells((2, 1), (1, 5), Some(1), None), Ok(2));
        assert_eq!(
            g.cells.iter().map(|c| c.kind).collect::<Vec<_>>(),
            [0, 1, 3, 5, 1, 1]
        );
        assert_eq!(g.set_cells((0, 0), (0, 0), None, Some(9.0)), Ok(1));
        assert_eq!(g.cells[0].heights, [9.0; 4]);
        // nothing differs: nothing counted
        assert_eq!(g.set_cells((0, 0), (0, 0), None, Some(9.0)), Ok(0));
        assert!(matches!(
            g.set_cells((3, 0), (4, 0), Some(0), None),
            Err(GatError::OutOfMap { .. })
        ));
        assert_eq!(Gat::parse(&g.to_bytes()).unwrap(), g);
    }

    #[test]
    fn round_trip() {
        let g = sample();
        let bytes = g.to_bytes();
        assert_eq!(bytes.len(), 14 + 6 * 20);
        assert_eq!(&bytes[..6], b"GRAT\x01\x02");
        let back = Gat::parse(&bytes).unwrap();
        assert_eq!(back, g);
        assert_eq!(back.to_bytes(), bytes);
    }

    #[test]
    fn lookups_counts_and_types() {
        let g = sample();
        assert_eq!(g.cell(2, 0).unwrap().kind, 3);
        assert_eq!(g.cell(0, 1).unwrap().kind, 5);
        assert!(g.cell(3, 0).is_none() && g.cell(0, 2).is_none());
        assert_eq!(g.type_counts().len(), 6);
        assert_eq!(g.height_range(), (0.0, 8.0));
        assert!(is_walkable(0) && is_walkable(3) && is_walkable(6) && !is_walkable(1));
        assert_eq!(type_name(1), "not walkable");
        assert_eq!(type_name(i32::MIN), "weird (high bit set)");
    }

    #[test]
    fn picture_has_north_at_the_top() {
        let g = sample();
        let img = g.to_image().unwrap();
        assert_eq!((img.width, img.height), (3, 2));
        // top row is game row 1: types 5, 6, weird
        assert_eq!(
            &img.pixels[..3],
            &[120, 120, 130],
            "cliff, wall colour of type 5"
        );
        // bottom row starts with the walkable cell (green dominant)
        let b = &img.pixels[3 * 4..3 * 4 + 3];
        assert!(b[1] > b[0] && b[1] > b[2]);
        // weird type is red
        assert_eq!(&img.pixels[8..11], &[200, 50, 50]);
    }

    #[test]
    fn errors_and_damaged_input() {
        assert_eq!(Gat::parse(b""), Err(GatError::BadMagic));
        assert_eq!(
            Gat::parse(b"GRSW\x01\x02aaaaaaaaaaaaaa"),
            Err(GatError::BadMagic)
        );
        let good = sample().to_bytes();
        for cut in 0..good.len() {
            assert!(Gat::parse(&good[..cut]).is_err(), "prefix {cut}");
        }
        let mut longer = good.clone();
        longer.push(0);
        assert_eq!(Gat::parse(&longer), Err(GatError::Trailing(1)));
        let mut huge = good;
        huge[6..10].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(Gat::parse(&huge), Err(GatError::BadSize(..))));
        let mut zero = sample().to_bytes();
        zero[6..10].copy_from_slice(&0i32.to_le_bytes());
        assert!(matches!(Gat::parse(&zero), Err(GatError::BadSize(..))));
    }
}
