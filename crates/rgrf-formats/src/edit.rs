//! Editing operations on SPR, ACT and PAL data (M7b).
//!
//! Everything works on the in-memory structures; the CLI writes the result to a new file.

use crate::act::{Act, Action};
use crate::pal::Pal;
use crate::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EditError {
    #[error("the SPR has no {kind} image {index} ({count} images of that kind)")]
    NoImage {
        kind: &'static str,
        index: usize,
        count: usize,
    },
    #[error("the SPR has no palette (it holds no palette images)")]
    NoPalette,
    #[error("the ACT has no action {index} ({count} actions)")]
    NoAction { index: usize, count: usize },
    #[error("too many actions (at most 65535)")]
    TooManyActions,
    #[error("the ACT is older than version 2.0 and cannot refer to true-colour images")]
    ActTooOld,
    #[error("the sprite has no true-colour images")]
    NoRgba,
    #[error("the sprite has no palette images")]
    NoIndexed,
    #[error("too many images (at most 65535 of one kind)")]
    TooManyImages,
}

/// What `Spr::to_indexed` did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedReport {
    /// Every pixel kept its colour (no colour was merged, no semi-transparent pixel was cut).
    pub lossless: bool,
    /// Colours of the palette in use (without the transparent entry 0).
    pub colours: usize,
    /// Images converted.
    pub images: usize,
}

/// Kind of SPR image; ACT layers use `0` for palette images and `1` for true-colour images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Indexed,
    Rgba,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Indexed => "palette",
            Self::Rgba => "true-colour",
        }
    }
    fn layer_type(self) -> i32 {
        match self {
            Self::Indexed => 0,
            Self::Rgba => 1,
        }
    }
}

/// Converts a true-colour image to palette indices: pixels with alpha below 128 become index 0
/// (transparent), all others the nearest colour (RGB distance) of the palette entries 1 to 255.
pub fn quantize(img: &RgbaImage, palette: &[u8; PALETTE_LEN]) -> IndexedImage {
    let mut cache: std::collections::HashMap<[u8; 3], u8> = std::collections::HashMap::new();
    let pixels = img
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if p[3] < 128 {
                return 0;
            }
            *cache.entry([p[0], p[1], p[2]]).or_insert_with(|| {
                let dist = |i: usize| -> i32 {
                    (0..3)
                        .map(|c| {
                            let d = i32::from(p[c]) - i32::from(palette[4 * i + c]);
                            d * d
                        })
                        .sum()
                };
                (1..256).min_by_key(|&i| dist(i)).unwrap_or(1) as u8
            })
        })
        .collect();
    IndexedImage {
        width: img.width,
        height: img.height,
        pixels,
    }
}

impl Spr {
    fn check(&self, kind: Kind, index: usize) -> Result<(), EditError> {
        let count = match kind {
            Kind::Indexed => self.indexed.len(),
            Kind::Rgba => self.rgba.len(),
        };
        if index < count {
            Ok(())
        } else {
            Err(EditError::NoImage {
                kind: kind.name(),
                index,
                count,
            })
        }
    }

    pub fn replace_indexed(&mut self, index: usize, img: IndexedImage) -> Result<(), EditError> {
        self.check(Kind::Indexed, index)?;
        self.indexed[index] = img;
        Ok(())
    }

    pub fn replace_rgba(&mut self, index: usize, img: RgbaImage) -> Result<(), EditError> {
        self.check(Kind::Rgba, index)?;
        self.rgba[index] = img;
        Ok(())
    }

    /// Replaces the palette; the images keep their indices, so the colours change.
    pub fn set_palette(&mut self, pal: &Pal) {
        self.palette = Some(pal.colors.clone());
    }

    /// Appends a palette image and returns its index. Needs a palette to exist already.
    pub fn push_indexed(&mut self, img: IndexedImage) -> Result<usize, EditError> {
        if self.palette.is_none() {
            return Err(EditError::NoPalette);
        }
        self.indexed.push(img);
        Ok(self.indexed.len() - 1)
    }

    pub fn push_rgba(&mut self, img: RgbaImage) -> usize {
        self.rgba.push(img);
        self.rgba.len() - 1
    }

    /// Removes an image. Pass the ACT that belongs to the sprite so that its layers follow:
    /// layers using the removed image become empty (index -1), later images move down by one.
    pub fn remove_image(
        &mut self,
        kind: Kind,
        index: usize,
        act: Option<&mut Act>,
    ) -> Result<(), EditError> {
        self.check(kind, index)?;
        match kind {
            Kind::Indexed => {
                self.indexed.remove(index);
            }
            Kind::Rgba => {
                self.rgba.remove(index);
            }
        }
        if let Some(act) = act {
            act.sprite_removed(kind, index);
        }
        Ok(())
    }
}

impl Spr {
    /// Turns every palette image into a true-colour image (index 0 becomes transparent) and drops
    /// the palette. Pass the ACT of the sprite so that its layers follow.
    pub fn to_rgba(&mut self, act: Option<&mut Act>) -> Result<usize, EditError> {
        let pal = match (&self.palette, self.indexed.is_empty()) {
            (_, true) => return Err(EditError::NoIndexed),
            (Some(p), false) => p.clone(),
            (None, false) => return Err(EditError::NoPalette),
        };
        if self.rgba.len() + self.indexed.len() > usize::from(u16::MAX) {
            return Err(EditError::TooManyImages);
        }
        if let Some(a) = &act
            && a.version < crate::act::V2_0
        {
            return Err(EditError::ActTooOld);
        }
        let offset = self.rgba.len() as i32;
        let n = self.indexed.len();
        for img in std::mem::take(&mut self.indexed) {
            let mut pixels = Vec::with_capacity(img.pixels.len() * 4);
            for &i in &img.pixels {
                let c = &pal[4 * usize::from(i)..4 * usize::from(i) + 3];
                pixels.extend_from_slice(&[c[0], c[1], c[2], if i == 0 { 0 } else { 255 }]);
            }
            self.rgba.push(RgbaImage {
                width: img.width,
                height: img.height,
                pixels,
            });
        }
        self.palette = None;
        if let Some(act) = act {
            for l in act
                .actions
                .iter_mut()
                .flat_map(|a| &mut a.frames)
                .flat_map(|f| &mut f.layers)
            {
                if l.sprite_index >= 0 && l.sprite_type == 0 {
                    l.sprite_index += offset;
                    l.sprite_type = 1;
                }
            }
        }
        Ok(n)
    }

    /// Turns every true-colour image into a palette image. With a palette present the nearest of
    /// its colours is used; without one a palette is made from the images (exact when they use
    /// at most 255 colours, else by median cut). Pixels with alpha below 128 become index 0.
    pub fn to_indexed(&mut self, act: Option<&mut Act>) -> Result<IndexedReport, EditError> {
        if self.rgba.is_empty() {
            return Err(EditError::NoRgba);
        }
        if self.rgba.len() + self.indexed.len() > usize::from(u16::MAX) {
            return Err(EditError::TooManyImages);
        }
        let pal: Box<[u8; PALETTE_LEN]> = match &self.palette {
            Some(p) => p.clone(),
            None => Box::new(build_palette(&self.rgba)),
        };
        let mut lossless = true;
        let offset = self.indexed.len() as i32;
        let images = self.rgba.len();
        let mut used = std::collections::BTreeSet::new();
        for img in std::mem::take(&mut self.rgba) {
            let indexed = quantize(&img, &pal);
            for (p, &i) in img.pixels.as_chunks::<4>().0.iter().zip(&indexed.pixels) {
                if p[3] != 0 && p[3] != 255 {
                    lossless = false;
                }
                if p[3] >= 128 {
                    used.insert(i);
                    let c = &pal[4 * usize::from(i)..4 * usize::from(i) + 3];
                    if c != &p[..3] {
                        lossless = false;
                    }
                }
            }
            self.indexed.push(indexed);
        }
        self.palette = Some(pal);
        if let Some(act) = act {
            for l in act
                .actions
                .iter_mut()
                .flat_map(|a| &mut a.frames)
                .flat_map(|f| &mut f.layers)
            {
                if l.sprite_index >= 0 && l.sprite_type == 1 {
                    l.sprite_index += offset;
                    l.sprite_type = 0;
                }
            }
        }
        Ok(IndexedReport {
            lossless,
            colours: used.len(),
            images,
        })
    }
}

/// A palette (entry 0 reserved for transparency) for true-colour images: the exact colours when
/// there are at most 255 opaque ones, else a median cut.
fn build_palette(images: &[RgbaImage]) -> [u8; PALETTE_LEN] {
    let mut count: std::collections::HashMap<[u8; 3], u64> = std::collections::HashMap::new();
    let mut order: Vec<[u8; 3]> = Vec::new();
    for p in images.iter().flat_map(|i| i.pixels.as_chunks::<4>().0) {
        if p[3] >= 128 {
            let c = [p[0], p[1], p[2]];
            let n = count.entry(c).or_insert(0);
            if *n == 0 {
                order.push(c);
            }
            *n += 1;
        }
    }
    let colours: Vec<[u8; 3]> = if order.len() <= 255 {
        order
    } else {
        median_cut(order.iter().map(|c| (*c, count[c])).collect(), 255)
    };
    let mut pal = [0u8; PALETTE_LEN];
    for (i, c) in colours.iter().enumerate() {
        pal[4 * (i + 1)..4 * (i + 1) + 3].copy_from_slice(c);
    }
    pal
}

/// Median cut on weighted colours: repeatedly splits the box with the most pixels along its widest
/// channel, then averages each box.
fn median_cut(colours: Vec<([u8; 3], u64)>, target: usize) -> Vec<[u8; 3]> {
    let mut boxes = vec![colours];
    while boxes.len() < target {
        let Some((at, _)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.len() > 1)
            .max_by_key(|(_, b)| b.iter().map(|(_, n)| n).sum::<u64>())
        else {
            break;
        };
        let mut b = boxes.swap_remove(at);
        let widest = (0..3)
            .max_by_key(|&c| {
                let (lo, hi) = b.iter().fold((255u8, 0u8), |(lo, hi), (p, _)| {
                    (lo.min(p[c]), hi.max(p[c]))
                });
                hi - lo
            })
            .unwrap_or(0);
        b.sort_by_key(|(p, _)| p[widest]);
        let total: u64 = b.iter().map(|(_, n)| n).sum();
        let (mut acc, mut cut) = (0u64, 1);
        for (i, (_, n)) in b.iter().enumerate() {
            acc += n;
            if acc * 2 >= total {
                cut = (i + 1).clamp(1, b.len() - 1);
                break;
            }
        }
        let tail = b.split_off(cut);
        boxes.push(b);
        boxes.push(tail);
    }
    boxes
        .iter()
        .map(|b| {
            let total: u64 = b.iter().map(|(_, n)| n).sum::<u64>().max(1);
            let avg =
                |c: usize| (b.iter().map(|(p, n)| u64::from(p[c]) * n).sum::<u64>() / total) as u8;
            [avg(0), avg(1), avg(2)]
        })
        .collect()
}

impl Act {
    /// Adjusts the layers after image `index` of `kind` was removed from the SPR.
    pub fn sprite_removed(&mut self, kind: Kind, index: usize) {
        let t = kind.layer_type();
        for a in &mut self.actions {
            for f in &mut a.frames {
                for l in &mut f.layers {
                    if l.sprite_index < 0 || l.sprite_type != t {
                        continue;
                    }
                    let i = l.sprite_index as usize;
                    if i == index {
                        l.sprite_index = -1;
                    } else if i > index {
                        l.sprite_index -= 1;
                    }
                }
            }
        }
    }

    fn check_action(&self, index: usize) -> Result<(), EditError> {
        if index < self.actions.len() {
            Ok(())
        } else {
            Err(EditError::NoAction {
                index,
                count: self.actions.len(),
            })
        }
    }

    /// Moves every layer (and anchor) of one action, or of all actions with `None`.
    pub fn translate(&mut self, action: Option<usize>, dx: f32, dy: f32) -> Result<(), EditError> {
        if let Some(a) = action {
            self.check_action(a)?;
        }
        for (i, a) in self.actions.iter_mut().enumerate() {
            if action.is_some_and(|n| n != i) {
                continue;
            }
            for f in &mut a.frames {
                for l in &mut f.layers {
                    l.offset_x += dx;
                    l.offset_y += dy;
                }
                for an in &mut f.anchors {
                    an.x += dx.round() as i32;
                    an.y += dy.round() as i32;
                }
            }
        }
        Ok(())
    }

    /// Frame time in units of 25 ms.
    pub fn set_interval(&mut self, action: usize, interval: f32) -> Result<(), EditError> {
        self.check_action(action)?;
        self.actions[action].interval = interval;
        Ok(())
    }

    /// Copies action `src` and inserts the copy at `at` (0 to the number of actions).
    pub fn copy_action(&mut self, src: usize, at: usize) -> Result<(), EditError> {
        self.check_action(src)?;
        if self.actions.len() >= usize::from(u16::MAX) {
            return Err(EditError::TooManyActions);
        }
        let copy: Action = self.actions[src].clone();
        let at = at.min(self.actions.len());
        self.actions.insert(at, copy);
        Ok(())
    }

    pub fn remove_action(&mut self, index: usize) -> Result<(), EditError> {
        self.check_action(index)?;
        self.actions.remove(index);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Anchor, Frame, Layer, V2_5};
    use crate::spr::Version;

    fn pal() -> Box<[u8; PALETTE_LEN]> {
        let mut p = Box::new([0u8; PALETTE_LEN]);
        p[4..8].copy_from_slice(&[255, 0, 0, 0]); // 1 red
        p[8..12].copy_from_slice(&[0, 255, 0, 0]); // 2 green
        p[12..16].copy_from_slice(&[0, 0, 255, 0]); // 3 blue
        p
    }

    fn img(v: u8) -> IndexedImage {
        IndexedImage {
            width: 1,
            height: 1,
            pixels: vec![v],
        }
    }

    fn spr() -> Spr {
        Spr {
            version: Version::V2_1,
            indexed: vec![img(1), img(2), img(3)],
            rgba: vec![],
            palette: Some(pal()),
            warnings: vec![],
        }
    }

    fn act() -> Act {
        let layer = |i: i32| Layer {
            sprite_index: i,
            ..Layer::default()
        };
        Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![
                Action {
                    frames: vec![Frame {
                        layers: vec![layer(0), layer(1), layer(2)],
                        anchors: vec![Anchor {
                            unknown: [0; 4],
                            x: 1,
                            y: 2,
                            other: 0,
                        }],
                        ..Frame::default()
                    }],
                    interval: 4.0,
                },
                Action::default(),
            ],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn quantize_picks_nearest_and_transparent() {
        let p = pal();
        let src = RgbaImage {
            width: 4,
            height: 1,
            pixels: vec![
                250, 10, 10, 255, // nearly red
                10, 200, 10, 255, // nearly green
                0, 0, 0, 0, // transparent
                20, 20, 240, 127, // alpha below 128
            ],
        };
        let q = quantize(&src, &p);
        assert_eq!(q.pixels, vec![1, 2, 0, 0]);
    }

    #[test]
    fn replace_push_and_palette() {
        let mut s = spr();
        s.replace_indexed(1, img(9)).unwrap();
        assert_eq!(s.indexed[1].pixels, vec![9]);
        assert_eq!(
            s.replace_indexed(5, img(1)),
            Err(EditError::NoImage {
                kind: "palette",
                index: 5,
                count: 3
            })
        );
        assert_eq!(s.push_indexed(img(2)).unwrap(), 3);
        let mut other = [0u8; PALETTE_LEN];
        other[4] = 77;
        s.set_palette(&Pal::parse(&other).unwrap());
        assert_eq!(s.palette.as_ref().unwrap()[4], 77);
        let mut no_pal = spr();
        no_pal.palette = None;
        assert_eq!(no_pal.push_indexed(img(1)), Err(EditError::NoPalette));
        assert_eq!(
            s.push_rgba(RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![0; 4]
            }),
            0
        );
    }

    #[test]
    fn removing_an_image_updates_the_layers() {
        let mut s = spr();
        let mut a = act();
        s.remove_image(Kind::Indexed, 1, Some(&mut a)).unwrap();
        assert_eq!(s.indexed.len(), 2);
        let idx: Vec<i32> = a.actions[0].frames[0]
            .layers
            .iter()
            .map(|l| l.sprite_index)
            .collect();
        assert_eq!(idx, vec![0, -1, 1]);
        // layers of the other type are left alone
        let mut a = act();
        a.actions[0].frames[0].layers[2].sprite_type = 1;
        a.sprite_removed(Kind::Indexed, 0);
        let idx: Vec<i32> = a.actions[0].frames[0]
            .layers
            .iter()
            .map(|l| l.sprite_index)
            .collect();
        assert_eq!(idx, vec![-1, 0, 2]);
        assert!(s.remove_image(Kind::Rgba, 0, None).is_err());
    }

    #[test]
    fn palette_images_become_true_colour_and_back_with_the_layers_following() {
        let mut s = spr();
        s.indexed.push(img(0)); // a transparent pixel
        let mut a = act();
        a.actions[0].frames[0].layers[1].sprite_index = 3;
        s.rgba.push(RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![9, 9, 9, 255],
        });
        let before = s.clone();
        assert_eq!(s.to_rgba(Some(&mut a)).unwrap(), 4);
        assert!(s.indexed.is_empty() && s.palette.is_none());
        // the existing true-colour image stays first, the converted ones follow
        assert_eq!(s.rgba[0].pixels, [9, 9, 9, 255]);
        assert_eq!(s.rgba[1].pixels, [255, 0, 0, 255]);
        assert_eq!(s.rgba[4].pixels[3], 0);
        let layers: Vec<(i32, i32)> = a.actions[0].frames[0]
            .layers
            .iter()
            .map(|l| (l.sprite_index, l.sprite_type))
            .collect();
        assert_eq!(layers, [(1, 1), (4, 1), (3, 1)]);
        assert_eq!(s.to_rgba(None), Err(EditError::NoIndexed));

        // and back: the colours are exact, so nothing is lost
        let r = s.to_indexed(Some(&mut a)).unwrap();
        assert!(r.lossless, "{r:?}");
        assert_eq!((r.images, r.colours), (5, 4));
        assert_eq!(s.indexed.len(), 5);
        let kinds: Vec<(i32, i32)> = a.actions[0].frames[0]
            .layers
            .iter()
            .map(|l| (l.sprite_index, l.sprite_type))
            .collect();
        assert_eq!(kinds, [(1, 0), (4, 0), (3, 0)]);
        // the image that was already true-colour (9, 9, 9) is now palette entry 4 or whatever it got
        let p = s.palette.as_ref().unwrap();
        let i = usize::from(s.indexed[0].pixels[0]);
        assert_eq!(&p[4 * i..4 * i + 3], [9, 9, 9]);
        assert_eq!(before.indexed.len(), 4);
    }

    #[test]
    fn many_colours_are_reduced_and_reported_as_lossy() {
        let mut s = Spr {
            version: Version::V2_1,
            indexed: vec![],
            rgba: vec![RgbaImage {
                width: 400,
                height: 1,
                pixels: (0..400u32)
                    .flat_map(|i| [(i % 256) as u8, (i / 2) as u8, (i * 7 % 256) as u8, 255])
                    .collect(),
            }],
            palette: None,
            warnings: vec![],
        };
        let r = s.to_indexed(None).unwrap();
        assert!(!r.lossless && r.colours <= 255, "{r:?}");
        assert!(s.indexed[0].pixels.iter().all(|&i| i != 0));
        // semi-transparent pixels are cut even when the colours fit
        let mut s = Spr {
            version: Version::V2_1,
            indexed: vec![],
            rgba: vec![RgbaImage {
                width: 1,
                height: 1,
                pixels: vec![1, 2, 3, 200],
            }],
            palette: None,
            warnings: vec![],
        };
        assert!(!s.to_indexed(None).unwrap().lossless);
    }

    #[test]
    fn translate_interval_copy_and_remove_action() {
        let mut a = act();
        a.translate(Some(0), 3.0, -2.0).unwrap();
        let f = &a.actions[0].frames[0];
        assert_eq!((f.layers[0].offset_x, f.layers[0].offset_y), (3.0, -2.0));
        assert_eq!((f.anchors[0].x, f.anchors[0].y), (4, 0));
        a.translate(None, 1.0, 0.0).unwrap();
        assert_eq!(a.actions[0].frames[0].layers[0].offset_x, 4.0);
        assert!(a.translate(Some(9), 1.0, 1.0).is_err());

        a.set_interval(1, 8.0).unwrap();
        assert_eq!(a.actions[1].interval, 8.0);
        assert!(a.set_interval(2, 1.0).is_err());

        a.copy_action(0, 1).unwrap();
        assert_eq!(a.actions.len(), 3);
        assert_eq!(a.actions[1], a.actions[0]);
        a.copy_action(0, 99).unwrap();
        assert_eq!(a.actions.len(), 4);
        a.remove_action(1).unwrap();
        assert_eq!(a.actions.len(), 3);
        assert!(a.remove_action(3).is_err());
        // the edited ACT still writes and reads back
        let bytes = a.to_bytes(V2_5).unwrap();
        assert_eq!(Act::parse(&bytes).unwrap().actions, a.actions);
    }
}
