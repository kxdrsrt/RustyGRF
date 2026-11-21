//! Editing operations on SPR, ACT and PAL data (M7b).
//!
//! Everything works on the in-memory structures; the CLI writes the result to a new file.

use crate::act::{Act, Action, Anchor, Frame, Layer};
use crate::pal::Pal;
use crate::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr};
use crate::version::Version;

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
    #[error("action {action} has no frame {frame} ({count} frames)")]
    NoFrame {
        action: usize,
        frame: usize,
        count: usize,
    },
    #[error("nothing selected: no layer matches")]
    NoLayerSelected,
    #[error("action {action} has no frame after frame {frame} to blend with")]
    NoNextFrame { action: usize, frame: usize },
    #[error("between 1 and {max} in-between frames can be added at once")]
    BadCount { max: usize },
    #[error("'{0}' is not a list of numbers like 0-2;4")]
    BadIndexList(String),
    #[error("the new order names frames or actions that do not exist, or is empty")]
    BadOrder,
    #[error("{what} needs ACT version {need} or newer (this file is {have})")]
    NeedsVersion {
        what: &'static str,
        need: &'static str,
        have: Version,
    },
    #[error("the sound name must be at most 39 Latin-1 characters")]
    BadSound,
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

    /// Inserts a palette image so that it gets `index` (images from there on move up by one). Needs
    /// a palette. Pass the ACT of the sprite so that its layers follow.
    pub fn insert_indexed(
        &mut self,
        index: usize,
        img: IndexedImage,
        act: Option<&mut Act>,
    ) -> Result<(), EditError> {
        if self.palette.is_none() {
            return Err(EditError::NoPalette);
        }
        if index > self.indexed.len() {
            return Err(EditError::NoImage {
                kind: Kind::Indexed.name(),
                index,
                count: self.indexed.len(),
            });
        }
        self.indexed.insert(index, img);
        if let Some(act) = act {
            act.sprite_inserted(Kind::Indexed, index);
        }
        Ok(())
    }

    /// The same for a true-colour image.
    pub fn insert_rgba(
        &mut self,
        index: usize,
        img: RgbaImage,
        act: Option<&mut Act>,
    ) -> Result<(), EditError> {
        if index > self.rgba.len() {
            return Err(EditError::NoImage {
                kind: Kind::Rgba.name(),
                index,
                count: self.rgba.len(),
            });
        }
        self.rgba.insert(index, img);
        if let Some(act) = act {
            act.sprite_inserted(Kind::Rgba, index);
        }
        Ok(())
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

/// Which layers an edit applies to; `None` means all.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerSelect {
    pub action: Option<usize>,
    pub frame: Option<usize>,
    pub layer: Option<usize>,
}

/// New values for the layers selected by [`LayerSelect`]; `None` leaves a property alone.
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerEdit {
    /// R, G, B, A mask (255 each is unchanged colour).
    pub color: Option<[u8; 4]>,
    /// Horizontal and vertical scale.
    pub scale: Option<(f32, f32)>,
    pub rotation: Option<i32>,
    pub mirror: Option<bool>,
    /// Toggles the mirror flag (after `mirror`).
    pub flip: bool,
}

impl Act {
    /// Adjusts the layers after an image was inserted at `index` of `kind`: the images from there
    /// on have moved up by one.
    pub fn sprite_inserted(&mut self, kind: Kind, index: usize) {
        let t = kind.layer_type();
        for l in self
            .actions
            .iter_mut()
            .flat_map(|a| &mut a.frames)
            .flat_map(|f| &mut f.layers)
        {
            if l.sprite_index >= 0 && l.sprite_type == t && l.sprite_index as usize >= index {
                l.sprite_index += 1;
            }
        }
    }

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

    /// Changes the properties of every selected layer; returns how many layers matched.
    /// Colour, scale and rotation exist from ACT 2.0, a separate vertical scale from 2.4.
    pub fn edit_layers(&mut self, sel: &LayerSelect, edit: &LayerEdit) -> Result<usize, EditError> {
        if let Some(a) = sel.action {
            self.check_action(a)?;
            if let Some(f) = sel.frame {
                let count = self.actions[a].frames.len();
                if f >= count {
                    return Err(EditError::NoFrame {
                        action: a,
                        frame: f,
                        count,
                    });
                }
            }
        }
        let v = self.version;
        let need = |what, min: Version, name| {
            if v < min {
                Err(EditError::NeedsVersion {
                    what,
                    need: name,
                    have: v,
                })
            } else {
                Ok(())
            }
        };
        if edit.color.is_some() {
            need("a colour", crate::act::V2_0, "2.0")?;
        }
        if edit.rotation.is_some() {
            need("a rotation", crate::act::V2_0, "2.0")?;
        }
        if edit.scale.is_some_and(|(sx, sy)| sx != 1.0 || sy != 1.0) {
            need("a scale", crate::act::V2_0, "2.0")?;
        }
        if edit.scale.is_some_and(|(sx, sy)| sx != sy) {
            need("a different vertical scale", crate::act::V2_4, "2.4")?;
        }
        let mut n = 0;
        for (ai, a) in self.actions.iter_mut().enumerate() {
            if sel.action.is_some_and(|x| x != ai) {
                continue;
            }
            for (fi, f) in a.frames.iter_mut().enumerate() {
                if sel.frame.is_some_and(|x| x != fi) {
                    continue;
                }
                for (li, l) in f.layers.iter_mut().enumerate() {
                    if sel.layer.is_some_and(|x| x != li) {
                        continue;
                    }
                    n += 1;
                    if let Some(c) = edit.color {
                        l.color = c;
                    }
                    if let Some((sx, sy)) = edit.scale {
                        l.scale_x = sx;
                        l.scale_y = sy;
                    }
                    if let Some(r) = edit.rotation {
                        l.rotation = r;
                    }
                    if let Some(m) = edit.mirror {
                        l.mirror = m;
                    }
                    if edit.flip {
                        l.mirror = !l.mirror;
                    }
                }
            }
        }
        if n == 0 {
            return Err(EditError::NoLayerSelected);
        }
        Ok(n)
    }

    /// Plays `sound` (a file name below `data\wav`, Latin-1) with one frame, or
    /// no sound with `None`. The name is added to the sound table when it is new. Sounds exist
    /// from ACT 2.0 (the table from 2.1).
    pub fn set_frame_sound(
        &mut self,
        action: usize,
        frame: usize,
        sound: Option<&str>,
    ) -> Result<(), EditError> {
        self.check_action(action)?;
        let count = self.actions[action].frames.len();
        if frame >= count {
            return Err(EditError::NoFrame {
                action,
                frame,
                count,
            });
        }
        if self.version < crate::act::V2_1 {
            return Err(EditError::NeedsVersion {
                what: "a sound",
                need: "2.1",
                have: self.version,
            });
        }
        let id = match sound {
            None => -1,
            Some(name) => {
                let raw = crate::act::sound_from_name(name).ok_or(EditError::BadSound)?;
                let at = match self.sounds.iter().position(|s| *s == raw) {
                    Some(i) => i,
                    None => {
                        self.sounds.push(raw);
                        self.sounds.len() - 1
                    }
                };
                at as i32
            }
        };
        self.actions[action].frames[frame].sound_id = id;
        Ok(())
    }

    /// Frame time in units of 25 ms.
    pub fn set_interval(&mut self, action: usize, interval: f32) -> Result<(), EditError> {
        self.check_action(action)?;
        self.actions[action].interval = interval;
        Ok(())
    }

    /// Mirrors the layers of one frame (`frame` is `Some`) or of a whole action: `up_down` turns the
    /// picture over (y), else left to right (x, around the origin). `odd_width` tells whether the
    /// image of a layer has an odd width (the origin is then half a pixel off). The rules of ACT
    /// Editor's `FlipCommand`.
    pub fn flip(
        &mut self,
        action: usize,
        frame: Option<usize>,
        up_down: bool,
        odd_width: &dyn Fn(&Layer) -> bool,
    ) -> Result<(), EditError> {
        self.check_action(action)?;
        let count = self.actions[action].frames.len();
        if let Some(f) = frame
            && f >= count
        {
            return Err(EditError::NoFrame {
                action,
                frame: f,
                count,
            });
        }
        let frames = self.actions[action].frames.iter_mut().enumerate();
        for (i, fr) in frames {
            if frame.is_some_and(|f| f != i) {
                continue;
            }
            for l in &mut fr.layers {
                flip_layer(l, up_down, odd_width(l));
            }
        }
        Ok(())
    }

    /// Writes a mirrored copy of `action` over its opposite direction (1 and 7, 2 and 6, 3 and 5 of
    /// each group of 8; ACT Editor's "Mirror action from left/right to right/left"). Returns the
    /// number of the action that was written, `None` when `action` faces straight up or down.
    pub fn mirror_action(
        &mut self,
        action: usize,
        odd_width: &dyn Fn(&Layer) -> bool,
    ) -> Result<Option<usize>, EditError> {
        self.check_action(action)?;
        let opposite = match action % 8 {
            d @ 1..=3 => 8 - d,
            d @ 5..=7 => 8 - d,
            _ => return Ok(None),
        } + action / 8 * 8;
        self.check_action(opposite)?;
        let mut copy = self.actions[action].clone();
        for fr in &mut copy.frames {
            for l in &mut fr.layers {
                flip_layer(l, false, odd_width(l));
            }
            for a in &mut fr.anchors {
                a.x = -(a.x - 1);
            }
        }
        self.actions[opposite] = copy;
        Ok(Some(opposite))
    }

    /// Plays the frames of an action backwards.
    pub fn reverse_action(&mut self, action: usize) -> Result<(), EditError> {
        self.check_action(action)?;
        self.actions[action].frames.reverse();
        Ok(())
    }

    /// ACT Editor's "Set anchors from file" (`adjust` false) and "Adjust anchors from file"
    /// (`adjust` true). Actions 0 to 7 and 16 to 23 (the body directions) take the frame at the
    /// same relative place of the other action, the rest the frame with the same number (the last
    /// one when there are fewer). Setting copies the anchors; adjusting makes the first anchor
    /// the other file's and moves all layers of the frame by the difference, so a headgear stays
    /// where it was seen (for example from the male to the female head).
    pub fn anchors_from(&mut self, other: &Act, adjust: bool) {
        for i in 0..self.actions.len().min(other.actions.len()) {
            let body = i < 8 || (16..24).contains(&i);
            let n = self.actions[i].frames.len();
            for j in 0..n {
                let theirs = &other.actions[i].frames;
                let reference = if body {
                    (!theirs.is_empty()).then(|| &theirs[j * theirs.len() / n])
                } else {
                    theirs.get(j).or(theirs.last())
                };
                let fr = &mut self.actions[i].frames[j];
                match reference {
                    None => fr.anchors.clear(),
                    Some(r) if !adjust => fr.anchors = r.anchors.clone(),
                    Some(r) => match (fr.anchors.first().cloned(), r.anchors.first()) {
                        (Some(own), Some(want)) => {
                            let (dx, dy) = ((want.x - own.x) as f32, (want.y - own.y) as f32);
                            fr.anchors[0] = want.clone();
                            for l in &mut fr.layers {
                                l.offset_x += dx;
                                l.offset_y += dy;
                            }
                        }
                        (None, None) => {}
                        _ => fr.anchors.clear(),
                    },
                }
            }
        }
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

    /// Adds `count` in-between frames that blend each frame with the next one, as an animator's
    /// "tweening": offsets, scale, rotation (the short way round), colour and anchors move in
    /// equal steps (or with a smooth start and stop); the image, mirroring and sound do not blend,
    /// the image changes halfway, and the sound stays with the original frame.
    ///
    /// `from` is one frame (blended with the one after it), or `None` for the whole action. With
    /// `wrap` the last frame also blends back into the first (looping actions). Returns how many
    /// frames were added. Layers and anchors are paired by number; those only one side has appear
    /// in the first or second half.
    pub fn interpolate(
        &mut self,
        action: usize,
        from: Option<usize>,
        count: usize,
        ease: Ease,
        wrap: bool,
    ) -> Result<usize, EditError> {
        self.check_action(action)?;
        if !(1..=MAX_IN_BETWEEN).contains(&count) {
            return Err(EditError::BadCount {
                max: MAX_IN_BETWEEN,
            });
        }
        let frames = &self.actions[action].frames;
        let n = frames.len();
        let pairs: Vec<(usize, usize)> = match from {
            Some(f) => {
                if f >= n {
                    return Err(EditError::NoFrame {
                        action,
                        frame: f,
                        count: n,
                    });
                }
                if f + 1 < n {
                    vec![(f, f + 1)]
                } else if wrap && n > 1 {
                    vec![(f, 0)]
                } else {
                    return Err(EditError::NoNextFrame { action, frame: f });
                }
            }
            None => {
                let mut p: Vec<_> = (0..n.saturating_sub(1)).map(|i| (i, i + 1)).collect();
                if wrap && n > 1 {
                    p.push((n - 1, 0));
                }
                if p.is_empty() {
                    return Err(EditError::NoNextFrame { action, frame: 0 });
                }
                p
            }
        };
        let added = pairs.len() * count;
        if n + added > MAX_FRAMES {
            return Err(EditError::BadCount {
                max: (MAX_FRAMES - n) / pairs.len(),
            });
        }
        let mut out = Vec::with_capacity(n + added);
        for (i, f) in frames.iter().enumerate() {
            out.push(f.clone());
            if let Some(&(a, b)) = pairs.iter().find(|p| p.0 == i) {
                for k in 1..=count {
                    let t = k as f32 / (count + 1) as f32;
                    out.push(blend_frames(&frames[a], &frames[b], t, ease));
                }
            }
        }
        self.actions[action].frames = out;
        Ok(added)
    }

    pub fn remove_action(&mut self, index: usize) -> Result<(), EditError> {
        self.check_action(index)?;
        self.actions.remove(index);
        Ok(())
    }
}

/// The most in-between frames added between two frames at once, and the most frames of an action.
pub const MAX_IN_BETWEEN: usize = 20;
const MAX_FRAMES: usize = 4096;

/// How in-between frames are spaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ease {
    /// Equal steps.
    #[default]
    Linear,
    /// Slow at both ends (smoothstep).
    Smooth,
}

fn lerp(a: f32, b: f32, s: f32) -> f32 {
    a + (b - a) * s
}

/// The frame a fraction `t` (0 to 1) of the way from `a` to `b`; see [`Act::interpolate`].
pub fn blend_frames(a: &Frame, b: &Frame, t: f32, ease: Ease) -> Frame {
    let s = match ease {
        Ease::Linear => t,
        Ease::Smooth => t * t * (3.0 - 2.0 * t),
    };
    let mut layers = Vec::new();
    for i in 0..a.layers.len().max(b.layers.len()) {
        match (a.layers.get(i), b.layers.get(i)) {
            (Some(x), Some(y)) => layers.push(blend_layer(x, y, t, s)),
            (Some(x), None) if t < 0.5 => layers.push(x.clone()),
            (None, Some(y)) if t >= 0.5 => layers.push(y.clone()),
            _ => {}
        }
    }
    let mut anchors = Vec::new();
    for i in 0..a.anchors.len().max(b.anchors.len()) {
        match (a.anchors.get(i), b.anchors.get(i)) {
            (Some(x), Some(y)) => anchors.push(Anchor {
                x: lerp(x.x as f32, y.x as f32, s).round() as i32,
                y: lerp(x.y as f32, y.y as f32, s).round() as i32,
                ..x.clone()
            }),
            (Some(x), None) if t < 0.5 => anchors.push(x.clone()),
            (None, Some(y)) if t >= 0.5 => anchors.push(y.clone()),
            _ => {}
        }
    }
    Frame {
        range: a.range,
        layers,
        sound_id: -1,
        anchors,
    }
}

fn blend_layer(a: &Layer, b: &Layer, t: f32, s: f32) -> Layer {
    let near = if t < 0.5 { a } else { b };
    // nothing to move towards when one side shows no image
    if a.sprite_index < 0 || b.sprite_index < 0 {
        return near.clone();
    }
    // the short way round: 350 to 10 degrees is 20 degrees, not 340
    let turn = (b.rotation - a.rotation + 540).rem_euclid(360) - 180;
    let mut color = [0u8; 4];
    for (c, (x, y)) in color.iter_mut().zip(a.color.iter().zip(&b.color)) {
        *c = lerp(f32::from(*x), f32::from(*y), s).round() as u8;
    }
    Layer {
        offset_x: lerp(a.offset_x, b.offset_x, s).round(),
        offset_y: lerp(a.offset_y, b.offset_y, s).round(),
        scale_x: lerp(a.scale_x, b.scale_x, s),
        scale_y: lerp(a.scale_y, b.scale_y, s),
        rotation: a.rotation + (turn as f32 * s).round() as i32,
        color,
        ..near.clone()
    }
}

/// One layer mirrored (see [`Act::flip`]).
fn flip_layer(l: &mut Layer, up_down: bool, odd: bool) {
    if up_down {
        l.offset_y = -l.offset_y;
        l.rotation = (180 - l.rotation).rem_euclid(360);
    } else {
        l.offset_x = -(l.offset_x - if odd { 1.0 } else { 0.0 });
        l.rotation = (360 - l.rotation).rem_euclid(360);
    }
    l.mirror = !l.mirror;
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
    fn inserting_an_image_moves_the_later_ones_and_their_layers() {
        let mut s = spr();
        let mut a = act();
        s.insert_indexed(1, img(9), Some(&mut a)).unwrap();
        let px: Vec<u8> = s.indexed.iter().map(|i| i.pixels[0]).collect();
        assert_eq!(px, [1, 9, 2, 3]);
        let idx: Vec<i32> = a.actions[0].frames[0]
            .layers
            .iter()
            .map(|l| l.sprite_index)
            .collect();
        assert_eq!(idx, [0, 2, 3], "layers of the images from 1 on follow");
        // at the end nothing moves; past the end is refused; true colour counts apart
        s.insert_indexed(4, img(7), Some(&mut a)).unwrap();
        assert_eq!(a.actions[0].frames[0].layers[2].sprite_index, 3);
        assert!(s.insert_indexed(9, img(1), None).is_err());
        let rgba = RgbaImage {
            width: 1,
            height: 1,
            pixels: vec![0; 4],
        };
        assert!(s.insert_rgba(1, rgba.clone(), None).is_err());
        s.insert_rgba(0, rgba, Some(&mut a)).unwrap();
        assert_eq!(
            a.actions[0].frames[0].layers[0].sprite_index, 0,
            "palette layers stay"
        );
        s.palette = None;
        assert_eq!(s.insert_indexed(0, img(1), None), Err(EditError::NoPalette));
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

    #[test]
    fn layers_are_selected_and_edited_and_sounds_added() {
        let mut a = act();
        let all = LayerSelect::default();
        let red = LayerEdit {
            color: Some([255, 0, 0, 128]),
            scale: Some((2.0, 3.0)),
            rotation: Some(90),
            flip: true,
            ..LayerEdit::default()
        };
        // one layer only
        let one = LayerSelect {
            action: Some(0),
            frame: Some(0),
            layer: Some(1),
        };
        assert_eq!(a.edit_layers(&one, &red).unwrap(), 1);
        let ls = &a.actions[0].frames[0].layers;
        assert_eq!(ls[1].color, [255, 0, 0, 128]);
        assert_eq!(
            (ls[1].scale_x, ls[1].scale_y, ls[1].rotation),
            (2.0, 3.0, 90)
        );
        assert!(ls[1].mirror && !ls[0].mirror && ls[0].color == [255; 4]);
        // all layers, mirror set explicitly
        let m = LayerEdit {
            mirror: Some(false),
            ..LayerEdit::default()
        };
        assert_eq!(a.edit_layers(&all, &m).unwrap(), 3);
        assert!(!a.actions[0].frames[0].layers[1].mirror);
        // nothing there / out of range
        let none = LayerSelect {
            action: Some(1),
            ..LayerSelect::default()
        };
        assert!(matches!(
            a.edit_layers(&none, &m),
            Err(EditError::NoLayerSelected)
        ));
        let bad_frame = LayerSelect {
            action: Some(0),
            frame: Some(5),
            layer: None,
        };
        assert!(matches!(
            a.edit_layers(&bad_frame, &m),
            Err(EditError::NoFrame { .. })
        ));
        // a version without separate vertical scale refuses
        let mut old = act();
        old.version = crate::act::V2_3;
        assert!(matches!(
            old.edit_layers(&all, &red),
            Err(EditError::NeedsVersion { .. })
        ));
        let mut v1 = act();
        v1.version = Version::new(1, 5);
        assert!(v1.edit_layers(&all, &red).is_err());

        // sounds: the table grows once, `None` clears
        a.set_frame_sound(0, 0, Some("effect\\hit.wav")).unwrap();
        a.set_frame_sound(0, 0, Some("effect\\hit.wav")).unwrap();
        assert_eq!(a.sounds.len(), 1);
        assert_eq!(a.actions[0].frames[0].sound_id, 0);
        a.set_frame_sound(0, 0, None).unwrap();
        assert_eq!(a.actions[0].frames[0].sound_id, -1);
        assert!(a.set_frame_sound(0, 3, None).is_err());
        assert!(a.set_frame_sound(0, 0, Some(&"x".repeat(40))).is_err());
        let bytes = {
            a.set_frame_sound(0, 0, Some("a.wav")).unwrap();
            a.to_bytes(V2_5).unwrap()
        };
        let back = Act::parse(&bytes).unwrap();
        assert_eq!(back.actions, a.actions);
        assert_eq!(back.sounds, a.sounds);
    }

    fn moving_act() -> Act {
        let at = |x: f32, rot: i32, c: u8, sprite: i32| Layer {
            offset_x: x,
            rotation: rot,
            color: [c, c, c, 255],
            sprite_index: sprite,
            ..Layer::default()
        };
        let frame = |x, rot, c, sprite, sound| Frame {
            layers: vec![at(x, rot, c, sprite)],
            anchors: vec![Anchor {
                unknown: [0; 4],
                x: x as i32,
                y: 0,
                other: 0,
            }],
            sound_id: sound,
            ..Frame::default()
        };
        Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![Action {
                frames: vec![frame(0.0, 350, 0, 0, 3), frame(10.0, 10, 100, 1, 4)],
                interval: 6.0,
            }],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        }
    }

    #[test]
    fn interpolates_one_pair_in_equal_steps() {
        let mut a = moving_act();
        assert_eq!(a.interpolate(0, Some(0), 1, Ease::Linear, false), Ok(1));
        let f = &a.actions[0].frames;
        assert_eq!(f.len(), 3);
        let mid = &f[1];
        assert_eq!(mid.layers[0].offset_x, 5.0);
        assert_eq!(mid.layers[0].color, [50, 50, 50, 255]);
        // 350 to 10 degrees turns by 20 the short way
        assert_eq!(mid.layers[0].rotation, 360);
        assert_eq!(mid.anchors[0].x, 5);
        // the image changes halfway, the sound stays with the original frames
        assert_eq!(mid.layers[0].sprite_index, 1);
        assert_eq!((f[0].sound_id, mid.sound_id, f[2].sound_id), (3, -1, 4));
        let mut b = moving_act();
        b.interpolate(0, Some(0), 3, Ease::Linear, false).unwrap();
        let xs: Vec<f32> = b.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].offset_x)
            .collect();
        assert_eq!(xs, [0.0, 3.0, 5.0, 8.0, 10.0]);
        assert_eq!(b.actions[0].frames[1].layers[0].sprite_index, 0);
    }

    #[test]
    fn interpolates_a_whole_action_and_loops() {
        let mut a = moving_act();
        assert_eq!(a.interpolate(0, None, 1, Ease::Linear, true), Ok(2));
        let xs: Vec<f32> = a.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].offset_x)
            .collect();
        // the last frame blends back into the first
        assert_eq!(xs, [0.0, 5.0, 10.0, 5.0]);
        let mut s = moving_act();
        s.interpolate(0, Some(0), 3, Ease::Smooth, false).unwrap();
        let xs: Vec<f32> = s.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].offset_x)
            .collect();
        assert!(xs[1] < 3.0 && xs[2] == 5.0 && xs[3] > 7.0);
    }

    #[test]
    fn interpolation_rejects_bad_requests() {
        let mut a = moving_act();
        assert!(matches!(
            a.interpolate(0, Some(1), 1, Ease::Linear, false),
            Err(EditError::NoNextFrame { .. })
        ));
        assert!(matches!(
            a.interpolate(0, Some(0), 0, Ease::Linear, false),
            Err(EditError::BadCount { .. })
        ));
        assert!(a.interpolate(5, None, 1, Ease::Linear, false).is_err());
        assert_eq!(a.actions[0].frames.len(), 2);
    }

    #[test]
    fn layers_only_one_side_has_appear_in_their_half() {
        let mut a = moving_act();
        a.actions[0].frames[1].layers.push(Layer {
            sprite_index: 2,
            ..Layer::default()
        });
        a.interpolate(0, Some(0), 3, Ease::Linear, false).unwrap();
        let counts: Vec<usize> = a.actions[0].frames.iter().map(|f| f.layers.len()).collect();
        assert_eq!(counts, [1, 1, 2, 2, 2]);
    }

    fn eight_actions() -> Act {
        let mut a = act();
        a.actions = (0..9)
            .map(|i| Action {
                frames: vec![Frame {
                    layers: vec![Layer {
                        sprite_index: i,
                        offset_x: 5.0,
                        offset_y: 3.0,
                        rotation: 30,
                        ..Layer::default()
                    }],
                    anchors: vec![Anchor {
                        unknown: [0; 4],
                        x: 4,
                        y: 7,
                        other: 0,
                    }],
                    ..Frame::default()
                }],
                interval: 4.0,
            })
            .collect();
        a
    }

    #[test]
    fn mirroring_flips_layers_and_twice_gives_the_original() {
        let mut a = eight_actions();
        let before = a.clone();
        a.flip(1, Some(0), false, &|_| false).unwrap();
        let l = &a.actions[1].frames[0].layers[0];
        assert_eq!(
            (l.offset_x, l.offset_y, l.rotation, l.mirror),
            (-5.0, 3.0, 330, true)
        );
        a.flip(1, None, false, &|_| false).unwrap();
        assert_eq!(a, before);
        // an odd-width image sits half a pixel off
        a.flip(2, None, false, &|_| true).unwrap();
        assert_eq!(a.actions[2].frames[0].layers[0].offset_x, -4.0);
        // upside down: y flips and the angle runs from 180 degrees
        a.flip(3, None, true, &|_| false).unwrap();
        let l = &a.actions[3].frames[0].layers[0];
        assert_eq!((l.offset_x, l.offset_y, l.rotation), (5.0, -3.0, 150));
        assert!(a.flip(1, Some(4), false, &|_| false).is_err());
    }

    #[test]
    fn an_action_is_mirrored_onto_its_opposite_and_frames_reverse() {
        let mut a = eight_actions();
        assert_eq!(a.mirror_action(1, &|_| false), Ok(Some(7)));
        assert_eq!(a.mirror_action(6, &|_| false), Ok(Some(2)));
        let w = &a.actions[7].frames[0];
        assert_eq!(
            w.layers[0].sprite_index, 1,
            "the copy of action 1 lies on 7"
        );
        assert_eq!((w.layers[0].offset_x, w.anchors[0].x), (-5.0, -3));
        // straight up and down have no opposite; 8 + 1 has none because action 15 does not exist
        assert_eq!(a.mirror_action(0, &|_| false), Ok(None));
        assert_eq!(a.mirror_action(4, &|_| false), Ok(None));
        assert!(a.mirror_action(8 + 1, &|_| false).is_err());
        let mut b = act();
        b.actions[0].frames.push(Frame {
            layers: vec![],
            ..Frame::default()
        });
        b.reverse_action(0).unwrap();
        assert!(b.actions[0].frames[0].layers.is_empty());
    }

    #[test]
    fn anchors_are_set_or_adjusted_from_another_animation() {
        let mut mine = eight_actions();
        let mut theirs = eight_actions();
        for a in &mut theirs.actions {
            a.frames[0].anchors[0].x = 10;
            a.frames[0].anchors[0].y = 1;
        }
        let mut set = mine.clone();
        set.anchors_from(&theirs, false);
        assert_eq!(set.actions[3].frames[0].anchors[0].x, 10);
        mine.anchors_from(&theirs, true);
        let f = &mine.actions[3].frames[0];
        assert_eq!((f.anchors[0].x, f.anchors[0].y), (10, 1));
        // the layers moved by the difference (6, -6), so the headgear is where it was
        assert_eq!((f.layers[0].offset_x, f.layers[0].offset_y), (11.0, -3.0));
        // without anchors on their side, mine are dropped
        theirs.actions[0].frames[0].anchors.clear();
        mine.anchors_from(&theirs, true);
        assert!(mine.actions[0].frames[0].anchors.is_empty());
    }
}
