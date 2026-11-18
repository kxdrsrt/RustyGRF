//! RSW: a map's world file: which ground and altitude files it uses, water, light and every object
//! (models, lights, sounds, effects).
//!
//! Layout (read from GRF Editor's `Rsw*.cs`, checked against real files, see `docs/09-formats.md`):
//! `"GRSW"`, major, minor; from 2.5 an `i32` build number; from 2.2 one byte; four 40-byte names
//! (ini, ground, from 1.4 altitude, source); water (from 1.3, not from 2.6 on) and light (from 1.5)
//! and ground bounds (from 1.6); from 2.7 an `i32` count and that many `i32` values; then the
//! objects. Whatever follows the objects (the quad tree of versions from 2.1) is kept as bytes.
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::version::Version;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RswError {
    #[error("not an RSW file (missing \"GRSW\" signature)")]
    BadMagic,
    #[error("RSW data ends too early (needed {need} bytes at offset {at}, file has {len})")]
    Truncated { at: usize, need: usize, len: usize },
    #[error("object {index} has the unknown type {kind}")]
    UnknownObject { index: usize, kind: i32 },
    #[error("{what} count {count} does not fit into the file")]
    BadCount { what: &'static str, count: i64 },
}

fn v(major: u8, minor: u8) -> Version {
    Version::new(major, minor)
}

pub type Vec3 = [f32; 3];

#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    pub level: f32,
    /// From 1.8.
    pub kind: i32,
    pub wave_height: f32,
    pub wave_speed: f32,
    pub wave_pitch: f32,
    /// From 1.9.
    pub texture_cycling: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub longitude: i32,
    pub latitude: i32,
    pub diffuse: Vec3,
    pub ambient: Vec3,
    /// From 1.7.
    pub intensity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelObject {
    pub name: [u8; 40],
    pub animation_type: i32,
    pub animation_speed: f32,
    pub block_type: i32,
    /// Version 2.6 with build 186 or later.
    pub unknown_byte: u8,
    /// From version 2.7.
    pub unknown_int: i32,
    /// File below `data\model\`.
    pub model_name: [u8; 80],
    pub node_name: [u8; 80],
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LightObject {
    pub name: [u8; 80],
    pub position: Vec3,
    pub color: Vec3,
    pub range: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundObject {
    pub name: [u8; 80],
    /// File below `data\wav\`.
    pub wave_name: [u8; 80],
    pub position: Vec3,
    pub volume: f32,
    pub width: i32,
    pub height: i32,
    pub range: f32,
    /// From version 2.0.
    pub cycle: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectObject {
    pub name: [u8; 80],
    pub position: Vec3,
    pub effect_number: i32,
    pub emit_speed: f32,
    pub params: [f32; 4],
}

#[derive(Debug, Clone, PartialEq)]
pub enum Object {
    Model(Box<ModelObject>),
    Light(LightObject),
    Sound(SoundObject),
    Effect(EffectObject),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rsw {
    pub version: Version,
    pub build_number: i32,
    pub unknown_byte: u8,
    pub ini: [u8; 40],
    pub ground: [u8; 40],
    pub altitude: [u8; 40],
    pub source: [u8; 40],
    pub water: Option<Water>,
    pub light: Option<Light>,
    /// Top, bottom, left, right (from 1.6; GRF Editor notes the values are leftovers).
    pub bounds: Option<[i32; 4]>,
    /// Version 2.7: the `i32` values announced by a count before the objects.
    pub extra_ints: Vec<i32>,
    pub objects: Vec<Object>,
    /// Bytes after the objects (the quad tree), kept untouched.
    pub tail: Vec<u8>,
}

/// Text of a NUL-padded name field (Latin-1).
pub fn text(raw: &[u8]) -> String {
    raw.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect()
}

struct Reader<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], RswError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.d.len());
        let Some(end) = end else {
            return Err(RswError::Truncated {
                at: self.pos,
                need: n,
                len: self.d.len(),
            });
        };
        let s = &self.d[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], RswError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn u8(&mut self) -> Result<u8, RswError> {
        Ok(self.take(1)?[0])
    }
    fn i32(&mut self) -> Result<i32, RswError> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn f32(&mut self) -> Result<f32, RswError> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    fn vec3(&mut self) -> Result<Vec3, RswError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
}

impl Rsw {
    pub fn parse(bytes: &[u8]) -> Result<Self, RswError> {
        if bytes.len() < 6 || &bytes[..4] != b"GRSW" {
            return Err(RswError::BadMagic);
        }
        let version = Version::new(bytes[4], bytes[5]);
        let mut r = Reader { d: bytes, pos: 6 };
        let build_number = if version >= v(2, 5) { r.i32()? } else { 0 };
        let unknown_byte = if version >= v(2, 2) { r.u8()? } else { 1 };
        let ini = r.arr()?;
        let ground = r.arr()?;
        let altitude = if version >= v(1, 4) {
            r.arr()?
        } else {
            [0; 40]
        };
        let source = r.arr()?;

        let water = if version >= v(1, 3) && version < v(2, 6) {
            let mut w = Water {
                level: r.f32()?,
                kind: 0,
                wave_height: 0.0,
                wave_speed: 0.0,
                wave_pitch: 0.0,
                texture_cycling: 0,
            };
            if version >= v(1, 8) {
                w.kind = r.i32()?;
                w.wave_height = r.f32()?;
                w.wave_speed = r.f32()?;
                w.wave_pitch = r.f32()?;
                if version >= v(1, 9) {
                    w.texture_cycling = r.i32()?;
                }
            }
            Some(w)
        } else {
            None
        };
        let light = if version >= v(1, 5) {
            let mut l = Light {
                longitude: r.i32()?,
                latitude: r.i32()?,
                diffuse: r.vec3()?,
                ambient: r.vec3()?,
                intensity: 0.0,
            };
            if version >= v(1, 7) {
                l.intensity = r.f32()?;
            }
            Some(l)
        } else {
            None
        };
        let bounds = if version >= v(1, 6) {
            Some([r.i32()?, r.i32()?, r.i32()?, r.i32()?])
        } else {
            None
        };
        let mut extra_ints = Vec::new();
        if version >= v(2, 7) {
            let n = r.i32()?;
            let n = usize::try_from(n)
                .ok()
                .filter(|&n| n <= (bytes.len() - r.pos) / 4)
                .ok_or(RswError::BadCount {
                    what: "extra value",
                    count: i64::from(n),
                })?;
            for _ in 0..n {
                extra_ints.push(r.i32()?);
            }
        }

        let count = r.i32()?;
        // The smallest object (type + light) is 4 + 80 + 12 + 12 + 4 bytes.
        let count = usize::try_from(count)
            .ok()
            .filter(|&c| c <= (bytes.len() - r.pos) / 100)
            .ok_or(RswError::BadCount {
                what: "object",
                count: i64::from(count),
            })?;
        let mut objects = Vec::with_capacity(count);
        for index in 0..count {
            let kind = r.i32()?;
            objects.push(match kind {
                1 => Object::Model(Box::new(read_model(&mut r, version, build_number)?)),
                2 => Object::Light(LightObject {
                    name: r.arr()?,
                    position: r.vec3()?,
                    color: r.vec3()?,
                    range: r.f32()?,
                }),
                3 => {
                    let mut s = SoundObject {
                        name: r.arr()?,
                        wave_name: r.arr()?,
                        position: r.vec3()?,
                        volume: r.f32()?,
                        width: r.i32()?,
                        height: r.i32()?,
                        range: r.f32()?,
                        cycle: 4.0,
                    };
                    if version >= v(2, 0) {
                        s.cycle = r.f32()?;
                    }
                    Object::Sound(s)
                }
                4 => Object::Effect(EffectObject {
                    name: r.arr()?,
                    position: r.vec3()?,
                    effect_number: r.i32()?,
                    emit_speed: r.f32()?,
                    params: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
                }),
                kind => return Err(RswError::UnknownObject { index, kind }),
            });
        }
        let tail = bytes[r.pos..].to_vec();
        Ok(Self {
            version,
            build_number,
            unknown_byte,
            ini,
            ground,
            altitude,
            source,
            water,
            light,
            bounds,
            extra_ints,
            objects,
            tail,
        })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let ver = self.version;
        let mut o = Vec::new();
        let f = |o: &mut Vec<u8>, x: f32| o.extend(x.to_le_bytes());
        let i = |o: &mut Vec<u8>, x: i32| o.extend(x.to_le_bytes());
        let v3 = |o: &mut Vec<u8>, x: Vec3| x.iter().for_each(|c| o.extend(c.to_le_bytes()));
        o.extend(b"GRSW");
        o.extend([ver.major, ver.minor]);
        if ver >= v(2, 5) {
            i(&mut o, self.build_number);
        }
        if ver >= v(2, 2) {
            o.push(self.unknown_byte);
        }
        o.extend(self.ini);
        o.extend(self.ground);
        if ver >= v(1, 4) {
            o.extend(self.altitude);
        }
        o.extend(self.source);
        if let Some(w) = &self.water
            && ver >= v(1, 3)
            && ver < v(2, 6)
        {
            f(&mut o, w.level);
            if ver >= v(1, 8) {
                i(&mut o, w.kind);
                f(&mut o, w.wave_height);
                f(&mut o, w.wave_speed);
                f(&mut o, w.wave_pitch);
                if ver >= v(1, 9) {
                    i(&mut o, w.texture_cycling);
                }
            }
        }
        if let Some(l) = &self.light
            && ver >= v(1, 5)
        {
            i(&mut o, l.longitude);
            i(&mut o, l.latitude);
            v3(&mut o, l.diffuse);
            v3(&mut o, l.ambient);
            if ver >= v(1, 7) {
                f(&mut o, l.intensity);
            }
        }
        if let Some(b) = &self.bounds
            && ver >= v(1, 6)
        {
            b.iter().for_each(|&x| i(&mut o, x));
        }
        if ver >= v(2, 7) {
            i(&mut o, self.extra_ints.len() as i32);
            self.extra_ints.iter().for_each(|&x| i(&mut o, x));
        }
        i(&mut o, self.objects.len() as i32);
        for obj in &self.objects {
            match obj {
                Object::Model(m) => {
                    i(&mut o, 1);
                    if ver >= v(1, 3) {
                        o.extend(m.name);
                        i(&mut o, m.animation_type);
                        f(&mut o, m.animation_speed);
                        i(&mut o, m.block_type);
                        if ver >= v(2, 6) && self.build_number >= 186 {
                            o.push(m.unknown_byte);
                        }
                        if ver >= v(2, 7) {
                            i(&mut o, m.unknown_int);
                        }
                    }
                    o.extend(m.model_name);
                    o.extend(m.node_name);
                    v3(&mut o, m.position);
                    v3(&mut o, m.rotation);
                    v3(&mut o, m.scale);
                }
                Object::Light(l) => {
                    i(&mut o, 2);
                    o.extend(l.name);
                    v3(&mut o, l.position);
                    v3(&mut o, l.color);
                    f(&mut o, l.range);
                }
                Object::Sound(s) => {
                    i(&mut o, 3);
                    o.extend(s.name);
                    o.extend(s.wave_name);
                    v3(&mut o, s.position);
                    f(&mut o, s.volume);
                    i(&mut o, s.width);
                    i(&mut o, s.height);
                    f(&mut o, s.range);
                    if ver >= v(2, 0) {
                        f(&mut o, s.cycle);
                    }
                }
                Object::Effect(e) => {
                    i(&mut o, 4);
                    o.extend(e.name);
                    v3(&mut o, e.position);
                    i(&mut o, e.effect_number);
                    f(&mut o, e.emit_speed);
                    e.params.iter().for_each(|&x| f(&mut o, x));
                }
            }
        }
        o.extend(&self.tail);
        o
    }

    /// Counts of models, lights, sounds, effects.
    pub fn object_counts(&self) -> (usize, usize, usize, usize) {
        let mut c = (0, 0, 0, 0);
        for o in &self.objects {
            match o {
                Object::Model(_) => c.0 += 1,
                Object::Light(_) => c.1 += 1,
                Object::Sound(_) => c.2 += 1,
                Object::Effect(_) => c.3 += 1,
            }
        }
        c
    }

    /// Distinct model files (below `data\model\`), sorted.
    pub fn model_files(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .objects
            .iter()
            .filter_map(|o| match o {
                Object::Model(m) => Some(text(&m.model_name)),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Distinct sound files (below `data\wav\`), sorted.
    pub fn sound_files(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .objects
            .iter()
            .filter_map(|o| match o {
                Object::Sound(s) => Some(text(&s.wave_name)),
                _ => None,
            })
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

fn read_model(r: &mut Reader<'_>, ver: Version, build: i32) -> Result<ModelObject, RswError> {
    let mut m = ModelObject {
        name: [0; 40],
        animation_type: 0,
        animation_speed: 1.0,
        block_type: 0,
        unknown_byte: 0,
        unknown_int: 0,
        model_name: [0; 80],
        node_name: [0; 80],
        position: [0.0; 3],
        rotation: [0.0; 3],
        scale: [1.0; 3],
    };
    if ver >= v(1, 3) {
        m.name = r.arr()?;
        m.animation_type = r.i32()?;
        m.animation_speed = r.f32()?;
        m.block_type = r.i32()?;
        if ver >= v(2, 6) && build >= 186 {
            m.unknown_byte = r.u8()?;
        }
        if ver >= v(2, 7) {
            m.unknown_int = r.i32()?;
        }
    }
    m.model_name = r.arr()?;
    m.node_name = r.arr()?;
    m.position = r.vec3()?;
    m.rotation = r.vec3()?;
    m.scale = r.vec3()?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name<const N: usize>(s: &str) -> [u8; N] {
        let mut a = [0u8; N];
        a[..s.len()].copy_from_slice(s.as_bytes());
        a
    }

    fn sample(version: Version, build: i32) -> Rsw {
        let model = Object::Model(Box::new(ModelObject {
            name: name("tree"),
            animation_type: 1,
            animation_speed: 2.0,
            block_type: 3,
            unknown_byte: if version >= v(2, 6) && build >= 186 {
                9
            } else {
                0
            },
            unknown_int: if version >= v(2, 7) { 7 } else { 0 },
            model_name: name("tree.rsm"),
            node_name: name("node"),
            position: [1.0, 2.0, 3.0],
            rotation: [0.0, 90.0, 0.0],
            scale: [1.0, 1.0, 1.0],
        }));
        Rsw {
            version,
            build_number: if version >= v(2, 5) { build } else { 0 },
            unknown_byte: if version >= v(2, 2) { 3 } else { 1 },
            ini: name("a.ini"),
            ground: name("map.gnd"),
            altitude: if version >= v(1, 4) {
                name("map.gat")
            } else {
                [0; 40]
            },
            source: name("src"),
            water: (version >= v(1, 3) && version < v(2, 6)).then(|| Water {
                level: 1.5,
                kind: if version >= v(1, 8) { 2 } else { 0 },
                wave_height: if version >= v(1, 8) { 0.5 } else { 0.0 },
                wave_speed: if version >= v(1, 8) { 2.0 } else { 0.0 },
                wave_pitch: if version >= v(1, 8) { 50.0 } else { 0.0 },
                texture_cycling: if version >= v(1, 9) { 3 } else { 0 },
            }),
            light: (version >= v(1, 5)).then(|| Light {
                longitude: 45,
                latitude: 45,
                diffuse: [1.0, 0.9, 0.8],
                ambient: [0.3, 0.3, 0.3],
                intensity: if version >= v(1, 7) { 0.5 } else { 0.0 },
            }),
            bounds: (version >= v(1, 6)).then_some([-500, 500, -500, 500]),
            extra_ints: if version >= v(2, 7) {
                vec![1, 2]
            } else {
                vec![]
            },
            objects: vec![
                model,
                Object::Light(LightObject {
                    name: name("l"),
                    position: [0.0; 3],
                    color: [1.0; 3],
                    range: 20.0,
                }),
                Object::Sound(SoundObject {
                    name: name("s"),
                    wave_name: name("rain.wav"),
                    position: [0.0; 3],
                    volume: 1.0,
                    width: 2,
                    height: 2,
                    range: 10.0,
                    cycle: if version >= v(2, 0) { 5.0 } else { 4.0 },
                }),
                Object::Effect(EffectObject {
                    name: name("e"),
                    position: [5.0; 3],
                    effect_number: 974,
                    emit_speed: 1.0,
                    params: [0.0, 1.0, 2.0, 3.0],
                }),
            ],
            tail: vec![1, 2, 3, 4],
        }
    }

    #[test]
    fn round_trip_all_real_versions() {
        for (maj, min, build) in [
            (1, 9, 0),
            (2, 1, 0),
            (2, 2, 0),
            (2, 5, 0),
            (2, 6, 0),
            (2, 6, 186),
            (2, 7, 186),
            (1, 3, 0),
            (1, 5, 0),
        ] {
            let r = sample(v(maj, min), build);
            let bytes = r.to_bytes();
            let back = Rsw::parse(&bytes).unwrap_or_else(|e| panic!("{maj}.{min}: {e}"));
            assert_eq!(back, r, "version {maj}.{min}");
            assert_eq!(back.to_bytes(), bytes);
        }
    }

    #[test]
    fn lists_files_and_counts() {
        let r = sample(v(2, 1), 0);
        assert_eq!(r.object_counts(), (1, 1, 1, 1));
        assert_eq!(r.model_files(), vec!["tree.rsm"]);
        assert_eq!(r.sound_files(), vec!["rain.wav"]);
        assert_eq!(text(&r.ground), "map.gnd");
        assert_eq!(text(&r.altitude), "map.gat");
    }

    #[test]
    fn garbage_is_refused_without_panic() {
        assert_eq!(Rsw::parse(b""), Err(RswError::BadMagic));
        assert_eq!(Rsw::parse(b"GRATxxxxxxxx"), Err(RswError::BadMagic));
        let good = sample(v(2, 1), 0).to_bytes();
        for cut in 0..good.len() {
            // the quad tree tail is optional, so a cut inside it may still parse; none may panic
            let _ = Rsw::parse(&good[..cut]);
        }
        // a file whose only object has an unknown type
        let mut empty = sample(v(2, 1), 0);
        empty.objects.clear();
        empty.tail.clear();
        let mut raw = empty.to_bytes();
        raw.truncate(raw.len() - 4); // drop the object count 0
        raw.extend(1i32.to_le_bytes());
        raw.extend(99i32.to_le_bytes());
        raw.extend(vec![0u8; 200]);
        assert!(matches!(
            Rsw::parse(&raw),
            Err(RswError::UnknownObject { index: 0, kind: 99 })
        ));
        // a count that cannot fit the file
        let mut raw = empty.to_bytes();
        let n = raw.len();
        raw[n - 4..].copy_from_slice(&1_000_000i32.to_le_bytes());
        assert!(matches!(Rsw::parse(&raw), Err(RswError::BadCount { .. })));
    }
}
