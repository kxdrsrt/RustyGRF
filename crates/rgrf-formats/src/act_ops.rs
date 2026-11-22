//! More of ACT Editor's scripts (`Core/Scripting/Scripts/*`, with permission, see
//! `docs/05-licensing.md`): repeat the frames of an action, add a sprite to all frames, copy the
//! frames of one direction onto its neighbour, reorder frames and actions, three effects that
//! animate the colour or scale of layers, and templates for new animations.

use std::collections::BTreeSet;

use crate::act::{Act, Action, Anchor, Frame, Layer, V2_4, V2_5};
use crate::edit::EditError;
use crate::spr::{Spr, Version as SprVersion};

/// Which frames the "rotation copy" scripts of ACT Editor copy where (the 8 directions of a group
/// of actions are 0 bottom, 1 bottom left, 2 left, 3 top left, 4 top, 5 top right, 6 right, 7 bottom right).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RotationCopy {
    /// 0 to 7, 2 to 1, 4 to 3, 6 to 5.
    BottomToBottomRight,
    /// 1 to 2, 3 to 4, 5 to 6, 7 to 0.
    BottomRightToBottom,
    /// 1 to 0, 3 to 2, 5 to 4, 7 to 6.
    BottomLeftToBottom,
    /// 0 to 1, 2 to 3, 4 to 5, 6 to 7.
    BottomToBottomLeft,
}

/// A list of numbers like `0-2;4;5` (`;` or `,` between them).
pub fn parse_indexes(text: &str) -> Result<BTreeSet<usize>, EditError> {
    let bad = || EditError::BadIndexList(text.to_string());
    let mut out = BTreeSet::new();
    for part in text
        .split([';', ','])
        .map(str::trim)
        .filter(|p| !p.is_empty())
    {
        match part.split_once('-') {
            Some((a, b)) => {
                let (a, b): (usize, usize) = (
                    a.trim().parse().map_err(|_| bad())?,
                    b.trim().parse().map_err(|_| bad())?,
                );
                if a > b || b > 99_999 {
                    return Err(bad());
                }
                out.extend(a..=b);
            }
            None => {
                out.insert(part.parse().map_err(|_| bad())?);
            }
        }
    }
    if out.is_empty() {
        return Err(bad());
    }
    Ok(out)
}

/// ACT Editor's ease: 0 is linear, positive starts fast, negative starts slow.
pub fn ease(amount: i32, v: f32) -> f32 {
    if amount == 0 || !(0.0..=1.0).contains(&v) {
        return v;
    }
    let p = (amount as f32 / 25.0).abs();
    if amount < 0 {
        v.powf(1.0 + p)
    } else {
        1.0 - (1.0 - v).powf(1.0 + p)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectKind {
    /// The colour of the layers turns into `target` (R, G, B, A).
    Fade { target: [u8; 4] },
    /// The layers grow and shrink by the factors and back.
    Breathing { scale_x: f32, scale_y: f32 },
    /// The colour of the layers turns into `color` and back (multiplied).
    Overlay { color: [u8; 4] },
}

#[derive(Debug, Clone)]
pub struct EffectScope {
    /// Animations (action number / 8) to apply to.
    pub groups: BTreeSet<usize>,
    /// Layers of each frame; empty means the first one.
    pub layers: BTreeSet<usize>,
    pub start: usize,
    /// Number of frames; 0 means all frames from `start` on.
    pub length: usize,
    /// When frames are missing: repeat the frames from the start (else the last frame).
    pub loop_missing: bool,
    /// Add an empty frame after the effect.
    pub add_empty: bool,
}

/// Pads the action with frames up to the end of the scope (looping from the start or repeating the
/// last one) and returns the number of frames of the scope and the frame number it ends at.
pub(crate) fn frame_span(action: &mut Action, scope: &EffectScope) -> (usize, usize) {
    let mut len = if scope.length == 0 {
        action.frames.len().saturating_sub(scope.start)
    } else {
        scope.length
    };
    if len == 0 {
        len = action.frames.len();
    }
    let need = scope.start + len;
    let had = action.frames.len();
    let mut b = 0;
    while action.frames.len() < need {
        let src = if scope.loop_missing { b } else { had - 1 };
        action.frames.push(action.frames[src].clone());
        b = (b + 1) % had;
    }
    (len, need)
}

fn lerp_color(from: [u8; 4], to: [u8; 4], t: f32) -> [u8; 4] {
    std::array::from_fn(|i| {
        let a = f32::from(from[i]);
        ((f32::from(to[i]) - a) * t + a).round().clamp(0.0, 255.0) as u8
    })
}

impl Act {
    /// Appends `times` copies of the frame sequence of an action (ACT Editor's "Duplicate frames").
    pub fn duplicate_frames(&mut self, action: usize, times: usize) -> Result<(), EditError> {
        self.check_action_pub(action)?;
        if times == 0 || times > 1000 {
            return Err(EditError::BadCount { max: 1000 });
        }
        let a = &mut self.actions[action];
        let once = a.frames.clone();
        for _ in 0..times {
            a.frames.extend(once.iter().cloned());
        }
        Ok(())
    }

    /// Adds `layer` to every frame of the actions of the given animations, in front of the others
    /// or behind them (ACT Editor's "Add sprite to all frames"). Returns the number of frames.
    pub fn add_layer_to_frames(
        &mut self,
        groups: &BTreeSet<usize>,
        layer: &Layer,
        in_front: bool,
    ) -> usize {
        let mut n = 0;
        for (i, a) in self.actions.iter_mut().enumerate() {
            if !groups.contains(&(i / 8)) {
                continue;
            }
            for f in &mut a.frames {
                if in_front {
                    f.layers.push(layer.clone());
                } else {
                    f.layers.insert(0, layer.clone());
                }
                n += 1;
            }
        }
        n
    }

    /// Replaces the frames of neighbouring directions in the group of 8 actions that holds
    /// `action` with copies of each other's (the actions' speed stays).
    pub fn rotation_copy(&mut self, action: usize, kind: RotationCopy) -> Result<(), EditError> {
        self.check_action_pub(action)?;
        let base = action / 8 * 8;
        if base + 7 >= self.actions.len() {
            return Err(EditError::NoAction {
                index: base + 7,
                count: self.actions.len(),
            });
        }
        use RotationCopy::*;
        let pairs: Vec<(usize, usize)> = match kind {
            BottomToBottomRight => [0, 2, 4, 6]
                .map(|d| (base + d, base + if d == 0 { 7 } else { d - 1 }))
                .to_vec(),
            BottomRightToBottom => [1, 3, 5, 7]
                .map(|d| (base + d, base + if d == 7 { 0 } else { d + 1 }))
                .to_vec(),
            BottomLeftToBottom => [1, 3, 5, 7].map(|d| (base + d, base + d - 1)).to_vec(),
            BottomToBottomLeft => [0, 2, 4, 6].map(|d| (base + d, base + d + 1)).to_vec(),
        };
        for (from, to) in pairs {
            self.actions[to].frames = self.actions[from].frames.clone();
        }
        Ok(())
    }

    /// Sets the frames of an action to the given list of old frame numbers (a number can occur
    /// several times, `-1` is a new empty frame): ACT Editor's "Edit frames" list.
    pub fn set_frame_order(&mut self, action: usize, order: &[i32]) -> Result<(), EditError> {
        self.check_action_pub(action)?;
        let old = &self.actions[action].frames;
        let frames = pick(old, order, Frame::default)?;
        if frames.is_empty() {
            return Err(EditError::BadOrder);
        }
        self.actions[action].frames = frames;
        Ok(())
    }

    /// The same for the whole list of actions (`-1` is a new action with one empty frame).
    pub fn set_action_order(&mut self, order: &[i32]) -> Result<(), EditError> {
        let actions = pick(&self.actions, order, || Action {
            frames: vec![Frame::default()],
            ..Action::default()
        })?;
        if actions.is_empty() || actions.len() > usize::from(u16::MAX) {
            return Err(EditError::BadOrder);
        }
        self.actions = actions;
        Ok(())
    }

    /// An effect over the frames of whole animations: ACT Editor's "Fade/Target color",
    /// "Breathing effect" and "Fade in/out color" (the multiplying one). Frames are added when
    /// the scope asks for more than an action has. Returns the number of actions changed.
    pub fn apply_effect(
        &mut self,
        kind: EffectKind,
        ease_amount: i32,
        scope: &EffectScope,
    ) -> Result<usize, EditError> {
        let mut changed = 0;
        for (aid, action) in self.actions.iter_mut().enumerate() {
            if !scope.groups.contains(&(aid / 8)) || action.frames.is_empty() {
                continue;
            }
            let (len, need) = frame_span(action, scope);
            for (step, frame) in action.frames[scope.start..need].iter_mut().enumerate() {
                let t = step as f32 / len as f32;
                let triangle = if t > 0.5 { (1.0 - t) * 2.0 } else { t * 2.0 };
                for (li, l) in frame.layers.iter_mut().enumerate() {
                    let wanted = if scope.layers.is_empty() {
                        li == 0
                    } else {
                        scope.layers.contains(&li)
                    };
                    if !wanted {
                        continue;
                    }
                    match kind {
                        EffectKind::Fade { target } => {
                            l.color = lerp_color(l.color, target, ease(ease_amount, t));
                        }
                        EffectKind::Overlay { color } => {
                            l.color = lerp_color(l.color, color, ease(ease_amount, triangle));
                        }
                        EffectKind::Breathing { scale_x, scale_y } => {
                            let t = ease(ease_amount, triangle);
                            l.scale_x += l.scale_x * t * (scale_x - 1.0);
                            l.scale_y += l.scale_y * t * (scale_y - 1.0);
                            l.offset_x = (l.offset_x + l.offset_x * t * (scale_x - 1.0)).trunc();
                            l.offset_y = (l.offset_y + l.offset_y * t * (scale_y - 1.0)).trunc();
                        }
                    }
                }
            }
            if scope.add_empty && action.frames.len() == need {
                action.frames.push(Frame::default());
            }
            changed += 1;
        }
        Ok(changed)
    }

    fn check_action_pub(&self, action: usize) -> Result<(), EditError> {
        if action < self.actions.len() {
            Ok(())
        } else {
            Err(EditError::NoAction {
                index: action,
                count: self.actions.len(),
            })
        }
    }
}

fn pick<T: Clone>(old: &[T], order: &[i32], new: impl Fn() -> T) -> Result<Vec<T>, EditError> {
    order
        .iter()
        .map(|&i| match i {
            -1 => Ok(new()),
            i if i >= 0 && (i as usize) < old.len() => Ok(old[i as usize].clone()),
            _ => Err(EditError::BadOrder),
        })
        .collect()
}

/// The templates of ACT Editor's File > New.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// One action with one empty frame.
    Empty,
    Npc,
    Monster,
    Homunculus,
    Weapon,
    /// The actions of a head sprite with the anchors of the male head, no layers.
    HeadgearMale,
    HeadgearFemale,
}

impl Template {
    /// The names the tools use, in the order of ACT Editor's File > New Act menu.
    pub const NAMES: [(&'static str, Template); 7] = [
        ("empty", Template::Empty),
        ("headgear_male", Template::HeadgearMale),
        ("headgear_female", Template::HeadgearFemale),
        ("monster", Template::Monster),
        ("homunculus", Template::Homunculus),
        ("weapon", Template::Weapon),
        ("npc", Template::Npc),
    ];

    pub fn from_name(name: &str) -> Option<Template> {
        Self::NAMES
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name.trim()))
            .map(|&(_, t)| t)
    }
}

/// The empty sprite that ACT Editor saves next to a new animation (`new Spr()`: no images).
pub fn empty_spr() -> Spr {
    Spr {
        version: SprVersion::V2_1,
        indexed: Vec::new(),
        rgba: Vec::new(),
        palette: None,
        warnings: Vec::new(),
    }
}

/// (frames, interval in units of 25 ms, number of actions) in a row.
type Runs = &'static [(usize, f32, usize)];

const MONSTER: Runs = &[
    (8, 3.0, 8),
    (8, 2.5, 8),
    (21, 2.0, 8),
    (5, 2.5, 8),
    (21, 3.0, 8),
];
const HOMUNCULUS: Runs = &[
    (8, 4.0, 8),
    (5, 4.0, 8),
    (12, 4.0, 8),
    (6, 3.0, 8),
    (18, 3.0, 8),
    (16, 3.0, 8),
    (8, 4.0, 16),
];
const WEAPON: Runs = &[
    (3, 4.0, 8),
    (8, 4.0, 8),
    (3, 4.0, 16),
    (6, 4.0, 8),
    (5, 4.0, 8),
    (3, 4.0, 8),
    (1, 4.0, 24),
    (6, 4.0, 8),
    (8, 4.0, 8),
    (1, 4.0, 8),
];
const HEAD: Runs = &[
    (3, 4.0, 8),
    (8, 3.0, 8),
    (3, 4.0, 16),
    (6, 4.0, 8),
    (5, 4.0, 8),
    (3, 4.0, 8),
    (1, 4.0, 24),
    (9, 4.0, 8),
    (8, 4.0, 8),
    (6, 4.0, 8),
];

/// Which of the `WEAPON` runs carry an empty layer in every frame (280 of the 392 frames of
/// ACT Editor's `weapon.act`). Its placeholders are all zero there (no colour, scale 0); here
/// they are ordinary empty layers, so that setting an image on one shows it.
const WEAPON_LAYER: [bool; 10] = [true, true, true, false, true, true, true, true, false, true];

fn from_runs(runs: Runs) -> Vec<Action> {
    runs.iter()
        .flat_map(|&(frames, interval, count)| {
            (0..count).map(move |_| Action {
                frames: vec![Frame::default(); frames],
                interval,
            })
        })
        .collect()
}

impl Act {
    /// A new animation shaped like one of ACT Editor's templates (the number of actions, frames
    /// and their speed; the sprite is yours to fill).
    pub fn template(kind: Template) -> Act {
        let mut act = Act {
            version: V2_5,
            reserved: [0; 10],
            actions: Vec::new(),
            sounds: Vec::new(),
            trailing: Vec::new(),
            warnings: Vec::new(),
        };
        act.actions = match kind {
            Template::Empty => vec![Action {
                frames: vec![Frame::default()],
                ..Action::default()
            }],
            Template::Npc => from_runs(&[(1, 4.0, 8)]),
            Template::Monster => from_runs(MONSTER),
            Template::Homunculus => from_runs(HOMUNCULUS),
            Template::Weapon => {
                let mut actions = from_runs(WEAPON);
                let mut next = actions.iter_mut();
                for (&(_, _, count), &with_layer) in WEAPON.iter().zip(&WEAPON_LAYER) {
                    for action in next.by_ref().take(count) {
                        if with_layer {
                            for f in &mut action.frames {
                                f.layers.push(Layer::default());
                            }
                        }
                    }
                }
                actions
            }
            Template::HeadgearMale | Template::HeadgearFemale => from_runs(HEAD),
        };
        match kind {
            Template::Monster => {
                // the attack sound on frame 11 of the attack actions (16 to 23)
                let mut name = [0u8; 40];
                name[..3].copy_from_slice(b"atk");
                act.sounds.push(name);
                for a in &mut act.actions[16..24] {
                    a.frames[11].sound_id = 0;
                }
            }
            Template::HeadgearMale | Template::HeadgearFemale => {
                let female = kind == Template::HeadgearFemale;
                if female {
                    act.version = V2_4;
                }
                let table: &[(i16, i16)] = if female {
                    &HEAD_ANCHORS_FEMALE
                } else {
                    &HEAD_ANCHORS_MALE
                };
                let frames = act.actions.iter_mut().flat_map(|a| a.frames.iter_mut());
                for (f, &(x, y)) in frames.zip(table) {
                    f.anchors.push(Anchor {
                        unknown: [0; 4],
                        x: i32::from(x),
                        y: i32::from(y),
                        other: 0,
                    });
                }
            }
            _ => {}
        }
        act
    }
}

const HEAD_ANCHORS_MALE: [(i16, i16); 456] = [
    (1, -56),
    (-4, -57),
    (5, -57),
    (-4, -57),
    (-6, -57),
    (1, -56),
    (-6, -57),
    (10, -74),
    (-4, -57),
    (10, -74),
    (11, -73),
    (-6, -57),
    (11, -73),
    (-9, -74),
    (10, -74),
    (-9, -74),
    (7, -57),
    (11, -73),
    (7, -57),
    (5, -57),
    (-9, -74),
    (5, -57),
    (0, -56),
    (7, -57),
    (1, -56),
    (1, -56),
    (1, -56),
    (1, -56),
    (1, -56),
    (1, -56),
    (1, -56),
    (1, -56),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (-6, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (11, -73),
    (11, -73),
    (11, -73),
    (11, -73),
    (11, -73),
    (11, -73),
    (11, -73),
    (11, -73),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (7, -57),
    (7, -57),
    (7, -57),
    (7, -57),
    (7, -57),
    (7, -57),
    (7, -57),
    (7, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (1, -36),
    (-4, -37),
    (5, -37),
    (-4, -37),
    (-6, -37),
    (1, -36),
    (-6, -37),
    (10, -54),
    (-4, -37),
    (10, -54),
    (11, -53),
    (-6, -37),
    (11, -53),
    (-9, -54),
    (10, -54),
    (-9, -54),
    (7, -37),
    (11, -53),
    (7, -37),
    (5, -37),
    (-9, -54),
    (5, -37),
    (0, -36),
    (7, -37),
    (-11, -53),
    (-14, -41),
    (-11, -51),
    (-11, -53),
    (-14, -41),
    (-11, -51),
    (-12, -50),
    (-13, -38),
    (-12, -49),
    (-12, -50),
    (-13, -38),
    (-12, -49),
    (13, -50),
    (14, -38),
    (13, -49),
    (13, -50),
    (14, -38),
    (13, -49),
    (12, -53),
    (15, -41),
    (12, -51),
    (12, -53),
    (15, -41),
    (12, -51),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (13, -73),
    (18, -74),
    (13, -73),
    (13, -73),
    (18, -74),
    (13, -73),
    (2, -81),
    (4, -79),
    (2, -81),
    (2, -81),
    (4, -79),
    (2, -81),
    (-1, -81),
    (-3, -79),
    (-1, -81),
    (-1, -81),
    (-3, -79),
    (-1, -81),
    (-12, -73),
    (-17, -74),
    (-12, -73),
    (-12, -73),
    (-17, -74),
    (-12, -73),
    (31, -66),
    (31, -66),
    (16, -71),
    (16, -71),
    (-15, -71),
    (-15, -71),
    (-30, -66),
    (-30, -66),
    (55, -45),
    (55, -45),
    (55, 9),
    (55, 9),
    (-54, 9),
    (-54, 9),
    (-54, -45),
    (-54, -45),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (-4, -57),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (10, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (-9, -74),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
    (5, -57),
];

const HEAD_ANCHORS_FEMALE: [(i16, i16); 456] = [
    (1, -55),
    (-3, -56),
    (5, -56),
    (-3, -56),
    (-3, -56),
    (1, -56),
    (-3, -56),
    (13, -72),
    (-3, -56),
    (13, -72),
    (13, -71),
    (-3, -56),
    (12, -71),
    (-9, -72),
    (12, -72),
    (-12, -72),
    (4, -56),
    (10, -71),
    (4, -56),
    (4, -56),
    (-12, -72),
    (4, -56),
    (0, -56),
    (4, -56),
    (0, -57),
    (0, -56),
    (0, -54),
    (0, -55),
    (0, -57),
    (0, -56),
    (0, -54),
    (0, -55),
    (-4, -57),
    (-4, -56),
    (-4, -54),
    (-4, -55),
    (-4, -57),
    (-4, -56),
    (-4, -54),
    (-4, -55),
    (-4, -57),
    (-4, -56),
    (-4, -54),
    (-4, -55),
    (-4, -57),
    (-4, -56),
    (-4, -54),
    (-4, -55),
    (14, -72),
    (14, -71),
    (14, -69),
    (14, -70),
    (14, -72),
    (14, -71),
    (14, -69),
    (14, -70),
    (12, -71),
    (12, -70),
    (12, -68),
    (12, -69),
    (12, -71),
    (12, -70),
    (12, -68),
    (12, -69),
    (-13, -72),
    (-13, -71),
    (-13, -69),
    (-13, -70),
    (-13, -72),
    (-13, -71),
    (-13, -69),
    (-13, -70),
    (5, -57),
    (5, -56),
    (5, -54),
    (5, -55),
    (5, -57),
    (5, -56),
    (5, -54),
    (5, -55),
    (5, -57),
    (5, -56),
    (5, -54),
    (5, -55),
    (5, -57),
    (5, -56),
    (5, -54),
    (5, -55),
    (0, -36),
    (-4, -36),
    (3, -36),
    (-4, -35),
    (-4, -35),
    (-1, -34),
    (-4, -35),
    (12, -51),
    (-4, -35),
    (12, -52),
    (11, -51),
    (-4, -36),
    (12, -51),
    (-9, -52),
    (12, -52),
    (-11, -52),
    (5, -36),
    (12, -51),
    (5, -35),
    (5, -35),
    (-11, -51),
    (5, -35),
    (2, -34),
    (5, -35),
    (-7, -56),
    (-11, -42),
    (-7, -54),
    (-7, -56),
    (-11, -42),
    (-7, -54),
    (-7, -56),
    (-12, -38),
    (-8, -54),
    (-7, -56),
    (-12, -38),
    (-8, -54),
    (8, -56),
    (13, -38),
    (9, -54),
    (8, -56),
    (13, -38),
    (9, -54),
    (8, -56),
    (12, -42),
    (8, -54),
    (8, -56),
    (12, -42),
    (8, -54),
    (-6, -57),
    (-6, -59),
    (-6, -60),
    (-6, -60),
    (-6, -59),
    (-6, -58),
    (-6, -57),
    (-6, -59),
    (-6, -60),
    (-6, -60),
    (-6, -59),
    (-6, -58),
    (14, -72),
    (14, -74),
    (14, -75),
    (14, -75),
    (14, -74),
    (14, -73),
    (14, -72),
    (14, -74),
    (14, -75),
    (14, -75),
    (14, -74),
    (14, -73),
    (-13, -72),
    (-13, -74),
    (-13, -75),
    (-13, -75),
    (-13, -74),
    (-13, -73),
    (-13, -72),
    (-13, -74),
    (-13, -75),
    (-13, -75),
    (-13, -74),
    (-13, -73),
    (7, -57),
    (7, -59),
    (7, -60),
    (7, -60),
    (7, -59),
    (7, -58),
    (7, -57),
    (7, -59),
    (7, -60),
    (7, -60),
    (7, -59),
    (7, -58),
    (-4, -57),
    (-2, -58),
    (-14, -47),
    (-16, -44),
    (-16, -44),
    (-4, -57),
    (-2, -58),
    (-14, -47),
    (-16, -44),
    (-16, -44),
    (15, -72),
    (19, -72),
    (2, -65),
    (0, -64),
    (0, -64),
    (15, -72),
    (19, -72),
    (2, -65),
    (0, -64),
    (0, -64),
    (-14, -72),
    (-18, -72),
    (-1, -65),
    (1, -64),
    (1, -64),
    (-14, -72),
    (-18, -72),
    (-1, -65),
    (1, -64),
    (1, -64),
    (5, -57),
    (3, -58),
    (15, -47),
    (17, -44),
    (17, -44),
    (5, -57),
    (3, -58),
    (15, -47),
    (17, -44),
    (17, -44),
    (13, -55),
    (17, -57),
    (13, -55),
    (13, -55),
    (17, -57),
    (13, -55),
    (7, -64),
    (12, -60),
    (7, -64),
    (7, -64),
    (12, -60),
    (7, -64),
    (-6, -64),
    (-11, -60),
    (-6, -64),
    (-6, -64),
    (-11, -60),
    (-6, -64),
    (-12, -55),
    (-16, -57),
    (-12, -55),
    (-12, -55),
    (-16, -57),
    (-12, -55),
    (29, -51),
    (29, -51),
    (24, -53),
    (24, -53),
    (-23, -53),
    (-23, -53),
    (-28, -51),
    (-28, -51),
    (57, -26),
    (57, -26),
    (32, -17),
    (32, -17),
    (-31, -17),
    (-31, -17),
    (-56, -26),
    (-56, -26),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-5, -57),
    (-4, -57),
    (-3, -57),
    (-3, -57),
    (-14, -51),
    (-15, -49),
    (-16, -47),
    (-16, -46),
    (-16, -46),
    (-5, -57),
    (-4, -57),
    (-3, -57),
    (-3, -57),
    (-14, -51),
    (-15, -49),
    (-16, -47),
    (-16, -46),
    (-16, -46),
    (14, -71),
    (16, -71),
    (17, -71),
    (17, -71),
    (9, -67),
    (0, -65),
    (-1, -62),
    (-1, -61),
    (-1, -61),
    (14, -71),
    (16, -71),
    (17, -71),
    (17, -71),
    (9, -67),
    (0, -65),
    (-1, -62),
    (-1, -61),
    (-1, -61),
    (-13, -71),
    (-15, -71),
    (-16, -71),
    (-16, -71),
    (-8, -67),
    (1, -65),
    (2, -62),
    (2, -61),
    (2, -61),
    (-13, -71),
    (-15, -71),
    (-16, -71),
    (-16, -71),
    (-8, -67),
    (1, -65),
    (2, -62),
    (2, -61),
    (2, -61),
    (6, -57),
    (5, -57),
    (4, -57),
    (4, -57),
    (15, -51),
    (16, -49),
    (17, -47),
    (17, -46),
    (17, -46),
    (6, -57),
    (5, -57),
    (4, -57),
    (4, -57),
    (15, -51),
    (16, -49),
    (17, -47),
    (17, -46),
    (17, -46),
    (-5, -58),
    (-2, -59),
    (1, -60),
    (-2, -59),
    (-4, -58),
    (-20, -50),
    (-16, -51),
    (-15, -52),
    (-5, -58),
    (-2, -59),
    (1, -60),
    (-2, -59),
    (-4, -58),
    (-20, -50),
    (-16, -51),
    (-15, -52),
    (13, -72),
    (18, -71),
    (26, -68),
    (21, -69),
    (16, -70),
    (4, -69),
    (8, -69),
    (9, -69),
    (13, -72),
    (18, -71),
    (26, -68),
    (21, -69),
    (16, -70),
    (4, -69),
    (8, -69),
    (9, -69),
    (-12, -72),
    (-17, -71),
    (-25, -68),
    (-20, -69),
    (-15, -70),
    (-3, -69),
    (-7, -69),
    (-8, -69),
    (-12, -72),
    (-17, -71),
    (-25, -68),
    (-20, -69),
    (-15, -70),
    (-3, -69),
    (-7, -69),
    (-8, -69),
    (6, -58),
    (3, -59),
    (0, -60),
    (3, -59),
    (5, -58),
    (21, -50),
    (17, -51),
    (16, -52),
    (6, -58),
    (3, -59),
    (0, -60),
    (3, -59),
    (5, -58),
    (21, -50),
    (17, -51),
    (16, -52),
    (-6, -57),
    (-6, -59),
    (-6, -61),
    (-5, -61),
    (-8, -57),
    (-7, -56),
    (-6, -57),
    (-6, -59),
    (-6, -61),
    (-5, -61),
    (-8, -57),
    (-7, -56),
    (15, -72),
    (15, -74),
    (15, -75),
    (15, -75),
    (14, -73),
    (14, -72),
    (15, -72),
    (15, -74),
    (15, -75),
    (15, -75),
    (14, -73),
    (14, -72),
    (-14, -72),
    (-14, -74),
    (-14, -75),
    (-14, -75),
    (-13, -73),
    (-13, -72),
    (-14, -72),
    (-14, -74),
    (-14, -75),
    (-14, -75),
    (-13, -73),
    (-13, -72),
    (7, -57),
    (7, -59),
    (7, -61),
    (6, -61),
    (9, -57),
    (8, -56),
    (7, -57),
    (7, -59),
    (7, -61),
    (6, -61),
    (9, -57),
    (8, -56),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn act(frames: &[usize]) -> Act {
        let mut a = Act::template(Template::Empty);
        a.actions = frames
            .iter()
            .map(|&n| Action {
                frames: (0..n)
                    .map(|k| Frame {
                        layers: vec![Layer {
                            sprite_index: k as i32,
                            color: [255; 4],
                            scale_x: 1.0,
                            scale_y: 1.0,
                            offset_x: 10.0,
                            offset_y: -20.0,
                            ..Layer::default()
                        }],
                        ..Frame::default()
                    })
                    .collect(),
                interval: 4.0,
            })
            .collect();
        a
    }

    #[test]
    fn index_lists_parse_and_bad_ones_are_refused() {
        let s = parse_indexes("0-2; 4,6").unwrap();
        assert_eq!(s.into_iter().collect::<Vec<_>>(), [0, 1, 2, 4, 6]);
        for bad in ["", "a", "3-1", "1-", "2;x"] {
            assert!(parse_indexes(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn frames_repeat_and_a_layer_goes_to_every_frame_of_chosen_animations() {
        let mut a = act(&[2, 3]);
        a.duplicate_frames(0, 2).unwrap();
        let seq: Vec<i32> = a.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].sprite_index)
            .collect();
        assert_eq!(seq, [0, 1, 0, 1, 0, 1]);
        assert!(a.duplicate_frames(0, 0).is_err() && a.duplicate_frames(5, 1).is_err());
        let mut b = act(&[2; 9]);
        let layer = Layer {
            sprite_index: 7,
            ..Layer::default()
        };
        let n = b.add_layer_to_frames(&BTreeSet::from([1]), &layer, false);
        assert_eq!(n, 2); // only action 8 of the animation 1 exists
        assert_eq!(b.actions[8].frames[0].layers[0].sprite_index, 7);
        assert_eq!(b.actions[0].frames[0].layers.len(), 1);
    }

    #[test]
    fn directions_copy_onto_their_neighbour() {
        let mut a = act(&[1, 2, 3, 4, 5, 6, 7, 8]);
        a.rotation_copy(3, RotationCopy::BottomToBottomRight)
            .unwrap();
        let lens: Vec<usize> = a.actions.iter().map(|x| x.frames.len()).collect();
        assert_eq!(lens, [1, 3, 3, 5, 5, 7, 7, 1]); // 0 -> 7, 2 -> 1, 4 -> 3, 6 -> 5
        let mut a = act(&[1, 2, 3, 4, 5, 6, 7, 8]);
        a.rotation_copy(0, RotationCopy::BottomRightToBottom)
            .unwrap();
        let lens: Vec<usize> = a.actions.iter().map(|x| x.frames.len()).collect();
        assert_eq!(lens, [8, 2, 2, 4, 4, 6, 6, 8]);
        let mut a = act(&[1, 2, 3, 4, 5, 6, 7, 8]);
        a.rotation_copy(0, RotationCopy::BottomToBottomLeft)
            .unwrap();
        assert_eq!(a.actions[1].frames.len(), 1);
        let mut short = act(&[1, 1, 1]);
        assert!(
            short
                .rotation_copy(0, RotationCopy::BottomLeftToBottom)
                .is_err()
        );
    }

    #[test]
    fn frames_and_actions_are_reordered_and_new_ones_are_empty() {
        let mut a = act(&[3]);
        a.set_frame_order(0, &[2, 2, -1, 0]).unwrap();
        let k: Vec<(i32, usize)> = a.actions[0]
            .frames
            .iter()
            .map(|f| {
                (
                    f.layers.first().map_or(-1, |l| l.sprite_index),
                    f.layers.len(),
                )
            })
            .collect();
        assert_eq!(k, [(2, 1), (2, 1), (-1, 0), (0, 1)]);
        assert!(a.set_frame_order(0, &[]).is_err() && a.set_frame_order(0, &[5]).is_err());
        let mut b = act(&[1, 2]);
        b.set_action_order(&[1, -1, 0, 1]).unwrap();
        let lens: Vec<usize> = b.actions.iter().map(|x| x.frames.len()).collect();
        assert_eq!(lens, [2, 1, 1, 2]);
        assert!(b.set_action_order(&[9]).is_err());
    }

    #[test]
    fn effects_turn_colour_and_scale_over_the_frames() {
        let scope = |length, add_empty| EffectScope {
            groups: BTreeSet::from([0]),
            layers: BTreeSet::new(),
            start: 0,
            length,
            loop_missing: true,
            add_empty,
        };
        let mut a = act(&[4]);
        a.apply_effect(
            EffectKind::Fade {
                target: [0, 0, 0, 255],
            },
            0,
            &scope(0, false),
        )
        .unwrap();
        let c: Vec<u8> = a.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].color[0])
            .collect();
        assert_eq!(
            c,
            [255, 191, 128, 64],
            "step / length, not reaching the target"
        );
        // more frames than there are: the sequence loops, and an empty frame can follow
        let mut b = act(&[2]);
        b.apply_effect(
            EffectKind::Overlay {
                color: [0, 0, 0, 255],
            },
            0,
            &scope(6, true),
        )
        .unwrap();
        assert_eq!(b.actions[0].frames.len(), 7);
        assert!(b.actions[0].frames[6].layers.is_empty());
        assert_eq!(
            b.actions[0].frames[2].layers[0].sprite_index, 0,
            "looped from the start"
        );
        // the middle of the breathing is the strongest
        let mut c = act(&[4]);
        c.apply_effect(
            EffectKind::Breathing {
                scale_x: 2.0,
                scale_y: 3.0,
            },
            0,
            &scope(0, false),
        )
        .unwrap();
        let l = &c.actions[0].frames[2].layers[0];
        assert_eq!(
            (l.scale_x, l.scale_y, l.offset_x, l.offset_y),
            (2.0, 3.0, 20.0, -60.0)
        );
        assert_eq!(c.actions[0].frames[0].layers[0].scale_x, 1.0);
        assert_eq!(ease(0, 0.3), 0.3);
        assert!(ease(25, 0.5) > 0.5 && ease(-25, 0.5) < 0.5);
    }

    #[test]
    fn templates_have_the_shape_of_acts_editors() {
        let shape = |t| {
            let a = Act::template(t);
            (
                a.actions.len(),
                a.actions.iter().map(|x| x.frames.len()).sum::<usize>(),
            )
        };
        assert_eq!(shape(Template::Npc), (8, 8));
        assert_eq!(shape(Template::Monster), (40, 504));
        assert_eq!(shape(Template::Homunculus), (64, 648));
        assert_eq!(shape(Template::Weapon), (104, 392));
        let layers = |t| Act::template(t).layers().count();
        assert_eq!(layers(Template::Weapon), 280);
        assert_eq!(layers(Template::Monster), 0);
        assert_eq!(shape(Template::HeadgearMale), (104, 456));
        let m = Act::template(Template::Monster);
        assert_eq!(m.actions[16].frames[11].sound_id, 0);
        assert_eq!(m.sounds.len(), 1);
        let h = Act::template(Template::HeadgearFemale);
        assert_eq!(h.actions[0].frames[0].anchors[0].y, -55);
        assert!(
            h.actions
                .iter()
                .flat_map(|a| &a.frames)
                .all(|f| f.anchors.len() == 1)
        );
        assert_eq!(h.version, V2_4);
        // the bytes read back
        assert!(Act::parse(&m.to_bytes(V2_5).unwrap()).is_ok());
        // every menu name resolves, and the empty sprite next to a new animation reads back
        assert!(
            Template::NAMES
                .iter()
                .all(|&(n, t)| Template::from_name(n) == Some(t))
        );
        assert_eq!(Template::from_name(" Monster "), Some(Template::Monster));
        assert_eq!(Template::from_name("garment"), None);
        let bytes = empty_spr().to_bytes(SprVersion::V2_1).unwrap();
        assert!(Spr::parse(&bytes).unwrap().indexed.is_empty());
    }
}
