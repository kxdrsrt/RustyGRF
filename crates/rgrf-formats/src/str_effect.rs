//! STR effect files (`data\texture\effect\*.str`): reading, the animation of a layer at a point in
//! time, and a software preview renderer.
//!
//! File layout (version 0x94, little endian): `"STRM"`, version, fps, last frame, layer count and
//! 16 unused bytes; per layer a texture count, 128-byte texture names, a key count and the keys
//! (124 bytes each). A key either sets a state (kind 0) or says how the state changes per frame
//! after it (kind 1). The animation rules and the vertex order were checked against the behaviour
//! of roBrowser's `StrEffect` (GPL-3.0, only observed, no code copied).

use std::collections::HashMap;

use crate::render::{Bounds, Canvas};
use crate::spr::RgbaImage;

/// Size of the effect picture the keys are authored for.
pub const STAGE_WIDTH: u32 = 640;
pub const STAGE_HEIGHT: u32 = 480;

const KEY_LEN: usize = 124;
const MAX_LAYERS: usize = 1024;
const MAX_TEXTURES: usize = 256;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StrError {
    #[error("not an STR file")]
    NotStr,
    #[error("unsupported STR version {0:#x} (0x94 is supported)")]
    Version(u32),
    #[error("the STR file is truncated or damaged")]
    Truncated,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub frame: i32,
    /// 0 sets a state, 1 describes the change per frame after the state before it.
    pub kind: u32,
    pub pos: [f32; 2],
    pub uv: [f32; 8],
    /// Four corner x values, then four corner y values.
    pub xy: [f32; 8],
    pub aniframe: f32,
    pub anitype: u32,
    pub delay: f32,
    /// Degrees (the file stores 1024 steps per turn).
    pub angle: f32,
    /// R, G, B, A in 0..1 (the file stores 0..255).
    pub color: [f32; 4],
    pub src_blend: u32,
    pub dst_blend: u32,
    pub preset: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// Texture names as stored (CP949 bytes), relative to `data\texture\effect\`.
    pub textures: Vec<Vec<u8>>,
    /// Sorted by frame (stable).
    pub keys: Vec<Key>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Str {
    pub version: u32,
    pub fps: u32,
    pub last_frame: u32,
    pub layers: Vec<Layer>,
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], StrError> {
        let s = self
            .d
            .get(self.at..self.at.checked_add(n).ok_or(StrError::Truncated)?)
            .ok_or(StrError::Truncated)?;
        self.at += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32, StrError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn i32(&mut self) -> Result<i32, StrError> {
        Ok(self.u32()? as i32)
    }
    fn f32(&mut self) -> Result<f32, StrError> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn floats<const N: usize>(&mut self) -> Result<[f32; N], StrError> {
        let mut out = [0.0; N];
        for v in &mut out {
            *v = self.f32()?;
        }
        Ok(out)
    }
}

pub fn parse(bytes: &[u8]) -> Result<Str, StrError> {
    let mut r = Reader { d: bytes, at: 0 };
    if r.take(4).map_err(|_| StrError::NotStr)? != b"STRM" {
        return Err(StrError::NotStr);
    }
    let version = r.u32()?;
    if version != 0x94 {
        return Err(StrError::Version(version));
    }
    let fps = r.u32()?;
    let last_frame = r.u32()?;
    let n_layers = r.u32()? as usize;
    r.take(16)?;
    if n_layers > MAX_LAYERS {
        return Err(StrError::Truncated);
    }
    let mut layers = Vec::with_capacity(n_layers);
    for _ in 0..n_layers {
        let n_tex = usize::try_from(r.i32()?).map_err(|_| StrError::Truncated)?;
        if n_tex > MAX_TEXTURES {
            return Err(StrError::Truncated);
        }
        let mut textures = Vec::with_capacity(n_tex);
        for _ in 0..n_tex {
            let raw = r.take(128)?;
            let end = raw.iter().position(|&b| b == 0).unwrap_or(128);
            textures.push(raw[..end].to_vec());
        }
        let n_keys = usize::try_from(r.i32()?).map_err(|_| StrError::Truncated)?;
        if r.d.len().saturating_sub(r.at) < n_keys.saturating_mul(KEY_LEN) {
            return Err(StrError::Truncated);
        }
        let mut keys = Vec::with_capacity(n_keys);
        for _ in 0..n_keys {
            let frame = r.i32()?;
            let kind = r.u32()?;
            let pos = r.floats::<2>()?;
            let uv = r.floats::<8>()?;
            let xy = r.floats::<8>()?;
            let aniframe = r.f32()?;
            let anitype = r.u32()?;
            let delay = r.f32()?;
            let angle = r.f32()? / (1024.0 / 360.0);
            let c = r.floats::<4>()?;
            keys.push(Key {
                frame,
                kind,
                pos,
                uv,
                xy,
                aniframe,
                anitype,
                delay,
                angle,
                color: c.map(|v| v / 255.0),
                src_blend: r.u32()?,
                dst_blend: r.u32()?,
                preset: r.u32()?,
            });
        }
        keys.sort_by_key(|k| k.frame);
        layers.push(Layer { textures, keys });
    }
    Ok(Str {
        version,
        fps,
        last_frame,
        layers,
    })
}

/// The state of a layer at one moment.
#[derive(Debug, Clone, PartialEq)]
pub struct Pose {
    pub pos: [f32; 2],
    pub xy: [f32; 8],
    pub angle: f32,
    pub color: [f32; 4],
    /// Index into the layer's textures (may be out of range for damaged files).
    pub texture: f32,
    pub src_blend: u32,
    pub dst_blend: u32,
}

/// What the layer shows at frame `t`, or `None` when it shows nothing then.
pub fn pose(layer: &Layer, t: f32) -> Option<Pose> {
    let n_tex = layer.textures.len() as f32;
    let mut from: Option<usize> = None;
    let mut to: Option<usize> = None;
    let (mut last_frame, mut last_source) = (0, 0);
    for (i, k) in layer.keys.iter().enumerate() {
        if k.frame as f32 <= t {
            match k.kind {
                0 => from = Some(i),
                1 => to = Some(i),
                _ => {}
            }
        }
        last_frame = last_frame.max(k.frame);
        if k.kind == 0 {
            last_source = last_source.max(k.frame);
        }
    }
    let from_i = from?;
    if to.is_none() && (last_frame as f32) < t {
        return None;
    }
    let f = &layer.keys[from_i];
    let delta = t - f.frame as f32;
    let morph = to.filter(|&ti| ti == from_i + 1 && layer.keys[ti].frame == f.frame);
    let Some(ti) = morph else {
        // a state that is not changed by a following key; it ends when no later state exists
        if to.is_some() && last_source <= f.frame {
            return None;
        }
        return Some(Pose {
            pos: f.pos,
            xy: f.xy,
            angle: f.angle,
            color: f.color,
            texture: f.aniframe,
            src_blend: f.src_blend,
            dst_blend: f.dst_blend,
        });
    };
    let k = &layer.keys[ti];
    let mix = |a: f32, b: f32| a + b * delta;
    let texture = match k.anitype {
        1 => mix(f.aniframe, k.aniframe),
        2 => (f.aniframe + k.delay * delta).min(n_tex - 1.0),
        3 if n_tex > 0.0 => (f.aniframe + k.delay * delta).rem_euclid(n_tex),
        4 if n_tex > 0.0 => (f.aniframe - k.delay * delta).rem_euclid(n_tex),
        _ => 0.0,
    };
    Some(Pose {
        pos: [mix(f.pos[0], k.pos[0]), mix(f.pos[1], k.pos[1])],
        xy: std::array::from_fn(|i| mix(f.xy[i], k.xy[i])),
        angle: mix(f.angle, k.angle),
        color: std::array::from_fn(|i| mix(f.color[i], k.color[i])),
        texture,
        src_blend: f.src_blend,
        dst_blend: f.dst_blend,
    })
}

/// Texture images by the name stored in the file.
pub type Textures = HashMap<Vec<u8>, RgbaImage>;

fn factor(code: u32, src: [f32; 4], dst: [f32; 4]) -> [f32; 3] {
    let rgb = |c: [f32; 4]| [c[0], c[1], c[2]];
    let all = |v: f32| [v; 3];
    match code {
        1 => all(0.0),
        3 => rgb(src),
        4 => rgb(src).map(|v| 1.0 - v),
        5 => all(src[3]),
        6 => all(1.0 - src[3]),
        7 => all(dst[3]),
        8 => all(1.0 - dst[3]),
        9 => rgb(dst),
        10 => rgb(dst).map(|v| 1.0 - v),
        11 => all(src[3].min(1.0 - dst[3])),
        _ => all(1.0),
    }
}

fn sample(img: &RgbaImage, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (usize::from(img.width), usize::from(img.height));
    let x = (u * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let px = |xx: usize, yy: usize| {
        let i = 4 * (yy * w + xx);
        [
            f32::from(img.pixels[i]),
            f32::from(img.pixels[i + 1]),
            f32::from(img.pixels[i + 2]),
            f32::from(img.pixels[i + 3]),
        ]
    };
    let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
    std::array::from_fn(|i| {
        let top = a[i] + (b[i] - a[i]) * fx;
        let bottom = c[i] + (d[i] - c[i]) * fx;
        (top + (bottom - top) * fy) / 255.0
    })
}

/// Draws one triangle with UV interpolation and the layer's blend mode.
fn triangle(
    canvas: &mut [f32],
    size: (usize, usize),
    p: [[f32; 2]; 3],
    uv: [[f32; 2]; 3],
    tex: &RgbaImage,
    color: [f32; 4],
    blend: (u32, u32),
) {
    let (w, h) = size;
    let area =
        (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = p
        .iter()
        .map(|q| q[0])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = (p
        .iter()
        .map(|q| q[0])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(w as f32))
    .max(0.0) as usize;
    let min_y = p
        .iter()
        .map(|q| q[1])
        .fold(f32::MAX, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = (p
        .iter()
        .map(|q| q[1])
        .fold(f32::MIN, f32::max)
        .ceil()
        .min(h as f32))
    .max(0.0) as usize;
    for y in min_y..max_y {
        for x in min_x..max_x {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w1 = ((px - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (py - p[0][1]))
                / area;
            let w2 = ((p[1][0] - p[0][0]) * (py - p[0][1]) - (px - p[0][0]) * (p[1][1] - p[0][1]))
                / area;
            let w0 = 1.0 - w1 - w2;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let u = w0 * uv[0][0] + w1 * uv[1][0] + w2 * uv[2][0];
            let v = w0 * uv[0][1] + w1 * uv[1][1] + w2 * uv[2][1];
            let t = sample(tex, u, v);
            let src: [f32; 4] = std::array::from_fn(|i| t[i] * color[i]);
            if src[3] <= 0.0 {
                continue;
            }
            let at = 4 * (y * w + x);
            let dst = [canvas[at], canvas[at + 1], canvas[at + 2], canvas[at + 3]];
            let sf = factor(blend.0, src, dst);
            let df = factor(blend.1, src, dst);
            for c in 0..3 {
                canvas[at + c] = (src[c] * sf[c] + dst[c] * df[c]).clamp(0.0, 1.0);
            }
        }
    }
}

/// Renders the effect at frame `t` on a dark background, `scale` times the stage size
/// (640 x 480). Layers whose texture is missing from `textures` are skipped.
pub fn render(s: &Str, t: f32, textures: &Textures, scale: f32) -> Canvas {
    let w = ((STAGE_WIDTH as f32 * scale).round() as usize).max(1);
    let h = ((STAGE_HEIGHT as f32 * scale).round() as usize).max(1);
    let mut canvas = vec![0.0f32; w * h * 4];
    for px in canvas.as_chunks_mut::<4>().0 {
        px.copy_from_slice(&[0.1, 0.1, 0.12, 1.0]);
    }
    for layer in &s.layers {
        let Some(p) = pose(layer, t) else { continue };
        let idx = p.texture as isize;
        let Some(name) = usize::try_from(idx)
            .ok()
            .and_then(|i| layer.textures.get(i))
        else {
            continue;
        };
        let Some(tex) = textures.get(name) else {
            continue;
        };
        if tex.width == 0 || tex.height == 0 {
            continue;
        }
        // the corners turn around the layer's own origin, then the layer moves to its position
        let (sin, cos) = p.angle.to_radians().sin_cos();
        let corner = |i: usize| {
            let (x, y) = (p.xy[i], p.xy[4 + i]);
            [
                (p.pos[0] + x * cos - y * sin) * scale,
                (p.pos[1] + x * sin + y * cos) * scale,
            ]
        };
        let (a, b, c, d) = (corner(0), corner(1), corner(3), corner(2));
        let blend = (p.src_blend, p.dst_blend);
        // a triangle strip A B C D with the texture corners (0,0) (1,0) (0,1) (1,1)
        triangle(
            &mut canvas,
            (w, h),
            [a, b, c],
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            tex,
            p.color,
            blend,
        );
        triangle(
            &mut canvas,
            (w, h),
            [b, c, d],
            [[1.0, 0.0], [0.0, 1.0], [1.0, 1.0]],
            tex,
            p.color,
            blend,
        );
    }
    Canvas {
        width: w as u32,
        height: h as u32,
        bounds: Bounds {
            min_x: 0,
            min_y: 0,
            max_x: w as i32,
            max_y: h as i32,
        },
        pixels: canvas
            .iter()
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
            .collect(),
    }
}

/// Texture names used by the effect, once each, in order of first use.
pub fn texture_names(s: &Str) -> Vec<Vec<u8>> {
    let mut seen: Vec<Vec<u8>> = Vec::new();
    for l in &s.layers {
        for t in &l.textures {
            if !seen.contains(t) {
                seen.push(t.clone());
            }
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_bytes(k: &Key, out: &mut Vec<u8>) {
        out.extend(k.frame.to_le_bytes());
        out.extend(k.kind.to_le_bytes());
        for v in k.pos.iter().chain(&k.uv).chain(&k.xy) {
            out.extend(v.to_le_bytes());
        }
        out.extend(k.aniframe.to_le_bytes());
        out.extend(k.anitype.to_le_bytes());
        out.extend(k.delay.to_le_bytes());
        out.extend((k.angle * (1024.0 / 360.0)).to_le_bytes());
        for v in k.color {
            out.extend((v * 255.0).to_le_bytes());
        }
        out.extend(k.src_blend.to_le_bytes());
        out.extend(k.dst_blend.to_le_bytes());
        out.extend(k.preset.to_le_bytes());
    }

    fn key(frame: i32, kind: u32) -> Key {
        Key {
            frame,
            kind,
            pos: [320.0, 240.0],
            uv: [0.0; 8],
            xy: [-20.0, 20.0, 20.0, -20.0, -20.0, -20.0, 20.0, 20.0],
            aniframe: 0.0,
            anitype: 0,
            delay: 0.0,
            angle: 0.0,
            color: [1.0; 4],
            src_blend: 5,
            dst_blend: 6,
            preset: 0,
        }
    }

    fn file(layers: &[(Vec<&str>, Vec<Key>)]) -> Vec<u8> {
        let mut v = b"STRM".to_vec();
        for n in [0x94u32, 30, 60, layers.len() as u32] {
            v.extend(n.to_le_bytes());
        }
        v.extend([0; 16]);
        for (tex, keys) in layers {
            v.extend((tex.len() as i32).to_le_bytes());
            for t in tex {
                let mut name = t.as_bytes().to_vec();
                name.resize(128, 0);
                v.extend(name);
            }
            v.extend((keys.len() as i32).to_le_bytes());
            for k in keys {
                key_bytes(k, &mut v);
            }
        }
        v
    }

    #[test]
    fn parses_layers_textures_and_keys() {
        let mut k2 = key(10, 1);
        k2.angle = 90.0;
        let bytes = file(&[(vec!["a.bmp", "b.tga"], vec![key(0, 0), k2])]);
        let s = parse(&bytes).unwrap();
        assert_eq!((s.fps, s.last_frame, s.layers.len()), (30, 60, 1));
        assert_eq!(s.layers[0].textures, [b"a.bmp".to_vec(), b"b.tga".to_vec()]);
        assert_eq!(s.layers[0].keys.len(), 2);
        assert!((s.layers[0].keys[1].angle - 90.0).abs() < 1e-3);
        assert_eq!(texture_names(&s).len(), 2);
    }

    #[test]
    fn bad_files_are_errors() {
        assert_eq!(parse(b"nope"), Err(StrError::NotStr));
        let mut bytes = file(&[(vec![], vec![])]);
        assert!(parse(&bytes).is_ok());
        bytes[4] = 0x95;
        assert_eq!(parse(&bytes), Err(StrError::Version(0x95)));
        let full = file(&[(vec!["a.bmp"], vec![key(0, 0)])]);
        assert_eq!(parse(&full[..full.len() - 5]), Err(StrError::Truncated));
        // a huge key count must not allocate
        let mut huge = file(&[(vec![], vec![])]);
        let n = huge.len();
        huge[n - 4..].copy_from_slice(&i32::MAX.to_le_bytes());
        assert_eq!(parse(&huge), Err(StrError::Truncated));
    }

    #[test]
    fn a_state_followed_by_a_change_moves_per_frame() {
        let mut change = key(0, 1);
        change.pos = [2.0, 0.0];
        change.color = [0.0, 0.0, 0.0, -0.1];
        let s = parse(&file(&[(
            vec!["a.bmp"],
            vec![key(0, 0), change, key(10, 0)],
        )]))
        .unwrap();
        let l = &s.layers[0];
        assert_eq!(pose(l, 0.0).unwrap().pos, [320.0, 240.0]);
        let p = pose(l, 5.0).unwrap();
        assert_eq!(p.pos, [330.0, 240.0]);
        assert!((p.color[3] - 0.5).abs() < 1e-4);
        // after the last key nothing is shown
        assert!(pose(l, 11.0).is_none());
        // before the first key too
        let late = parse(&file(&[(vec!["a.bmp"], vec![key(4, 0)])])).unwrap();
        assert!(pose(&late.layers[0], 1.0).is_none());
    }

    #[test]
    fn texture_animation_types() {
        let mut change = key(0, 1);
        change.anitype = 3; // repeat
        change.delay = 1.0;
        let s = parse(&file(&[(vec!["a", "b", "c"], vec![key(0, 0), change])])).unwrap();
        let l = &s.layers[0];
        assert_eq!(pose(l, 4.0).unwrap().texture, 1.0);
        let mut stop = key(0, 1);
        stop.anitype = 2; // stop at the end
        stop.delay = 1.0;
        let s = parse(&file(&[(vec!["a", "b", "c"], vec![key(0, 0), stop])])).unwrap();
        assert_eq!(pose(&s.layers[0], 9.0).unwrap().texture, 2.0);
    }

    fn white() -> RgbaImage {
        RgbaImage {
            width: 2,
            height: 2,
            pixels: vec![255; 16],
        }
    }

    #[test]
    fn renders_a_quad_with_blending_and_skips_missing_textures() {
        let s = parse(&file(&[
            (vec!["w.bmp"], vec![key(0, 0)]),
            (vec!["missing.bmp"], vec![key(0, 0)]),
        ]))
        .unwrap();
        let mut tex = Textures::new();
        tex.insert(b"w.bmp".to_vec(), white());
        let c = render(&s, 0.0, &tex, 1.0);
        assert_eq!((c.width, c.height), (640, 480));
        let at = |x: usize, y: usize| &c.pixels[4 * (y * 640 + x)..4 * (y * 640 + x) + 3];
        assert_eq!(at(320, 240), [255, 255, 255], "inside the 40 x 40 quad");
        assert_eq!(at(100, 100), [26, 26, 31], "background stays");
        // half size renders the same picture smaller
        assert_eq!(render(&s, 0.0, &tex, 0.5).width, 320);
        // additive blending adds to the background instead of replacing it
        let mut k = key(0, 0);
        k.src_blend = 2;
        k.dst_blend = 2;
        k.color = [0.5, 0.5, 0.5, 1.0];
        let s = parse(&file(&[(vec!["w.bmp"], vec![k])])).unwrap();
        let c = render(&s, 0.0, &tex, 1.0);
        let p = &c.pixels[4 * (240 * 640 + 320)..][..3];
        assert_eq!(p, [153, 153, 158]);
    }

    #[test]
    fn rotation_turns_the_quad_around_its_position() {
        let mut k = key(0, 0);
        k.xy = [0.0, 60.0, 60.0, 0.0, -5.0, -5.0, 5.0, 5.0]; // a bar to the right of the position
        k.angle = 90.0;
        let s = parse(&file(&[(vec!["w.bmp"], vec![k])])).unwrap();
        let mut tex = Textures::new();
        tex.insert(b"w.bmp".to_vec(), white());
        let c = render(&s, 0.0, &tex, 1.0);
        let at = |x: usize, y: usize| c.pixels[4 * (y * 640 + x)];
        assert_eq!(at(320, 270), 255, "turned downwards (clockwise on screen)");
        assert_eq!(at(350, 240), 26, "no longer to the right");
    }
}
