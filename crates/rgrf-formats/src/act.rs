//! ACT animation files: actions (directions/animations) made of frames, each frame a stack of
//! layers that point into an SPR.
//!
//! Layout (read from GRF Editor's `ActConverter.cs`, checked against real files, see
//! `docs/09-formats.md`): `"AC"`, minor, major, `u16` number of actions, 10 reserved bytes. Each
//! action: `i32` frames. Each frame: 32 bytes (two RECTs, kept as they are), `i32` layers, the
//! layers, from 2.0 an `i32` sound id, from 2.3 `i32` anchors with 16 bytes each. After the actions:
//! from 2.1 `i32` sound names of 40 bytes, from 2.2 one `f32` interval per action.
//! A layer is: offset x, y (`i32`, from 2.6 `f32`), sprite index, mirror (`i32`); from 2.0 colour
//! (R, G, B, A), scale x (`f32`, from 2.4 also scale y), rotation (`i32` degrees), sprite type
//! (`i32`: 0 palette image, 1 true colour); from 2.5 width and height (`i32`).
//!
//! Derived from GRF Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::version::Version;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ActError {
    #[error("not an ACT file (missing \"AC\" signature)")]
    BadMagic,
    #[error("ACT data ends too early (needed {need} bytes at offset {at}, file has {len})")]
    Truncated { at: usize, need: usize, len: usize },
    #[error("unsupported ACT version {0}")]
    UnsupportedVersion(Version),
    #[error("{what} count {count} at offset {at} does not fit into the file")]
    BadCount {
        what: &'static str,
        count: i64,
        at: usize,
    },
    #[error("too many actions ({0}, at most 65535)")]
    TooManyActions(usize),
    #[error("count of {what} does not fit into 32 bits")]
    TooMany { what: &'static str },
}

pub const V2_0: Version = Version::new(2, 0);
pub const V2_1: Version = Version::new(2, 1);
pub const V2_2: Version = Version::new(2, 2);
pub const V2_3: Version = Version::new(2, 3);
pub const V2_4: Version = Version::new(2, 4);
pub const V2_5: Version = Version::new(2, 5);
pub const V2_6: Version = Version::new(2, 6);

/// Interval GRF Editor assumes for actions whose file stores none (versions before 2.2).
pub const DEFAULT_INTERVAL: f32 = 6.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// `i32` in the file before version 2.6, `f32` from 2.6 (whole numbers survive both).
    pub offset_x: f32,
    pub offset_y: f32,
    /// Index into the SPR images of the kind `sprite_type`; `-1` for an empty layer.
    pub sprite_index: i32,
    pub mirror: bool,
    /// R, G, B, A colour mask; white (255) when the version stores none.
    pub color: [u8; 4],
    pub scale_x: f32,
    pub scale_y: f32,
    /// Degrees.
    pub rotation: i32,
    /// 0 palette image, 1 true-colour image.
    pub sprite_type: i32,
    pub width: i32,
    pub height: i32,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            sprite_index: -1,
            mirror: false,
            color: [255; 4],
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0,
            sprite_type: 0,
            width: 0,
            height: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    pub unknown: [u8; 4],
    pub x: i32,
    pub y: i32,
    pub other: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The two RECTs at the start of a frame (attack range), kept untouched.
    pub range: [u8; 32],
    pub layers: Vec<Layer>,
    /// Index into [`Act::sounds`], `-1` for none.
    pub sound_id: i32,
    pub anchors: Vec<Anchor>,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            range: [0; 32],
            layers: Vec::new(),
            sound_id: -1,
            anchors: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub frames: Vec<Frame>,
    /// Frame time in units of 25 ms (so 6 is 150 ms).
    pub interval: f32,
}

impl Default for Action {
    fn default() -> Self {
        Self {
            frames: Vec::new(),
            interval: DEFAULT_INTERVAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Act {
    pub version: Version,
    /// The 10 bytes after the action count, kept untouched.
    pub reserved: [u8; 10],
    pub actions: Vec<Action>,
    /// Sound file names as stored (40 bytes each, NUL padded); see [`sound_name`].
    pub sounds: Vec<[u8; 40]>,
    /// Bytes after the last known field, kept so that a rewrite loses nothing.
    pub trailing: Vec<u8>,
    /// Things the reader tolerated.
    pub warnings: Vec<String>,
}

/// Text of a stored sound name (up to the first NUL, Latin-1).
pub fn sound_name(raw: &[u8; 40]) -> String {
    raw.iter()
        .take_while(|&&b| b != 0)
        .map(|&b| char::from(b))
        .collect()
}

/// Stored form of a sound name; `None` when it is longer than 39 bytes or not Latin-1.
pub fn sound_from_name(name: &str) -> Option<[u8; 40]> {
    let mut raw = [0u8; 40];
    for (n, c) in name.chars().enumerate() {
        let b = u8::try_from(u32::from(c)).ok().filter(|&b| b != 0)?;
        *raw.get_mut(n).filter(|_| n < 39)? = b;
    }
    Some(raw)
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], ActError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len());
        let Some(end) = end else {
            return Err(ActError::Truncated {
                at: self.pos,
                need: n,
                len: self.data.len(),
            });
        };
        let s = &self.data[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn arr<const N: usize>(&mut self) -> Result<[u8; N], ActError> {
        let mut a = [0u8; N];
        a.copy_from_slice(self.take(N)?);
        Ok(a)
    }
    fn i32(&mut self) -> Result<i32, ActError> {
        Ok(i32::from_le_bytes(self.arr()?))
    }
    fn f32(&mut self) -> Result<f32, ActError> {
        Ok(f32::from_le_bytes(self.arr()?))
    }
    /// A count that must leave room for `count * min_item` bytes.
    fn count(&mut self, what: &'static str, min_item: usize) -> Result<usize, ActError> {
        let at = self.pos;
        let n = self.i32()?;
        usize::try_from(n)
            .ok()
            .filter(|&c| c <= self.remaining() / min_item.max(1))
            .ok_or(ActError::BadCount {
                what,
                count: i64::from(n),
                at,
            })
    }
}

fn supported(v: Version) -> bool {
    v.major == 2 && v.minor <= 6
}

fn layer_min_size(v: Version) -> usize {
    16 + if v >= V2_0 {
        4 + 4 + 4 + 4 + if v >= V2_4 { 4 } else { 0 } + if v >= V2_5 { 8 } else { 0 }
    } else {
        0
    }
}

impl Act {
    pub fn parse(bytes: &[u8]) -> Result<Self, ActError> {
        if bytes.len() < 4 || &bytes[..2] != b"AC" {
            return Err(ActError::BadMagic);
        }
        let version = Version::new(bytes[3], bytes[2]);
        if !supported(version) {
            return Err(ActError::UnsupportedVersion(version));
        }
        let mut r = Reader {
            data: bytes,
            pos: 4,
        };
        let n_actions = usize::from(u16::from_le_bytes(r.arr()?));
        let reserved = r.arr::<10>()?;

        let mut actions = Vec::with_capacity(n_actions.min(1024));
        for _ in 0..n_actions {
            let n_frames = r.count("frame", 36)?;
            let mut frames = Vec::with_capacity(n_frames.min(1024));
            for _ in 0..n_frames {
                let range = r.arr::<32>()?;
                let n_layers = r.count("layer", layer_min_size(version))?;
                let mut layers = Vec::with_capacity(n_layers.min(256));
                for _ in 0..n_layers {
                    layers.push(read_layer(&mut r, version)?);
                }
                let sound_id = if version >= V2_0 { r.i32()? } else { -1 };
                let mut anchors = Vec::new();
                if version >= V2_3 {
                    let n = r.count("anchor", 16)?;
                    for _ in 0..n {
                        anchors.push(Anchor {
                            unknown: r.arr()?,
                            x: r.i32()?,
                            y: r.i32()?,
                            other: r.i32()?,
                        });
                    }
                }
                frames.push(Frame {
                    range,
                    layers,
                    sound_id,
                    anchors,
                });
            }
            actions.push(Action {
                frames,
                interval: DEFAULT_INTERVAL,
            });
        }

        let mut warnings = Vec::new();
        let mut sounds = Vec::new();
        if version >= V2_1 {
            let n = r.count("sound", 40)?;
            for _ in 0..n {
                sounds.push(r.arr::<40>()?);
            }
            if version >= V2_2 {
                let mut last = 1.0f32;
                let mut missing = 0usize;
                for a in &mut actions {
                    if r.remaining() >= 4 {
                        last = r.f32()?;
                    } else {
                        missing += 1;
                    }
                    a.interval = last;
                }
                if missing > 0 {
                    warnings.push(format!(
                        "{missing} action intervals are missing, the last one is used"
                    ));
                }
            }
        }
        let trailing = bytes[r.pos..].to_vec();
        if !trailing.is_empty() {
            warnings.push(format!("{} unused bytes at the end", trailing.len()));
        }
        Ok(Self {
            version,
            reserved,
            actions,
            sounds,
            trailing,
            warnings,
        })
    }

    /// Serialises as `version`. Fields the version cannot hold are dropped (sound, anchors, ...).
    pub fn to_bytes(&self, version: Version) -> Result<Vec<u8>, ActError> {
        if !supported(version) {
            return Err(ActError::UnsupportedVersion(version));
        }
        let n_actions = u16::try_from(self.actions.len())
            .map_err(|_| ActError::TooManyActions(self.actions.len()))?;
        let too_many = |what| ActError::TooMany { what };
        let mut out = vec![b'A', b'C', version.minor, version.major];
        out.extend(n_actions.to_le_bytes());
        out.extend(self.reserved);
        for action in &self.actions {
            out.extend(
                i32::try_from(action.frames.len())
                    .map_err(|_| too_many("frames"))?
                    .to_le_bytes(),
            );
            for frame in &action.frames {
                out.extend(frame.range);
                out.extend(
                    i32::try_from(frame.layers.len())
                        .map_err(|_| too_many("layers"))?
                        .to_le_bytes(),
                );
                for l in &frame.layers {
                    write_layer(&mut out, l, version);
                }
                if version >= V2_0 {
                    out.extend(frame.sound_id.to_le_bytes());
                }
                if version >= V2_3 {
                    out.extend(
                        i32::try_from(frame.anchors.len())
                            .map_err(|_| too_many("anchors"))?
                            .to_le_bytes(),
                    );
                    for a in &frame.anchors {
                        out.extend(a.unknown);
                        out.extend(a.x.to_le_bytes());
                        out.extend(a.y.to_le_bytes());
                        out.extend(a.other.to_le_bytes());
                    }
                }
            }
        }
        if version >= V2_1 {
            out.extend(
                i32::try_from(self.sounds.len())
                    .map_err(|_| too_many("sounds"))?
                    .to_le_bytes(),
            );
            for s in &self.sounds {
                out.extend(s);
            }
            if version >= V2_2 {
                for a in &self.actions {
                    out.extend(a.interval.to_le_bytes());
                }
            }
        }
        out.extend(&self.trailing);
        Ok(out)
    }

    /// Every layer with its action, frame and layer index.
    pub fn layers(&self) -> impl Iterator<Item = (usize, usize, usize, &Layer)> {
        self.actions.iter().enumerate().flat_map(|(a, act)| {
            act.frames.iter().enumerate().flat_map(move |(f, fr)| {
                fr.layers
                    .iter()
                    .enumerate()
                    .map(move |(l, ly)| (a, f, l, ly))
            })
        })
    }
}

fn read_layer(r: &mut Reader<'_>, v: Version) -> Result<Layer, ActError> {
    let mut l = Layer::default();
    if v >= V2_6 {
        l.offset_x = r.f32()?;
        l.offset_y = r.f32()?;
    } else {
        l.offset_x = r.i32()? as f32;
        l.offset_y = r.i32()? as f32;
    }
    l.sprite_index = r.i32()?;
    l.mirror = r.i32()? != 0;
    if v >= V2_0 {
        l.color = r.arr()?;
        l.scale_x = r.f32()?;
        l.scale_y = if v >= V2_4 { r.f32()? } else { l.scale_x };
        l.rotation = r.i32()?;
        l.sprite_type = r.i32()?;
        if v >= V2_5 {
            l.width = r.i32()?;
            l.height = r.i32()?;
        }
    }
    Ok(l)
}

fn write_layer(out: &mut Vec<u8>, l: &Layer, v: Version) {
    if v >= V2_6 {
        out.extend(l.offset_x.to_le_bytes());
        out.extend(l.offset_y.to_le_bytes());
    } else {
        out.extend((l.offset_x.round() as i32).to_le_bytes());
        out.extend((l.offset_y.round() as i32).to_le_bytes());
    }
    out.extend(l.sprite_index.to_le_bytes());
    out.extend(i32::from(l.mirror).to_le_bytes());
    if v >= V2_0 {
        out.extend(l.color);
        out.extend(l.scale_x.to_le_bytes());
        if v >= V2_4 {
            out.extend(l.scale_y.to_le_bytes());
        }
        out.extend(l.rotation.to_le_bytes());
        out.extend(l.sprite_type.to_le_bytes());
        if v >= V2_5 {
            out.extend(l.width.to_le_bytes());
            out.extend(l.height.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(version: Version) -> Act {
        let layer = |x: f32, idx: i32| Layer {
            offset_x: x,
            offset_y: -3.0,
            sprite_index: idx,
            mirror: idx % 2 == 1,
            color: [10, 20, 30, 255],
            scale_x: 1.5,
            scale_y: if version >= V2_4 { 0.5 } else { 1.5 },
            rotation: 90,
            sprite_type: 1,
            width: if version >= V2_5 { 16 } else { 0 },
            height: if version >= V2_5 { 24 } else { 0 },
        };
        let frame = |n: i32| Frame {
            range: [n as u8; 32],
            layers: vec![layer(2.0, 0), layer(-5.0, 1)],
            sound_id: if version >= V2_0 { n } else { -1 },
            anchors: if version >= V2_3 {
                vec![Anchor {
                    unknown: [1, 2, 3, 4],
                    x: 5,
                    y: -6,
                    other: 7,
                }]
            } else {
                vec![]
            },
        };
        Act {
            version,
            reserved: [0; 10],
            actions: vec![
                Action {
                    frames: vec![frame(0), frame(1)],
                    interval: if version >= V2_2 {
                        4.0
                    } else {
                        DEFAULT_INTERVAL
                    },
                },
                Action {
                    frames: vec![],
                    interval: if version >= V2_2 {
                        8.0
                    } else {
                        DEFAULT_INTERVAL
                    },
                },
            ],
            sounds: if version >= V2_1 {
                vec![sound_from_name("atk.wav").unwrap()]
            } else {
                vec![]
            },
            trailing: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn round_trip_every_version() {
        for minor in 0..=6 {
            let v = Version::new(2, minor);
            let a = sample(v);
            let bytes = a.to_bytes(v).unwrap();
            let back = Act::parse(&bytes).unwrap();
            assert_eq!(back, a, "version {v}");
            assert_eq!(back.to_bytes(v).unwrap(), bytes, "stable rewrite {v}");
        }
    }

    #[test]
    fn old_version_drops_what_it_cannot_hold() {
        let a = sample(V2_6);
        let back = Act::parse(&a.to_bytes(V2_0).unwrap()).unwrap();
        assert!(back.sounds.is_empty());
        assert!(back.actions[0].frames[0].anchors.is_empty());
        assert_eq!(back.actions[0].frames[0].layers[0].width, 0);
        assert_eq!(back.actions[0].frames[0].layers[0].scale_y, 1.5);
    }

    #[test]
    fn version_2_6_keeps_fractional_offsets() {
        let mut a = sample(V2_6);
        a.actions[0].frames[0].layers[0].offset_x = 2.25;
        let back = Act::parse(&a.to_bytes(V2_6).unwrap()).unwrap();
        assert_eq!(back.actions[0].frames[0].layers[0].offset_x, 2.25);
    }

    #[test]
    fn missing_intervals_and_trailing_bytes_are_reported_and_kept() {
        let a = sample(V2_5);
        let mut bytes = a.to_bytes(V2_5).unwrap();
        bytes.truncate(bytes.len() - 4); // second interval gone
        let back = Act::parse(&bytes).unwrap();
        assert_eq!(back.actions[1].interval, 4.0);
        assert_eq!(back.warnings.len(), 1);
        let mut bytes = a.to_bytes(V2_5).unwrap();
        bytes.extend([9, 9, 9]);
        let back = Act::parse(&bytes).unwrap();
        assert_eq!(back.trailing, vec![9, 9, 9]);
        assert_eq!(back.to_bytes(V2_5).unwrap(), bytes);
    }

    #[test]
    fn garbage_never_panics_or_allocates_wildly() {
        assert_eq!(Act::parse(b""), Err(ActError::BadMagic));
        assert_eq!(Act::parse(b"SP\x01\x02aaaa"), Err(ActError::BadMagic));
        assert!(matches!(
            Act::parse(b"AC\x09\x09\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"),
            Err(ActError::UnsupportedVersion(_))
        ));
        // one action announcing two billion frames
        let mut bad = b"AC\x05\x02\x01\x00".to_vec();
        bad.extend([0u8; 10]);
        bad.extend(i32::MAX.to_le_bytes());
        assert!(matches!(Act::parse(&bad), Err(ActError::BadCount { .. })));
        let good = sample(V2_5).to_bytes(V2_5).unwrap();
        for cut in 0..good.len() {
            let _ = Act::parse(&good[..cut]);
        }
    }

    #[test]
    fn sound_names() {
        let raw = sound_from_name("atk.wav").unwrap();
        assert_eq!(sound_name(&raw), "atk.wav");
        assert!(sound_from_name(&"x".repeat(40)).is_none());
        assert!(sound_from_name(&"x".repeat(39)).is_some());
        assert!(sound_from_name("日本").is_none());
    }

    #[test]
    fn layers_iterator_gives_indexes() {
        let a = sample(V2_5);
        let v: Vec<_> = a.layers().map(|(a, f, l, _)| (a, f, l)).collect();
        assert_eq!(v, vec![(0, 0, 0), (0, 0, 1), (0, 1, 0), (0, 1, 1)]);
    }
}
