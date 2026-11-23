//! RSM / RSM2: 3D models of the client. This is a structural reader: it walks the whole file
//! (meshes, vertices, faces, key frames, volume boxes), keeps the names, the texture lists and the
//! counts, and skips the geometry itself. It cannot write a model. Reading the whole file to its
//! last byte is what proves the layout right, so [`Rsm::trailing`] reports what is left over.
//!
//! Layout (from GRF Editor's `Rsm.cs`/`Mesh.cs`, checked against real files): `"GRSM"`, major,
//! minor, `i32` animation length, `i32` shade type, from 1.4 a byte alpha; then, by version:
//! before 2.2 16 reserved bytes, the texture list (`i32` count, 40-byte names) and the main mesh
//! name (40 bytes); 2.2 an `f32` frame rate, the texture list and the main mesh names with `i32`
//! length prefixes; 2.3 the frame rate and the main mesh names (textures live in each mesh).
//! Version 2.1 is stored like 2.2 except that faces have no length in front (24 bytes as before
//! 2.2). Measured: the only two such files in kRO, `engine\biggear_s_01.rsm2` and
//! `engine\position_j_01.rsm2` (used by the maps `vr_hero00` and `vr_hero01`), read to their
//! last byte that way and no other combination does; see [`layout`].
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::version::Version;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RsmError {
    #[error("not an RSM file (missing \"GRSM\" signature)")]
    BadMagic,
    #[error("unsupported RSM version {0}")]
    UnsupportedVersion(Version),
    #[error("RSM data ends too early (needed {need} bytes at offset {at}, file has {len})")]
    Truncated { at: usize, need: usize, len: usize },
    #[error("{what} count {count} at offset {at} does not fit into the file")]
    BadCount {
        what: &'static str,
        count: i64,
        at: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshInfo {
    pub name: Vec<u8>,
    pub parent: Vec<u8>,
    /// From version 2.3: the mesh's own texture list. Before that: empty (see [`Rsm::textures`]).
    pub textures: Vec<Vec<u8>>,
    pub vertices: usize,
    pub faces: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rsm {
    pub version: Version,
    pub animation_length: i32,
    pub shade_type: i32,
    pub alpha: u8,
    pub frame_rate: f32,
    /// Model-wide texture list (versions before 2.3), as stored (Latin-1 view of the bytes).
    pub textures: Vec<Vec<u8>>,
    pub meshes: Vec<MeshInfo>,
    pub volume_boxes: usize,
    /// Bytes after the last known part (0 in a file whose layout is read correctly).
    pub trailing: usize,
}

pub(crate) struct Reader<'a> {
    pub(crate) d: &'a [u8],
    pub(crate) pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], RsmError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.d.len());
        let Some(end) = end else {
            return Err(RsmError::Truncated {
                at: self.pos,
                need: n,
                len: self.d.len(),
            });
        };
        let s = &self.d[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    pub(crate) fn skip(&mut self, n: usize) -> Result<(), RsmError> {
        self.take(n).map(|_| ())
    }
    pub(crate) fn i32(&mut self) -> Result<i32, RsmError> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn f32(&mut self) -> Result<f32, RsmError> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    pub(crate) fn remaining(&self) -> usize {
        self.d.len() - self.pos
    }
    /// A count of items of at least `min` bytes each.
    pub(crate) fn count(&mut self, what: &'static str, min: usize) -> Result<usize, RsmError> {
        let at = self.pos;
        let n = self.i32()?;
        usize::try_from(n)
            .ok()
            .filter(|&c| c <= self.remaining() / min.max(1))
            .ok_or(RsmError::BadCount {
                what,
                count: i64::from(n),
                at,
            })
    }
    /// A string with an `i32` length in front.
    pub(crate) fn lstr(&mut self) -> Result<Vec<u8>, RsmError> {
        let n = self.count("string", 1)?;
        Ok(cut(self.take(n)?))
    }
    /// A NUL-padded string in a fixed field.
    pub(crate) fn fstr(&mut self, n: usize) -> Result<Vec<u8>, RsmError> {
        Ok(cut(self.take(n)?))
    }
}

pub(crate) fn cut(raw: &[u8]) -> Vec<u8> {
    raw.iter().take_while(|&&b| b != 0).copied().collect()
}

pub(crate) const V1_2: Version = Version::new(1, 2);
pub(crate) const V1_3: Version = Version::new(1, 3);
pub(crate) const V1_4: Version = Version::new(1, 4);
pub(crate) const V1_5: Version = Version::new(1, 5);
pub(crate) const V1_6: Version = Version::new(1, 6);
pub(crate) const V2_2: Version = Version::new(2, 2);
pub(crate) const V2_3: Version = Version::new(2, 3);

/// The version whose layout a file of version `v` has: 2.1 is laid out like 2.2 (but for the
/// faces, which follow the stored version).
pub(crate) fn layout(v: Version) -> Version {
    if v == Version::new(2, 1) { V2_2 } else { v }
}

impl Rsm {
    pub fn parse(bytes: &[u8]) -> Result<Self, RsmError> {
        if bytes.len() < 6 || (&bytes[..4] != b"GRSM" && &bytes[..4] != b"GRSX") {
            return Err(RsmError::BadMagic);
        }
        let stored = Version::new(bytes[4], bytes[5]);
        if stored < Version::new(1, 0) || stored > V2_3 {
            return Err(RsmError::UnsupportedVersion(stored));
        }
        let version = layout(stored);
        let mut r = Reader { d: bytes, pos: 6 };
        let animation_length = r.i32()?;
        let shade_type = r.i32()?;
        let alpha = if version >= V1_4 { r.take(1)?[0] } else { 255 };
        let mut frame_rate = 0.0;
        let mut textures = Vec::new();
        let n_meshes = if version >= V2_3 {
            frame_rate = r.f32()?;
            let n = r.count("main mesh name", 4)?;
            for _ in 0..n {
                r.lstr()?;
            }
            r.count("mesh", 8)?
        } else if version >= V2_2 {
            frame_rate = r.f32()?;
            let n = r.count("texture", 4)?;
            for _ in 0..n {
                textures.push(r.lstr()?);
            }
            let n = r.count("main mesh name", 4)?;
            for _ in 0..n {
                r.lstr()?;
            }
            r.count("mesh", 8)?
        } else {
            r.skip(16)?;
            let n = r.count("texture", 40)?;
            for _ in 0..n {
                textures.push(r.fstr(40)?);
            }
            r.skip(40)?; // main mesh name
            r.count("mesh", 80)?
        };

        let mut meshes = Vec::with_capacity(n_meshes.min(256));
        for _ in 0..n_meshes {
            meshes.push(read_mesh(&mut r, stored)?);
        }
        if version < V1_6 {
            let n = r.count("scale key frame", 20)?;
            r.skip(n * 20)?;
        }
        // Version 1.5 files carry one more list here that GRF Editor does not read (measured: all
        // 490 models of that version end exactly with it): position key frames of the whole model.
        if version >= V1_5 && version < V1_6 {
            let n = r.count("position key frame", 20)?;
            r.skip(n * 20)?;
        }
        let mut volume_boxes = 0;
        if r.remaining() >= 4 {
            let size = if version >= V1_3 { 40 } else { 36 };
            volume_boxes = r.count("volume box", size)?;
            r.skip(volume_boxes * size)?;
        }
        Ok(Self {
            version: stored,
            animation_length,
            shade_type,
            alpha,
            frame_rate,
            textures,
            meshes,
            volume_boxes,
            trailing: r.remaining(),
        })
    }

    /// Every distinct texture the model names, sorted (paths below `data\texture\`).
    pub fn all_textures(&self) -> Vec<Vec<u8>> {
        let mut v: Vec<Vec<u8>> = self.textures.clone();
        for m in &self.meshes {
            v.extend(m.textures.iter().cloned());
        }
        v.retain(|t| !t.is_empty());
        v.sort();
        v.dedup();
        v
    }
}

fn read_mesh(r: &mut Reader<'_>, stored: Version) -> Result<MeshInfo, RsmError> {
    let v = layout(stored);
    let (name, parent) = if v >= V2_2 {
        (r.lstr()?, r.lstr()?)
    } else {
        (r.fstr(40)?, r.fstr(40)?)
    };
    let mut textures = Vec::new();
    if v >= V2_3 {
        let n = r.count("mesh texture", 4)?;
        for _ in 0..n {
            textures.push(r.lstr()?);
        }
    } else {
        let n = r.count("texture index", 4)?;
        r.skip(n * 4)?;
    }
    r.skip(9 * 4 + 3 * 4)?; // transformation matrix, local position
    if v < V2_2 {
        r.skip(10 * 4)?; // global position, rotation angle and axis, scale
    }
    let vertices = r.count("vertex", 12)?;
    r.skip(vertices * 12)?;
    let tv = r.count("texture vertex", 8)?;
    r.skip(tv * if v >= V1_2 { 12 } else { 8 })?;
    let faces = r.count("face", 20)?;
    for _ in 0..faces {
        if stored >= V2_2 {
            let len = r.count("face length", 1)?;
            r.skip(len.max(24))?;
        } else {
            r.skip(if v >= V1_2 { 24 } else { 20 })?;
        }
    }
    if v >= V1_6 {
        let n = r.count("scale key frame", 20)?;
        r.skip(n * 20)?;
    }
    let n = r.count("rotation key frame", 20)?;
    r.skip(n * 20)?;
    if v >= V2_2 {
        let n = r.count("position key frame", 20)?;
        r.skip(n * 20)?;
    }
    if v >= V2_3 {
        let n = r.count("texture animation", 8)?;
        for _ in 0..n {
            r.skip(4)?; // texture id
            let kinds = r.count("texture animation kind", 8)?;
            for _ in 0..kinds {
                r.skip(4)?; // type
                let frames = r.count("texture key frame", 8)?;
                r.skip(frames * 8)?;
            }
        }
    }
    Ok(MeshInfo {
        name,
        parent,
        textures,
        vertices,
        faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A model with one mesh of one triangle, built for `version`.
    fn model(stored: Version) -> Vec<u8> {
        let version = layout(stored);
        let mut b = b"GRSM".to_vec();
        b.extend([stored.major, stored.minor]);
        b.extend(100i32.to_le_bytes()); // animation length
        b.extend(1i32.to_le_bytes()); // shade type
        if version >= V1_4 {
            b.push(255);
        }
        let fixed = |s: &str, n: usize| {
            let mut a = vec![0u8; n];
            a[..s.len()].copy_from_slice(s.as_bytes());
            a
        };
        let lstr = |s: &str| {
            let mut a = (s.len() as i32).to_le_bytes().to_vec();
            a.extend(s.as_bytes());
            a
        };
        if version >= V2_3 {
            b.extend(30.0f32.to_le_bytes());
            b.extend(1i32.to_le_bytes());
            b.extend(lstr("root"));
        } else if version >= V2_2 {
            b.extend(30.0f32.to_le_bytes());
            b.extend(1i32.to_le_bytes());
            b.extend(lstr("a.bmp"));
            b.extend(1i32.to_le_bytes());
            b.extend(lstr("root"));
        } else {
            b.extend([0u8; 16]);
            b.extend(1i32.to_le_bytes());
            b.extend(fixed("a.bmp", 40));
            b.extend(fixed("root", 40));
        }
        b.extend(1i32.to_le_bytes()); // one mesh
        if version >= V2_2 {
            b.extend(lstr("root"));
            b.extend(lstr(""));
        } else {
            b.extend(fixed("root", 40));
            b.extend(fixed("", 40));
        }
        if version >= V2_3 {
            b.extend(1i32.to_le_bytes());
            b.extend(lstr("mesh\\b.bmp"));
        } else {
            b.extend(1i32.to_le_bytes());
            b.extend(0i32.to_le_bytes());
        }
        b.extend(vec![0u8; 9 * 4 + 3 * 4]);
        if version < V2_2 {
            b.extend(vec![0u8; 40]);
        }
        b.extend(3i32.to_le_bytes()); // vertices
        b.extend(vec![0u8; 36]);
        b.extend(3i32.to_le_bytes()); // texture vertices
        b.extend(vec![0u8; 3 * if version >= V1_2 { 12 } else { 8 }]);
        b.extend(1i32.to_le_bytes()); // faces
        if stored >= V2_2 {
            b.extend(40i32.to_le_bytes());
            b.extend(vec![0u8; 40]);
        } else {
            b.extend(vec![0u8; if version >= V1_2 { 24 } else { 20 }]);
        }
        if version >= V1_6 {
            b.extend(0i32.to_le_bytes());
        }
        b.extend(1i32.to_le_bytes()); // one rotation key frame
        b.extend(vec![0u8; 20]);
        if version >= V2_2 {
            b.extend(0i32.to_le_bytes()); // position key frames of the mesh
        }
        if version >= V2_3 {
            b.extend(1i32.to_le_bytes()); // one texture animation
            b.extend(0i32.to_le_bytes());
            b.extend(1i32.to_le_bytes());
            b.extend(0i32.to_le_bytes());
            b.extend(2i32.to_le_bytes());
            b.extend(vec![0u8; 16]);
        }
        if version < V1_6 {
            b.extend(0i32.to_le_bytes()); // scale key frames of the model
        }
        if version >= V1_5 && version < V1_6 {
            b.extend(0i32.to_le_bytes()); // position key frames of the model
        }
        b.extend(1i32.to_le_bytes()); // one volume box
        b.extend(vec![0u8; if version >= V1_3 { 40 } else { 36 }]);
        b
    }

    #[test]
    fn reads_every_version_to_the_last_byte() {
        for (maj, min) in [(1, 1), (1, 3), (1, 4), (1, 5), (1, 6), (2, 2), (2, 3)] {
            let v = Version::new(maj, min);
            let r = Rsm::parse(&model(v)).unwrap_or_else(|e| panic!("{v}: {e}"));
            assert_eq!(r.trailing, 0, "version {v}");
            assert_eq!(r.meshes.len(), 1);
            assert_eq!((r.meshes[0].vertices, r.meshes[0].faces), (3, 1));
            assert_eq!(r.volume_boxes, 1);
            assert_eq!(r.meshes[0].name, b"root");
            let want: &[&[u8]] = if v >= V2_3 {
                &[b"mesh\\b.bmp"]
            } else {
                &[b"a.bmp"]
            };
            assert_eq!(r.all_textures(), want, "version {v}");
        }
    }

    #[test]
    fn version_2_1_is_laid_out_like_2_2_with_fixed_faces() {
        // measured: engine\biggear_s_01.rsm2 and engine\position_j_01.rsm2 of kRO (maps vr_hero00/01)
        let f = model(Version::new(2, 1));
        assert_eq!(&f[15..19], &30.0f32.to_le_bytes(), "frame rate as in 2.2");
        let r = Rsm::parse(&f).unwrap();
        assert_eq!(
            (r.version, r.trailing, r.meshes[0].faces),
            (Version::new(2, 1), 0, 1)
        );
        assert_eq!(r.all_textures(), [b"a.bmp"]);
        let m = crate::rsm_model::parse(&f).unwrap();
        assert_eq!((m.version, m.nodes.len(), m.frame_rate), (V2_2, 1, 30.0));
    }

    #[test]
    fn garbage_is_refused_without_panic() {
        assert_eq!(Rsm::parse(b""), Err(RsmError::BadMagic));
        assert_eq!(Rsm::parse(b"GRAT\x01\x04aaaa"), Err(RsmError::BadMagic));
        assert!(matches!(
            Rsm::parse(b"GRSM\x09\x09aaaaaaaaaaaaaaaaaaa"),
            Err(RsmError::UnsupportedVersion(_))
        ));
        for (maj, min) in [(1, 4), (2, 2), (2, 3)] {
            let good = model(Version::new(maj, min));
            for cut in 0..good.len() {
                let _ = Rsm::parse(&good[..cut]);
            }
        }
        // a texture count that cannot fit
        let mut bad = model(Version::new(1, 4));
        bad[15 + 16..15 + 16 + 4].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(matches!(Rsm::parse(&bad), Err(RsmError::BadCount { .. })));
    }
}
