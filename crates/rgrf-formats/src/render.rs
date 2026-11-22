//! Software renderer: draws one frame of an ACT with its SPR into an RGBA image.
//!
//! Ported from the transform chain of ACT Editor's `LayerDraw.cs` (mirror, move to the centre,
//! scale, rotate, offset) with nearest-neighbour sampling like its preview. The colour mask
//! multiplies the pixels. Several ACTs can be drawn together ([`Part`]), with the later ones bound
//! to the base by their anchors (headgear on a body).
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

/// One ACT with its SPR. In a composition the first part is the base (e.g. the body); the others
/// are drawn on top of it, in order.
#[derive(Clone, Copy)]
pub struct Part<'a> {
    pub act: &'a Act,
    pub spr: &'a Spr,
    /// Move the part so that its first anchor meets the base frame's first anchor (ACT Editor's
    /// `AnchoredTo`: how headgear sits on a head). Ignored for the base and for frames without anchors.
    pub anchored: bool,
    /// Draw the part below the base instead of on top (a reference behind the animation that is
    /// edited). Ignored for the base.
    pub under: bool,
    /// With `anchored`: move the base instead of the part, so that the base's first anchor meets
    /// the part's (ACT Editor's "Use body as base" for garments and wings, where the edited
    /// animation follows the body). Ignored for the base.
    pub base_follows: bool,
}

impl<'a> Part<'a> {
    pub fn new(act: &'a Act, spr: &'a Spr) -> Self {
        Self {
            act,
            spr,
            anchored: false,
            under: false,
            base_follows: false,
        }
    }
}

/// All layers to draw for `frame` of `action`: the parts marked `under`, the base, then the
/// others, each group in order. The base part must have the action and frame; the others are
/// skipped when they lack them (a head has fewer actions than a body).
fn frame_layers<'a>(
    parts: &[Part<'a>],
    action: usize,
    frame: usize,
) -> Result<Vec<Prepared>, RenderError> {
    let frame_of = |p: &Part<'a>| -> Option<&'a crate::act::Frame> {
        p.act.actions.get(action).and_then(|a| a.frames.get(frame))
    };
    let Some(base) = parts.first() else {
        return Ok(Vec::new());
    };
    let Some(base_frame) = frame_of(base) else {
        return if base.act.actions.get(action).is_some() {
            Err(RenderError::NoFrame { action, frame })
        } else {
            Err(RenderError::NoAction(action))
        };
    };
    let base_anchor = base_frame.anchors.first().map(|a| (a.x, a.y));
    let anchor_shift = |n: usize, f: &crate::act::Frame| match (base_anchor, f.anchors.first()) {
        (Some((bx, by)), Some(a)) if parts[n].anchored => {
            Some((f64::from(bx - a.x), f64::from(by - a.y)))
        }
        _ => None,
    };
    // with `base_follows` the base moves to the part's anchor and the part stays where it is
    let mut base_shift = (0.0, 0.0);
    for (n, part) in parts.iter().enumerate().skip(1) {
        if part.base_follows
            && let Some(f) = frame_of(part)
            && let Some((dx, dy)) = anchor_shift(n, f)
        {
            base_shift = (-dx, -dy);
        }
    }
    let order = (1..parts.len())
        .filter(|&n| parts[n].under)
        .chain([0])
        .chain((1..parts.len()).filter(|&n| !parts[n].under));
    let mut out = Vec::new();
    for n in order {
        let part = &parts[n];
        let Some(f) = frame_of(part) else { continue };
        let (dx, dy) = if n == 0 {
            base_shift
        } else if part.base_follows {
            (0.0, 0.0)
        } else {
            anchor_shift(n, f).unwrap_or((0.0, 0.0))
        };
        for l in &f.layers {
            if let Some(mut p) = prepare(l, part.spr) {
                p.ox += dx;
                p.oy += dy;
                out.push(p);
            }
        }
    }
    Ok(out)
}

/// Smallest rectangle holding the layers of the given frames; `None` when nothing is drawn.
/// With several frames (e.g. a whole action) every frame can be rendered on the same canvas.
pub fn bounds_of_parts(
    parts: &[Part<'_>],
    action: usize,
    frames: impl IntoIterator<Item = usize>,
) -> Result<Option<Bounds>, RenderError> {
    let mut all: Option<Bounds> = None;
    for frame in frames {
        for p in frame_layers(parts, action, frame)? {
            let b = p.bounds();
            all = Some(all.map_or(b, |a| a.union(b)));
        }
    }
    Ok(all)
}

/// [`bounds_of_parts`] for a single ACT.
pub fn bounds_of(
    act: &Act,
    spr: &Spr,
    action: usize,
    frames: impl IntoIterator<Item = usize>,
) -> Result<Option<Bounds>, RenderError> {
    bounds_of_parts(&[Part::new(act, spr)], action, frames)
}

/// Renders one frame of a composition onto a canvas of `bounds` (use [`bounds_of_parts`]).
pub fn render_parts(
    parts: &[Part<'_>],
    action: usize,
    frame: usize,
    bounds: Bounds,
) -> Result<Canvas, RenderError> {
    let layers = frame_layers(parts, action, frame)?;
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

/// The frame area of each layer of `frame` (`None` for a layer that draws nothing).
pub fn layer_bounds(act: &Act, spr: &Spr, action: usize, frame: usize) -> Vec<Option<Bounds>> {
    act.actions
        .get(action)
        .and_then(|a| a.frames.get(frame))
        .map(|f| {
            f.layers
                .iter()
                .map(|l| prepare(l, spr).map(|p| p.bounds()))
                .collect()
        })
        .unwrap_or_default()
}

/// The topmost layer of `frame` with a visible pixel at the frame point (`x`, `y`).
pub fn layer_at(
    act: &Act,
    spr: &Spr,
    action: usize,
    frame: usize,
    x: f64,
    y: f64,
) -> Option<usize> {
    let f = act.actions.get(action)?.frames.get(frame)?;
    f.layers.iter().enumerate().rev().find_map(|(i, l)| {
        let p = prepare(l, spr)?;
        let (u, v) = p.inverse(x, y);
        let (iu, iv) = (u.floor() as i64, v.floor() as i64);
        let (w, h) = (i64::from(p.image.width), i64::from(p.image.height));
        if iu < 0 || iv < 0 || iu >= w || iv >= h {
            return None;
        }
        let at = 4 * (iv as usize * w as usize + iu as usize);
        (p.image.pixels[at + 3] > 0 && p.color[3] > 0).then_some(i)
    })
}

/// [`render_parts`] for a single ACT.
pub fn render_frame(
    act: &Act,
    spr: &Spr,
    action: usize,
    frame: usize,
    bounds: Bounds,
) -> Result<Canvas, RenderError> {
    render_parts(&[Part::new(act, spr)], action, frame, bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Anchor, Frame, Layer};
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
    fn anchored_part_moves_so_that_its_anchor_meets_the_base_anchor() {
        let s = spr(vec![IndexedImage {
            width: 2,
            height: 2,
            pixels: vec![1; 4],
        }]);
        let anchor = |x, y| Anchor {
            unknown: [0; 4],
            x,
            y,
            other: 0,
        };
        let mut base = act(vec![layer(0)]);
        base.actions[0].frames[0].anchors = vec![anchor(10, 4)];
        let mut hat = act(vec![layer(0)]);
        hat.actions[0].frames[0].anchors = vec![anchor(1, 1)];
        let plain = [Part::new(&base, &s), Part::new(&hat, &s)];
        let b = bounds_of_parts(&plain, 0, [0]).unwrap().unwrap();
        assert_eq!(
            (b.min_x, b.max_x),
            (-1, 1),
            "not anchored: on top of each other"
        );
        let parts = [
            Part::new(&base, &s),
            Part {
                anchored: true,
                ..Part::new(&hat, &s)
            },
        ];
        let b = bounds_of_parts(&parts, 0, [0]).unwrap().unwrap();
        // the hat moves by (10 - 1, 4 - 1) = (9, 3)
        assert_eq!((b.min_x, b.max_x, b.min_y, b.max_y), (-1, 10, -1, 4));
        let c = render_parts(&parts, 0, 0, b).unwrap();
        assert_eq!(px(&c, 0, 0)[3], 255);
        assert_eq!(px(&c, 10, 3)[3], 255);
        // "use as base": the base meets the part instead, which stays at the origin
        let follow = [
            Part::new(&base, &s),
            Part {
                anchored: true,
                base_follows: true,
                ..Part::new(&hat, &s)
            },
        ];
        let b = bounds_of_parts(&follow, 0, [0]).unwrap().unwrap();
        assert_eq!((b.min_x, b.max_x, b.min_y, b.max_y), (-10, 1, -4, 1));
        // a part without the action is skipped, a base without it is an error
        let short = act(vec![]);
        let two = [Part::new(&base, &s), Part::new(&short, &s)];
        assert!(bounds_of_parts(&two, 0, [0]).is_ok());
        assert_eq!(
            bounds_of_parts(&[Part::new(&base, &s)], 4, [0]),
            Err(RenderError::NoAction(4))
        );
    }

    #[test]
    fn a_part_under_the_base_is_drawn_first() {
        let red = spr(vec![IndexedImage {
            width: 1,
            height: 1,
            pixels: vec![1],
        }]);
        let green = spr(vec![IndexedImage {
            width: 1,
            height: 1,
            pixels: vec![2],
        }]);
        let a = act(vec![layer(0)]);
        let top = |under| {
            let parts = [
                Part::new(&a, &red),
                Part {
                    under,
                    ..Part::new(&a, &green)
                },
            ];
            let b = bounds_of_parts(&parts, 0, [0]).unwrap().unwrap();
            px(&render_parts(&parts, 0, 0, b).unwrap(), 0, 0)
        };
        assert_eq!(top(false), [0, 255, 0, 255], "on top of the base");
        assert_eq!(top(true), [255, 0, 0, 255], "below the base");
    }

    #[test]
    fn the_layer_under_a_point_is_the_topmost_visible_one() {
        // 2 x 2 images: 0 all red, 1 red only at the top left (index 0 is transparent)
        let s = spr(vec![
            IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![1; 4],
            },
            IndexedImage {
                width: 2,
                height: 2,
                pixels: vec![1, 0, 0, 0],
            },
        ]);
        let mut top = layer(1);
        top.offset_x = 0.0;
        let a = act(vec![layer(0), top]);
        // a 2 x 2 image covers -1..1; the top layer is opaque only in its top left pixel
        assert_eq!(layer_at(&a, &s, 0, 0, -0.5, -0.5), Some(1));
        assert_eq!(
            layer_at(&a, &s, 0, 0, 0.5, 0.5),
            Some(0),
            "transparent there: the one below"
        );
        assert_eq!(layer_at(&a, &s, 0, 0, 5.0, 5.0), None);
        assert_eq!(layer_at(&a, &s, 9, 0, 0.0, 0.0), None);
        let b = layer_bounds(&a, &s, 0, 0);
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].map(|b| (b.min_x, b.max_x)), Some((-1, 1)));
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
