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
