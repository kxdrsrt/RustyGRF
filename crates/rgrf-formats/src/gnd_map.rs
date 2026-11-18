//! GND ground with its tiles, lightmaps and cube grid, for the 3D map view. (`gnd.rs` reads only
//! the header and the texture list.) Reading only; the layout follows the one used by the
//! client's tools and was checked against all real maps (see the tests with real data).

use crate::version::Version;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GroundError {
    #[error("not a GND file (missing \"GRGN\" signature)")]
    BadMagic,
    #[error("the GND file is truncated or damaged")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    /// u1 u2 u3 u4 v1 v2 v3 v4, as stored.
    pub uv: [f32; 8],
    pub texture: u16,
    pub lightmap: u16,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Cube {
    /// Heights of the four corners: top-left, top-right, bottom-left, bottom-right (raw units).
    pub height: [f32; 4],
    /// Tile of the top surface, of the wall towards the next row and of the wall towards the next
    /// column; `-1` for none.
    pub up: i32,
    pub front: i32,
    pub right: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ground {
    pub version: Version,
    pub width: usize,
    pub height: usize,
    /// Size of one cell in raw units (10 in practice; the view uses a fifth of it).
    pub zoom: f32,
    pub textures: Vec<Vec<u8>>,
    /// Number of lightmaps, each 8 x 8: 64 brightness bytes, then 64 RGB triples.
    pub lightmap_count: usize,
    pub lightmaps: Vec<u8>,
    pub tiles: Vec<Tile>,
    pub cubes: Vec<Cube>,
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], GroundError> {
        let end = self.at.checked_add(n).ok_or(GroundError::Truncated)?;
        let s = self.d.get(self.at..end).ok_or(GroundError::Truncated)?;
        self.at = end;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, GroundError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Result<i32, GroundError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, GroundError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn u16(&mut self) -> Result<u16, GroundError> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn remaining(&self) -> usize {
        self.d.len() - self.at
    }
}

pub fn parse(bytes: &[u8]) -> Result<Ground, GroundError> {
    if bytes.len() < 6 || &bytes[..4] != b"GRGN" {
        return Err(GroundError::BadMagic);
    }
    let version = Version::new(bytes[4], bytes[5]);
    let mut r = Reader { d: bytes, at: 6 };
    let width = r.u32()? as usize;
    let height = r.u32()? as usize;
    let zoom = r.f32()?;
    let cells = width.checked_mul(height).ok_or(GroundError::Truncated)?;
    if cells > 4_000_000 || r.remaining() < cells * 28 {
        return Err(GroundError::Truncated);
    }

    let n_tex = r.u32()? as usize;
    let len = r.u32()? as usize;
    if len == 0 || len > 1024 || n_tex.saturating_mul(len) > r.remaining() {
        return Err(GroundError::Truncated);
    }
    let mut textures = Vec::with_capacity(n_tex);
    for _ in 0..n_tex {
        let raw = r.take(len)?;
        let end = raw.iter().position(|&b| b == 0).unwrap_or(len);
        textures.push(raw[..end].to_vec());
    }

    let lightmap_count = r.u32()? as usize;
    let (px, py, size) = (r.i32()?, r.i32()?, r.i32()?);
    let per_cell = usize::try_from(px)
        .ok()
        .zip(usize::try_from(py).ok())
        .zip(usize::try_from(size).ok());
    let per_cell = per_cell
        .map(|((a, b), c)| a.saturating_mul(b).saturating_mul(c))
        .ok_or(GroundError::Truncated)?;
    if per_cell != 64 || lightmap_count.saturating_mul(per_cell * 4) > r.remaining() {
        return Err(GroundError::Truncated);
    }
    let lightmaps = r.take(lightmap_count * per_cell * 4)?.to_vec();

    let n_tiles = r.u32()? as usize;
    if n_tiles.saturating_mul(40) > r.remaining() {
        return Err(GroundError::Truncated);
    }
    let mut tiles = Vec::with_capacity(n_tiles);
    for _ in 0..n_tiles {
        let mut uv = [0.0; 8];
        for v in &mut uv {
            *v = r.f32()?;
        }
        let texture = r.u16()?;
        let lightmap = r.u16()?;
        let c = r.take(4)?;
        tiles.push(Tile {
            uv,
            texture,
            lightmap,
            color: [c[0], c[1], c[2], c[3]],
        });
    }

    let mut cubes = Vec::with_capacity(cells);
    for _ in 0..cells {
        let mut h = [0.0; 4];
        for v in &mut h {
            *v = r.f32()?;
        }
        cubes.push(Cube {
            height: h,
            up: r.i32()?,
            front: r.i32()?,
            right: r.i32()?,
        });
    }
    Ok(Ground {
        version,
        width,
        height,
        zoom,
        textures,
        lightmap_count,
        lightmaps,
        tiles,
        cubes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(w: u32, h: u32, tiles: u32, cube_tile: i32) -> Vec<u8> {
        let mut b = b"GRGN\x01\x07".to_vec();
        for v in [w, h] {
            b.extend(v.to_le_bytes());
        }
        b.extend(10.0f32.to_le_bytes());
        b.extend(1u32.to_le_bytes()); // textures
        b.extend(80u32.to_le_bytes());
        let mut name = b"a.bmp".to_vec();
        name.resize(80, 0);
        b.extend(name);
        b.extend(1u32.to_le_bytes()); // lightmaps
        for v in [8i32, 8, 1] {
            b.extend(v.to_le_bytes());
        }
        b.extend([255u8; 256]);
        b.extend(tiles.to_le_bytes());
        for _ in 0..tiles {
            for v in [0.0f32, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 1.0] {
                b.extend(v.to_le_bytes());
            }
            b.extend(0u16.to_le_bytes());
            b.extend(0u16.to_le_bytes());
            b.extend([1, 2, 3, 4]);
        }
        for _ in 0..w * h {
            for v in [5.0f32, 5.0, 10.0, 10.0] {
                b.extend(v.to_le_bytes());
            }
            for t in [cube_tile, -1, -1] {
                b.extend(t.to_le_bytes());
            }
        }
        b
    }

    #[test]
    fn reads_textures_lightmaps_tiles_and_cubes() {
        let g = parse(&file(2, 3, 1, 0)).unwrap();
        assert_eq!((g.width, g.height, g.zoom), (2, 3, 10.0));
        assert_eq!(g.textures, [b"a.bmp".to_vec()]);
        assert_eq!((g.lightmap_count, g.lightmaps.len()), (1, 256));
        assert_eq!(g.tiles[0].color, [1, 2, 3, 4]);
        assert_eq!(g.cubes.len(), 6);
        assert_eq!(
            (g.cubes[0].height[3], g.cubes[0].up, g.cubes[0].front),
            (10.0, 0, -1)
        );
    }

    #[test]
    fn bad_files_are_errors_not_panics() {
        assert_eq!(parse(b"nope"), Err(GroundError::BadMagic));
        let f = file(2, 2, 1, 0);
        assert_eq!(parse(&f[..f.len() - 3]), Err(GroundError::Truncated));
        // a huge size must not allocate
        let mut huge = file(1, 1, 0, -1);
        huge[6..10].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse(&huge), Err(GroundError::Truncated));
    }
}
