//! The effects of ACT Editor's `EffectsMenu.cs` that make new pictures in the sprite: "Cut sprite"
//! (a cone is cut out of every layer, with a ragged red edge), "Ash/Bleeding outline" (a growing
//! and shrinking fringe above the top edge of the sprite) and the additive "Fade in/out color"
//! (a white copy of the layer, coloured and faded on top of it). Ported with permission, see
//! `docs/05-licensing.md`.
//!
//! Differences: the random numbers come from our own generator (the same seed does not give the
//! same pattern as in ACT Editor), pictures are changed as true colour and mapped back to the
//! palette of a palette sprite (the nearest colours, instead of adding the colour to the palette),
//! the layer picture is placed by its offset without scale and rotation, and the outline's extra
//! rows are cut off again as far as they stay empty.

use std::collections::HashMap;

use crate::act::{Act, Layer, V2_0};
use crate::act_ops::{EffectScope, ease, frame_span};
use crate::edit::{EditError, quantize};
use crate::spr::{PALETTE_LEN, RgbaImage, Spr};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutOptions {
    /// Where the cone starts, in the animation's coordinates (y up, as ACT Editor asks).
    pub source: (f32, f32),
    pub angle: f32,
    pub half_angle: f32,
    pub seed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutlineOptions {
    pub min_height: i32,
    pub max_height: i32,
    /// Share of the columns without fringe (0 to 1).
    pub dead_rate: f32,
    pub color: [u8; 4],
    pub seed: u64,
}

/// A small generator (xorshift), enough for the patterns.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 >> 11
    }

    /// A number in `min..max` (`min` when the range is empty).
    fn range(&mut self, min: i32, max: i32) -> i32 {
        if max <= min {
            return min;
        }
        min + (self.next() % (max - min) as u64) as i32
    }
}

/// Where the picture of a layer starts in the animation's coordinates.
fn origin(l: &Layer, w: u16, h: u16) -> (i32, i32) {
    (
        (l.offset_x - f32::from(w) / 2.0).floor() as i32,
        (l.offset_y - f32::from(h) / 2.0).floor() as i32,
    )
}

fn layer_image(spr: &Spr, l: &Layer) -> Option<RgbaImage> {
    let i = usize::try_from(l.sprite_index).ok()?;
    match l.sprite_type {
        0 => spr.indexed_to_rgba(i),
        _ => spr.rgba.get(i).cloned(),
    }
}

/// The picture as a new image of the sprite, of the layer's kind; returns its number.
fn push_image(spr: &mut Spr, kind: i32, img: RgbaImage) -> Result<i32, EditError> {
    if spr.indexed.len() + spr.rgba.len() >= usize::from(u16::MAX) {
        return Err(EditError::TooManyImages);
    }
    if kind == 0 {
        let pal = *spr.palette.clone().ok_or(EditError::NoPalette)?;
        let q = quantize(&img, &pal);
        Ok(spr.push_indexed(q)? as i32)
    } else {
        Ok(spr.push_rgba(img) as i32)
    }
}

fn is_clear(img: &RgbaImage, x: usize, y: usize) -> bool {
    img.pixels[(y * usize::from(img.width) + x) * 4 + 3] == 0
}

/// The box (min x, min y, max x, max y) the layers of all frames of an action cover.
fn action_box(act: &Act, spr: &Spr, action: usize) -> Option<(i32, i32, i32, i32)> {
    let mut b: Option<(i32, i32, i32, i32)> = None;
    for l in act.actions[action].frames.iter().flat_map(|f| &f.layers) {
        let Some(img) = layer_image(spr, l) else {
            continue;
        };
        let (x, y) = origin(l, img.width, img.height);
        let (x2, y2) = (x + i32::from(img.width) - 1, y + i32::from(img.height) - 1);
        b = Some(match b {
            None => (x, y, x2, y2),
            Some((a, bb, c, d)) => (a.min(x), bb.min(y), c.max(x2), d.max(y2)),
        });
    }
    b
}

fn layer_wanted(scope: &EffectScope, li: usize, all: bool) -> bool {
    all || if scope.layers.is_empty() {
        li == 0
    } else {
        scope.layers.contains(&li)
    }
}

/// "Cut sprite": a cone from `source` is cut out of the pictures of every layer of the chosen
/// animations; pixels at its border get red tones. Returns the number of actions changed.
pub fn cut_sprite(
    act: &mut Act,
    spr: &mut Spr,
    o: &CutOptions,
    scope: &EffectScope,
) -> Result<usize, EditError> {
    const BORDER: [[u8; 4]; 3] = [[255, 0, 0, 255], [200, 0, 0, 255], [150, 0, 0, 255]];
    let mut rng = Rng::new(o.seed);
    let mut changed = 0;
    for aid in 0..act.actions.len() {
        if !scope.groups.contains(&(aid / 8)) || act.actions[aid].frames.is_empty() {
            continue;
        }
        let Some((bx, by, bx2, by2)) = action_box(act, spr, aid) else {
            continue;
        };
        let (w, h) = ((bx2 - bx + 1) as usize, (by2 - by + 1) as usize);
        let (px, py) = (
            (o.source.0 - bx as f32) as i32,
            (-o.source.1 - by as f32) as i32,
        );
        let mask = cone(w, h, px, py, o.angle, o.half_angle);
        let (_, need) = frame_span(&mut act.actions[aid], scope);
        // the same picture is cut once and shared by the frames that show it
        let mut done: HashMap<(i32, i32, i32, i32, i32), i32> = HashMap::new();
        // the loop also borrows the whole action and the sprite, so it indexes
        #[allow(clippy::needless_range_loop)]
        for fi in scope.start..need {
            for li in 0..act.actions[aid].frames[fi].layers.len() {
                let l = act.actions[aid].frames[fi].layers[li].clone();
                let Some(img) = layer_image(spr, &l) else {
                    continue;
                };
                let (x0, y0) = origin(&l, img.width, img.height);
                let key = (l.sprite_type, l.sprite_index, x0, y0, 0);
                let new = match done.get(&key) {
                    Some(&n) => n,
                    None => {
                        let mut out = img.clone();
                        let (sx, sy) = ((x0 - bx) as isize, (y0 - by) as isize);
                        for y in 0..usize::from(img.height) {
                            for x in 0..usize::from(img.width) {
                                let (xx, yy) = (x as isize + sx, y as isize + sy);
                                let at = |x: isize, y: isize| -> bool {
                                    x < 0
                                        || y < 0
                                        || x >= w as isize
                                        || y >= h as isize
                                        || mask[y as usize * w + x as usize]
                                };
                                if xx < 0 || yy < 0 || xx >= w as isize || yy >= h as isize {
                                    continue;
                                }
                                if !mask[yy as usize * w + xx as usize] || is_clear(&img, x, y) {
                                    continue;
                                }
                                let o4 = (y * usize::from(img.width) + x) * 4;
                                if at(xx - 1, yy)
                                    && at(xx + 1, yy)
                                    && at(xx, yy - 1)
                                    && at(xx, yy + 1)
                                {
                                    out.pixels[o4..o4 + 4].copy_from_slice(&[0, 0, 0, 0]);
                                } else {
                                    let c = BORDER[rng.range(0, BORDER.len() as i32) as usize];
                                    out.pixels[o4..o4 + 4].copy_from_slice(&c);
                                }
                            }
                        }
                        let n = push_image(spr, l.sprite_type, out)?;
                        done.insert(key, n);
                        n
                    }
                };
                act.actions[aid].frames[fi].layers[li].sprite_index = new;
            }
        }
        changed += 1;
    }
    Ok(changed)
}

/// The cells inside the cone between the two directions `angle ± half_angle` (degrees) from (px, py).
fn cone(w: usize, h: usize, px: i32, py: i32, angle: f32, half: f32) -> Vec<bool> {
    let (left, right) = ((angle - half).to_radians(), (angle + half).to_radians());
    let ln = (left.sin(), -left.cos());
    let rn = (-right.sin(), right.cos());
    let mut mask = vec![false; w * h];
    for y in 0..h {
        let dy = (y as i32 - py) as f32;
        for x in 0..w {
            let dx = (x as i32 - px) as f32;
            if dx * ln.0 + dy * ln.1 >= 0.0 && dx * rn.0 + dy * rn.1 >= 0.0 {
                mask[y * w + x] = true;
            }
        }
    }
    mask
}

struct Stripe {
    height: i32,
    dir: i32,
}

fn new_stripe(rng: &mut Rng, prev: Option<&Stripe>, min: i32, max: i32) -> Stripe {
    let mut height = rng.range(min, max);
    let mut tries = 3;
    while prev.is_some_and(|p| p.height == height) && tries > 1 {
        height = rng.range(min, max);
        tries -= 1;
    }
    Stripe {
        height,
        dir: (rng.next() % 2) as i32 * 2 - 1,
    }
}

/// "Ash/Bleeding outline": a fringe of `color` grows and shrinks over the top edge of the
/// pictures of the chosen layers, column by column, from frame to frame. Returns the number of
/// actions changed.
pub fn bleeding_outline(
    act: &mut Act,
    spr: &mut Spr,
    o: &OutlineOptions,
    scope: &EffectScope,
) -> Result<usize, EditError> {
    let (min, max) = (
        o.min_height.min(o.max_height).max(0),
        o.min_height.max(o.max_height).max(1),
    );
    let pad = max as usize;
    let mut rng = Rng::new(o.seed);
    let mut changed = 0;
    for aid in 0..act.actions.len() {
        if !scope.groups.contains(&(aid / 8)) || act.actions[aid].frames.is_empty() {
            continue;
        }
        let Some((bx, _, bx2, _)) = action_box(act, spr, aid) else {
            continue;
        };
        let w = (bx2 - bx + 1) as usize;
        let mut stripes: Vec<Stripe> = Vec::with_capacity(w);
        for i in 0..w {
            let s = new_stripe(&mut rng, stripes.get(i.wrapping_sub(1)), min, max);
            stripes.push(s);
        }
        // columns without fringe, in blocks of five
        let mut dead = (o.dead_rate.clamp(0.0, 1.0) * w as f32) as i32;
        while dead > 0 && stripes.iter().any(|s| s.dir != 0) {
            let x = rng.range(0, (w as i32 - 1).max(1)) as usize;
            for i in 0..5 {
                let s = &mut stripes[(x + i) % w];
                if s.dir != 0 {
                    s.height = min;
                    s.dir = 0;
                    dead -= 1;
                }
                if dead <= 0 {
                    break;
                }
            }
        }
        let (_, need) = frame_span(&mut act.actions[aid], scope);
        // the loop also borrows the whole action and the sprite, so it indexes
        #[allow(clippy::needless_range_loop)]
        for fi in scope.start..need {
            for s in stripes.iter_mut().filter(|s| s.dir != 0) {
                if s.height >= max {
                    s.dir = -1;
                } else if s.height <= min {
                    s.dir = 1;
                }
                s.height += s.dir;
            }
            for li in 0..act.actions[aid].frames[fi].layers.len() {
                let l = act.actions[aid].frames[fi].layers[li].clone();
                if !layer_wanted(scope, li, false) {
                    continue;
                }
                let Some(img) = layer_image(spr, &l) else {
                    continue;
                };
                let (x0, _) = origin(&l, img.width, img.height);
                let start = x0 - bx;
                let (iw, ih) = (usize::from(img.width), usize::from(img.height));
                // room above and below, so that the picture keeps its centre
                let (nh, shift) = (ih + 2 * pad, pad);
                let mut out = vec![0u8; iw * nh * 4];
                for y in 0..ih {
                    out[((y + shift) * iw) * 4..((y + shift + 1) * iw) * 4]
                        .copy_from_slice(&img.pixels[y * iw * 4..(y + 1) * iw * 4]);
                }
                let big = RgbaImage {
                    width: img.width,
                    height: nh as u16,
                    pixels: out,
                };
                let mut out = big.clone();
                for x in 0..iw {
                    let col = start + x as i32;
                    let mut found = false;
                    for y in 0..nh {
                        if !found && !is_clear(&big, x, y) {
                            // columns beyond the box get a new stripe, like in ACT Editor
                            while col >= stripes.len() as i32 {
                                let s = new_stripe(&mut rng, stripes.last(), min, max);
                                stripes.push(s);
                            }
                            let height = stripes
                                .get(col.max(0) as usize)
                                .map_or(0, |s| s.height)
                                .max(0);
                            for yy in 0..height as usize {
                                if y >= yy {
                                    let at = ((y - yy) * iw + x) * 4;
                                    out.pixels[at..at + 4].copy_from_slice(&o.color);
                                }
                            }
                            found = true;
                        } else if found && is_clear(&big, x, y) {
                            found = false;
                        }
                    }
                }
                // cut the rows that stayed empty at the top and at the bottom, equally
                let used = |y: usize| (0..iw).any(|x| !is_clear(&out, x, y));
                let top = (0..nh).take_while(|&y| !used(y)).count();
                let bottom = (0..nh).rev().take_while(|&y| !used(y)).count();
                let cut = top.min(bottom).min((nh - 1) / 2);
                let cropped = RgbaImage {
                    width: img.width,
                    height: (nh - 2 * cut) as u16,
                    pixels: out.pixels[cut * iw * 4..(nh - cut) * iw * 4].to_vec(),
                };
                let new = push_image(spr, l.sprite_type, cropped)?;
                let layer = &mut act.actions[aid].frames[fi].layers[li];
                layer.sprite_index = new;
            }
        }
        changed += 1;
    }
    Ok(changed)
}

/// The additive "Fade in/out color": a white copy of each chosen layer's picture is put on top of
/// the layer, coloured with `color` and as opaque as the fade says (up to the colour's alpha in
/// the middle of the scope). Returns the number of actions changed.
pub fn additive_overlay(
    act: &mut Act,
    spr: &mut Spr,
    color: [u8; 4],
    ease_amount: i32,
    scope: &EffectScope,
) -> Result<usize, EditError> {
    let mut whites: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    let mut changed = 0;
    for aid in 0..act.actions.len() {
        if !scope.groups.contains(&(aid / 8)) || act.actions[aid].frames.is_empty() {
            continue;
        }
        let (len, need) = frame_span(&mut act.actions[aid], scope);
        for (step, fi) in (scope.start..need).enumerate() {
            let t = step as f32 / len as f32;
            let t = ease(ease_amount, if t > 0.5 { (1.0 - t) * 2.0 } else { t * 2.0 });
            let old = std::mem::take(&mut act.actions[aid].frames[fi].layers);
            let mut layers = Vec::with_capacity(old.len() * 2);
            for (li, l) in old.into_iter().enumerate() {
                let wanted = layer_wanted(scope, li, false);
                let white = if wanted {
                    white_copy(act.version >= V2_0, spr, &l, &mut whites)?
                } else {
                    None
                };
                layers.push(l.clone());
                if let Some((kind, index)) = white {
                    let mut n = l;
                    n.sprite_type = kind;
                    n.sprite_index = index;
                    n.color = [
                        color[0],
                        color[1],
                        color[2],
                        (f32::from(color[3]) * t) as u8,
                    ];
                    layers.push(n);
                }
            }
            act.actions[aid].frames[fi].layers = layers;
        }
        changed += 1;
    }
    Ok(changed)
}

/// A copy of the layer's picture with every visible pixel white: a palette image shares an unused
/// (or white) palette entry, else it becomes a true-colour image (which needs ACT 2.0).
fn white_copy(
    true_colour_ok: bool,
    spr: &mut Spr,
    l: &Layer,
    cache: &mut HashMap<(i32, i32), (i32, i32)>,
) -> Result<Option<(i32, i32)>, EditError> {
    let key = (l.sprite_type, l.sprite_index);
    if let Some(&c) = cache.get(&key) {
        return Ok(Some(c));
    }
    let Some(img) = layer_image(spr, l) else {
        return Ok(None);
    };
    if l.sprite_type == 0
        && let Some(slot) = white_slot(spr)
    {
        let src = &spr.indexed[l.sprite_index as usize];
        let out = crate::spr::IndexedImage {
            width: src.width,
            height: src.height,
            pixels: src
                .pixels
                .iter()
                .map(|&p| if p == 0 { 0 } else { slot })
                .collect(),
        };
        if spr.indexed.len() + spr.rgba.len() >= usize::from(u16::MAX) {
            return Err(EditError::TooManyImages);
        }
        let n = spr.push_indexed(out)? as i32;
        cache.insert(key, (0, n));
        return Ok(Some((0, n)));
    }
    if !true_colour_ok {
        return Err(EditError::ActTooOld);
    }
    let mut white = img;
    for p in white.pixels.as_chunks_mut::<4>().0 {
        if p[3] != 0 {
            *p = [255, 255, 255, p[3]];
        }
    }
    let n = push_image(spr, 1, white)?;
    cache.insert(key, (1, n));
    Ok(Some((1, n)))
}

/// A palette entry that is white, or one no image uses (made white).
fn white_slot(spr: &mut Spr) -> Option<u8> {
    let pal = spr.palette.as_mut()?;
    let white = (1..256).find(|&i| pal[4 * i..4 * i + 3] == [255, 255, 255]);
    if let Some(i) = white {
        return Some(i as u8);
    }
    let mut used = [false; 256];
    for p in spr.indexed.iter().flat_map(|i| &i.pixels) {
        used[usize::from(*p)] = true;
    }
    let pal = spr.palette.as_mut()?;
    let free = (1..256).find(|&i| !used[i])?;
    pal[4 * free..4 * free + 3].copy_from_slice(&[255, 255, 255]);
    debug_assert_eq!(pal.len(), PALETTE_LEN);
    Some(free as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Frame, V2_5};
    use crate::spr::{IndexedImage, Version};
    use std::collections::BTreeSet;

    fn setup() -> (Act, Spr) {
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[200, 100, 50, 0]);
        pal[8..12].copy_from_slice(&[10, 20, 30, 0]);
        let spr = Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 10,
                height: 10,
                pixels: vec![1; 100],
            }],
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        };
        let layer = Layer {
            sprite_index: 0,
            offset_x: 0.0,
            offset_y: 0.0,
            ..Layer::default()
        };
        let act = Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![Action {
                frames: vec![
                    Frame {
                        layers: vec![layer],
                        ..Frame::default()
                    };
                    3
                ],
                interval: 4.0,
            }],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        };
        (act, spr)
    }

    fn scope() -> EffectScope {
        EffectScope {
            groups: BTreeSet::from([0]),
            layers: BTreeSet::new(),
            start: 0,
            length: 0,
            loop_missing: true,
            add_empty: false,
        }
    }

    #[test]
    fn a_cone_is_cut_out_with_a_border_and_the_frames_share_the_picture() {
        let (mut act, mut spr) = setup();
        // from the middle of the picture, pointing down (y up is negative for the source)
        let o = CutOptions {
            source: (0.0, 0.0),
            angle: 0.0,
            half_angle: 30.0,
            seed: 7,
        };
        assert_eq!(cut_sprite(&mut act, &mut spr, &o, &scope()).unwrap(), 1);
        assert_eq!(spr.indexed.len(), 2, "one cut picture for the three frames");
        let l = &act.actions[0].frames[2].layers[0];
        assert_eq!(l.sprite_index, 1);
        let img = &spr.indexed[1];
        let clear = img.pixels.iter().filter(|&&p| p == 0).count();
        let kept = img.pixels.iter().filter(|&&p| p == 1).count();
        // the cut border takes the palette colour nearest to red, which is not the sprite's own
        let border = img.pixels.iter().filter(|&&p| p == 2).count();
        assert!(clear > 0 && kept > 0, "{clear} {kept} {border}");
        // the original stays as it was
        assert!(spr.indexed[0].pixels.iter().all(|&p| p == 1));
        // a scope that names no animation changes nothing
        let mut none = scope();
        none.groups = BTreeSet::from([9]);
        assert_eq!(cut_sprite(&mut act, &mut spr, &o, &none).unwrap(), 0);
    }

    #[test]
    fn the_outline_grows_rows_above_the_edge_and_keeps_the_centre() {
        let (mut act, mut spr) = setup();
        let o = OutlineOptions {
            min_height: 1,
            max_height: 3,
            dead_rate: 0.0,
            color: [10, 20, 30, 255],
            seed: 3,
        };
        assert_eq!(
            bleeding_outline(&mut act, &mut spr, &o, &scope()).unwrap(),
            1
        );
        assert_eq!(spr.indexed.len(), 4, "a picture per frame");
        for f in &act.actions[0].frames {
            let img = &spr.indexed[f.layers[0].sprite_index as usize];
            assert_eq!(img.width, 10);
            // rows were added on both sides by the same number: the centre did not move
            assert!(img.height > 10 && img.height % 2 == 0, "{}", img.height);
            // the fringe uses the palette colour nearest to the outline colour (2)
            assert!(img.pixels.contains(&2));
        }
        // nothing is drawn with no fringe at all
        let (mut act, mut spr) = setup();
        let none = OutlineOptions {
            min_height: 0,
            max_height: 1,
            dead_rate: 1.0,
            ..o
        };
        bleeding_outline(&mut act, &mut spr, &none, &scope()).unwrap();
        let l = &act.actions[0].frames[0].layers[0];
        assert_eq!(spr.indexed[l.sprite_index as usize].height, 10);
    }

    #[test]
    fn the_additive_overlay_puts_a_white_copy_on_top_with_a_fading_colour() {
        let (mut act, mut spr) = setup();
        assert_eq!(
            additive_overlay(&mut act, &mut spr, [255, 150, 0, 200], 0, &scope()).unwrap(),
            1
        );
        let alphas: Vec<u8> = act.actions[0]
            .frames
            .iter()
            .map(|f| {
                assert_eq!(f.layers.len(), 2);
                assert_eq!(f.layers[1].sprite_type, 0);
                f.layers[1].color[3]
            })
            .collect();
        assert!(alphas[0] == 0 && alphas[1] > alphas[0], "{alphas:?}");
        assert_eq!(spr.indexed.len(), 2, "one white copy for all frames");
        let slot = spr.indexed[1].pixels[0];
        assert_eq!(
            &spr.palette.as_ref().unwrap()[4 * usize::from(slot)..4 * usize::from(slot) + 3],
            &[255, 255, 255]
        );
        // no free palette entry: a true-colour copy, which an old animation cannot hold
        let (mut act, mut spr) = setup();
        spr.indexed[0] = IndexedImage {
            width: 16,
            height: 16,
            pixels: (0..256).map(|i| (i % 255 + 1) as u8).collect(),
        };
        let mut old = act.clone();
        old.version = crate::Version::new(1, 5);
        assert_eq!(
            additive_overlay(&mut old, &mut spr.clone(), [1, 2, 3, 4], 0, &scope()),
            Err(EditError::ActTooOld)
        );
        additive_overlay(&mut act, &mut spr, [1, 2, 3, 4], 0, &scope()).unwrap();
        assert_eq!(act.actions[0].frames[0].layers[1].sprite_type, 1);
        assert_eq!(spr.rgba.len(), 1);
    }
}
