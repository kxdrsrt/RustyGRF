//! Software renderer: draws one frame of an ACT with its SPR into an RGBA image.
//!
//! Ported from the transform chain of ACT Editor's `LayerDraw.cs` (mirror, move to the centre,
//! scale, rotate, offset) with nearest-neighbour sampling like its preview. The colour mask
//! multiplies the pixels. Not done yet: frames anchored to another ACT (`AnchoredTo`, used to put
//! headgear on a body).
//!
//! Derived from ACT Editor (Tokeiburu), used with permission, see `docs/05-licensing.md`.

use crate::act::{Act, Layer};
use crate::spr::{RgbaImage, Spr};

/// Pixel rectangle in frame coordinates (the origin is the sprite's anchor point).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub min_x: i32,
    pub min_y: i32,
    pub max_x: i32,
    pub max_y: i32,
}

impl Bounds {
    pub fn width(&self) -> u32 {
        (i64::from(self.max_x) - i64::from(self.min_x)).clamp(0, i64::from(u32::MAX)) as u32
    }
    pub fn height(&self) -> u32 {
        (i64::from(self.max_y) - i64::from(self.min_y)).clamp(0, i64::from(u32::MAX)) as u32
    }
    fn union(self, o: Self) -> Self {
        Self {
            min_x: self.min_x.min(o.min_x),
            min_y: self.min_y.min(o.min_y),
            max_x: self.max_x.max(o.max_x),
            max_y: self.max_y.max(o.max_y),
        }
    }
}

/// A rendered frame: `pixels` is RGBA, rows top-down, `bounds` places it relative to the origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub bounds: Bounds,
    pub pixels: Vec<u8>,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("the ACT has no action {0}")]
    NoAction(usize),
    #[error("action {action} has no frame {frame}")]
    NoFrame { action: usize, frame: usize },
    #[error(
        "the frame would be {width} x {height} pixels, more than the limit of {MAX_PIXELS} pixels"
    )]
    TooLarge { width: u32, height: u32 },
}

/// Largest canvas `render_frame` allocates (a damaged file can place layers far away).
pub const MAX_PIXELS: u64 = 64 * 1024 * 1024;

/// One layer ready to draw: its image as RGBA and the transform chain.
struct Prepared {
    image: RgbaImage,
    mirror: bool,
    tx: f64,
    ty: f64,
    sx: f64,
    sy: f64,
    cos: f64,
    sin: f64,
    cx: f64,
    cy: f64,
    ox: f64,
    oy: f64,
    color: [u8; 4],
}

fn prepare(layer: &Layer, spr: &Spr) -> Option<Prepared> {
    let index = usize::try_from(layer.sprite_index).ok()?;
    let image = if layer.sprite_type == 0 {
        spr.indexed_to_rgba(index)?
    } else {
        spr.rgba.get(index)?.clone()
    };
    let (w, h) = (i64::from(image.width), i64::from(image.height));
    if w == 0 || h == 0 {
        return None;
    }
    let (sx, sy) = (f64::from(layer.scale_x), f64::from(layer.scale_y));
    if sx == 0.0 || sy == 0.0 || !sx.is_finite() || !sy.is_finite() {
        return None;
    }
    let angle = f64::from(layer.rotation).to_radians();
    Some(Prepared {
        mirror: layer.mirror,
        // C# integer division: -(w + 1) / 2 truncates toward zero
        tx: -((w + 1) / 2) as f64,
        ty: -((h + 1) / 2) as f64,
        sx,
        sy,
        cos: angle.cos(),
        sin: angle.sin(),
        cx: if w % 2 == 1 { -0.5 * sx } else { 0.0 },
        cy: if h % 2 == 1 { -0.5 * sy } else { 0.0 },
        ox: f64::from(layer.offset_x),
        oy: f64::from(layer.offset_y),
        color: layer.color,
        image,
    })
}

impl Prepared {
    /// Image point (pixels, origin top-left) to frame coordinates.
    fn forward(&self, u: f64, v: f64) -> (f64, f64) {
        let w = f64::from(self.image.width);
        let u = if self.mirror { w - u } else { u };
        let (x, y) = ((u + self.tx) * self.sx, (v + self.ty) * self.sy);
        let (dx, dy) = (x - self.cx, y - self.cy);
        (
            self.cx + dx * self.cos - dy * self.sin + self.ox,
            self.cy + dx * self.sin + dy * self.cos + self.oy,
        )
    }

    /// Frame coordinates back to an image point.
    fn inverse(&self, px: f64, py: f64) -> (f64, f64) {
        let (dx, dy) = (px - self.ox - self.cx, py - self.oy - self.cy);
        let x = self.cx + dx * self.cos + dy * self.sin;
        let y = self.cy - dx * self.sin + dy * self.cos;
        let (u, v) = (x / self.sx - self.tx, y / self.sy - self.ty);
        let w = f64::from(self.image.width);
        (if self.mirror { w - u } else { u }, v)
    }

    fn bounds(&self) -> Bounds {
        let (w, h) = (f64::from(self.image.width), f64::from(self.image.height));
        let pts = [
            self.forward(0.0, 0.0),
            self.forward(w, 0.0),
            self.forward(0.0, h),
            self.forward(w, h),
        ];
        let min = |f: fn(&(f64, f64)) -> f64| pts.iter().map(f).fold(f64::INFINITY, f64::min);
        let max = |f: fn(&(f64, f64)) -> f64| pts.iter().map(f).fold(f64::NEG_INFINITY, f64::max);
        Bounds {
            min_x: (min(|p| p.0) + 1e-6).floor() as i32,
            min_y: (min(|p| p.1) + 1e-6).floor() as i32,
            max_x: (max(|p| p.0) - 1e-6).ceil() as i32,
            max_y: (max(|p| p.1) - 1e-6).ceil() as i32,
        }
    }
}

fn frame_layers(
    act: &Act,
    spr: &Spr,
    action: usize,
    frame: usize,
) -> Result<Vec<Prepared>, RenderError> {
    let a = act
        .actions
        .get(action)
        .ok_or(RenderError::NoAction(action))?;
    let f = a
        .frames
        .get(frame)
        .ok_or(RenderError::NoFrame { action, frame })?;
    Ok(f.layers.iter().filter_map(|l| prepare(l, spr)).collect())
}

/// Smallest rectangle holding the layers of the given frames; `None` when nothing is drawn.
/// With several frames (e.g. a whole action) every frame can be rendered on the same canvas.
pub fn bounds_of(
    act: &Act,
    spr: &Spr,
    action: usize,
    frames: impl IntoIterator<Item = usize>,
) -> Result<Option<Bounds>, RenderError> {
    let mut all: Option<Bounds> = None;
    for frame in frames {
        for p in frame_layers(act, spr, action, frame)? {
            let b = p.bounds();
            all = Some(all.map_or(b, |a| a.union(b)));
        }
    }
    Ok(all)
}

/// Renders one frame onto a canvas of `bounds` (use [`bounds_of`]).
pub fn render_frame(
    act: &Act,
    spr: &Spr,
    action: usize,
    frame: usize,
    bounds: Bounds,
) -> Result<Canvas, RenderError> {
    let layers = frame_layers(act, spr, action, frame)?;
    if u64::from(bounds.width()) * u64::from(bounds.height()) > MAX_PIXELS {
        return Err(RenderError::TooLarge {
            width: bounds.width(),
            height: bounds.height(),
        });
    }
    let (cw, ch) = (bounds.width() as usize, bounds.height() as usize);
    let mut px = vec![0u8; cw * ch * 4];
    for p in &layers {
        let b = p.bounds();
        let (w, h) = (i64::from(p.image.width), i64::from(p.image.height));
        for y in b.min_y.max(bounds.min_y)..b.max_y.min(bounds.max_y) {
            for x in b.min_x.max(bounds.min_x)..b.max_x.min(bounds.max_x) {
                let (u, v) = p.inverse(f64::from(x) + 0.5, f64::from(y) + 0.5);
                let (iu, iv) = (u.floor() as i64, v.floor() as i64);
                if iu < 0 || iv < 0 || iu >= w || iv >= h {
                    continue;
                }
                let s = 4 * (iv as usize * w as usize + iu as usize);
                let src = &p.image.pixels[s..s + 4];
                let m = |c: u8, k: u8| (u32::from(c) * u32::from(k) + 127) / 255;
                let sa = m(src[3], p.color[3]) as f32 / 255.0;
                if sa == 0.0 {
                    continue;
                }
                let d = 4 * ((y - bounds.min_y) as usize * cw + (x - bounds.min_x) as usize);
                let da = f32::from(px[d + 3]) / 255.0;
                let oa = sa + da * (1.0 - sa);
                for c in 0..3 {
                    let sc = m(src[c], p.color[c]) as f32;
                    let dc = f32::from(px[d + c]);
                    px[d + c] = ((sc * sa + dc * da * (1.0 - sa)) / oa).round() as u8;
                }
                px[d + 3] = (oa * 255.0).round() as u8;
            }
        }
    }
    Ok(Canvas {
        width: cw as u32,
        height: ch as u32,
        bounds,
        pixels: px,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Frame, Layer};
    use crate::spr::{IndexedImage, PALETTE_LEN, Version};

    fn spr(images: Vec<IndexedImage>) -> Spr {
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[255, 0, 0, 0]); // 1 red
        pal[8..12].copy_from_slice(&[0, 255, 0, 0]); // 2 green
        Spr {
            version: Version::V2_1,
            indexed: images,
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        }
    }

    fn act(layers: Vec<Layer>) -> Act {
        Act {
            version: crate::act::V2_5,
            reserved: [0; 10],
            actions: vec![Action {
                frames: vec![Frame {
                    layers,
                    ..Frame::default()
                }],
                interval: 6.0,
            }],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        }
    }

    fn layer(index: i32) -> Layer {
        Layer {
            sprite_index: index,
            ..Layer::default()
        }
    }

    fn render(a: &Act, s: &Spr) -> Canvas {
        let b = bounds_of(a, s, 0, [0]).unwrap().unwrap();
        render_frame(a, s, 0, 0, b).unwrap()
    }

    fn px(c: &Canvas, x: u32, y: u32) -> [u8; 4] {
        let i = 4 * (y * c.width + x) as usize;
        c.pixels[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn even_sized_image_is_centred_on_the_origin() {
        // 2 x 2: red green / transparent red
        let s = spr(vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1, 2, 0, 1],
        }]);
        let c = render(&act(vec![layer(0)]), &s);
        assert_eq!((c.width, c.height), (2, 2));
        assert_eq!((c.bounds.min_x, c.bounds.min_y), (-1, -1));
        assert_eq!(px(&c, 0, 0), [255, 0, 0, 255]);
        assert_eq!(px(&c, 1, 0), [0, 255, 0, 255]);
        assert_eq!(px(&c, 0, 1)[3], 0);
    }

    #[test]
    fn odd_sized_image_sits_half_a_pixel_to_the_left_and_up() {
        // 3 x 3: x = -(3+1)/2 = -2, so it covers -2..1
        let s = spr(vec![IndexedImage {
            width: 3,
            height: 3,
            pixels: vec![1; 9],
        }]);
        let c = render(&act(vec![layer(0)]), &s);
        assert_eq!((c.bounds.min_x, c.bounds.max_x), (-2, 1));
    }

    #[test]
    fn offset_mirror_scale_and_colour_mask() {
        let s = spr(vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1, 2, 1, 2],
        }]);
        let mut l = layer(0);
        l.mirror = true;
        let c = render(&act(vec![l.clone()]), &s);
        assert_eq!(px(&c, 0, 0), [0, 255, 0, 255], "mirrored: green first");

        l.offset_x = 10.0;
        l.offset_y = -4.0;
        let c = render(&act(vec![l.clone()]), &s);
        assert_eq!((c.bounds.min_x, c.bounds.min_y), (9, -5));

        l.mirror = false;
        l.scale_x = 2.0;
        let c = render(&act(vec![l.clone()]), &s);
        assert_eq!(c.width, 4);
        assert_eq!(px(&c, 1, 0), [255, 0, 0, 255]);
        assert_eq!(px(&c, 2, 0), [0, 255, 0, 255]);

        l.scale_x = 1.0;
        l.color = [128, 255, 255, 128];
        let c = render(&act(vec![l]), &s);
        assert_eq!(px(&c, 0, 0), [128, 0, 0, 128]);
    }

    #[test]
    fn rotation_by_90_degrees_turns_the_image() {
        // 2 x 2: top-left red, rest green; rotating clockwise puts the red top-right
        let s = spr(vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1, 2, 2, 2],
        }]);
        let mut l = layer(0);
        l.rotation = 90;
        let c = render(&act(vec![l]), &s);
        assert_eq!(px(&c, 1, 0), [255, 0, 0, 255]);
        assert_eq!(px(&c, 0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn later_layers_draw_on_top_and_empty_layers_are_skipped() {
        let s = spr(vec![
            IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![1; 4],
            },
            IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![2; 4],
            },
        ]);
        let c = render(&act(vec![layer(0), layer(-1), layer(1), layer(7)]), &s);
        assert_eq!(px(&c, 0, 0), [0, 255, 0, 255]);
    }

    #[test]
    fn far_away_layers_are_refused_not_allocated() {
        let s = spr(vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1; 4],
        }]);
        let mut l = layer(0);
        l.offset_x = 2.0e9;
        let a = act(vec![layer(0), l]);
        let b = bounds_of(&a, &s, 0, [0]).unwrap().unwrap();
        assert!(matches!(
            render_frame(&a, &s, 0, 0, b),
            Err(RenderError::TooLarge { .. })
        ));
    }

    #[test]
    fn bounds_of_a_frame_without_layers_and_bad_indexes() {
        let s = spr(vec![]);
        let a = act(vec![]);
        assert_eq!(bounds_of(&a, &s, 0, [0]).unwrap(), None);
        assert_eq!(bounds_of(&a, &s, 3, [0]), Err(RenderError::NoAction(3)));
        assert_eq!(
            bounds_of(&a, &s, 0, [5]),
            Err(RenderError::NoFrame {
                action: 0,
                frame: 5
            })
        );
    }
}
