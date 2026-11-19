//! The frame editor of an animation: one frame as data and picture, and edits of its layers,
//! anchors and frames. Works on file bytes, so `backend` can apply it to a staged copy.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rgrf_formats::act::{Act, Anchor, Layer, V2_1, V2_3, V2_4, sound_name};
use rgrf_formats::gif_io::upscale;
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::render::{Bounds, Part, bounds_of_parts, layer_at, layer_bounds, render_parts};
use rgrf_formats::spr::{RgbaImage, Spr};
use serde::{Deserialize, Serialize};

/// Most layers or anchors one frame may hold (a guard against typing mistakes).
const MAX_ITEMS: usize = 256;

#[derive(Serialize, Debug)]
pub struct LayerView {
    pub sprite_index: i32,
    pub sprite_type: i32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub mirror: bool,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: i32,
    pub color: [u8; 4],
    /// Where the layer lies in the picture (left, top, right, bottom in picture pixels); `None`
    /// when it draws nothing.
    pub bounds: Option<[i32; 4]>,
}

#[derive(Serialize, Debug)]
pub struct AnchorView {
    pub x: i32,
    pub y: i32,
}

#[derive(Serialize, Debug)]
pub struct ActFrameView {
    pub version: String,
    pub actions: usize,
    pub frames: usize,
    pub action: usize,
    pub frame: usize,
    pub interval_ms: u32,
    pub layers: Vec<LayerView>,
    pub anchors: Vec<AnchorView>,
    /// Versions before 2.3 have no anchors, before 2.4 one scale for both directions.
    pub anchors_supported: bool,
    pub separate_scale: bool,
    /// Sounds exist from ACT 2.1; `sound` is the file played with this frame, `sounds` the table.
    pub sounds_supported: bool,
    pub sound: Option<String>,
    pub sounds: Vec<String>,
    pub indexed_images: usize,
    pub rgba_images: usize,
    pub png: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Where the origin lies in the picture, in picture pixels.
    pub origin_x: i32,
    pub origin_y: i32,
    pub scale: u32,
    /// Why there is no picture (no sprite, or a layer that cannot be drawn).
    pub error: Option<String>,
    /// The reference animations drawn with this frame, and what kept one from showing.
    pub references: Vec<ReferenceView>,
}

#[derive(Serialize, Debug, Clone)]
pub struct ReferenceView {
    pub name: String,
    pub problem: Option<String>,
}

/// Another animation drawn with the edited one, e.g. a head under a headgear: its first anchor
/// meets the edited frame's first anchor, as the client attaches parts to the body.
pub struct Reference<'a> {
    pub name: String,
    pub act: &'a [u8],
    pub spr: Option<&'a [u8]>,
    /// Behind the edited animation instead of in front of it.
    pub under: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ActEdit {
    SetLayer {
        layer: usize,
        offset_x: Option<f32>,
        offset_y: Option<f32>,
        sprite_index: Option<i32>,
        sprite_type: Option<i32>,
        mirror: Option<bool>,
        scale_x: Option<f32>,
        scale_y: Option<f32>,
        rotation: Option<i32>,
        color: Option<[u8; 4]>,
    },
    AddLayer,
    RemoveLayer {
        layer: usize,
    },
    /// Moves the layer to position `to` (layers are drawn in order, the last one on top).
    MoveLayer {
        layer: usize,
        to: usize,
    },
    SetAnchor {
        anchor: usize,
        x: Option<i32>,
        y: Option<i32>,
    },
    AddAnchor,
    RemoveAnchor {
        anchor: usize,
    },
    /// Inserts a copy of the frame after it.
    CopyFrame,
    RemoveFrame,
    /// Moves the frame to position `to` within its action.
    MoveFrame {
        to: usize,
    },
    /// Inserts a copy of the action after it.
    CopyAction,
    /// Removes the action; later actions move down by one (the client addresses actions by number).
    RemoveAction,
    /// Moves the action to position `to`.
    MoveAction {
        to: usize,
    },
    /// In-between frames: between this frame and the next, or (`whole`) between all frames of the
    /// action; `keep_speed` (whole action) shortens the frame time so that the action lasts as long.
    Interpolate {
        count: usize,
        #[serde(default)]
        smooth: bool,
        #[serde(default)]
        whole: bool,
        #[serde(default)]
        wrap: bool,
        #[serde(default)]
        keep_speed: bool,
    },
    /// The sound played with the frame; `None` or an empty name removes it.
    SetSound {
        name: Option<String>,
    },
    /// Time per frame in milliseconds (the file keeps multiples of 25 ms).
    SetInterval {
        ms: f32,
    },
}

fn finite(v: f32, what: &str) -> Result<f32, String> {
    if v.is_finite() && v.abs() < 100_000.0 {
        Ok(v)
    } else {
        Err(format!("{what} must be a reasonable number"))
    }
}

pub fn apply(data: &[u8], action: usize, frame: usize, edit: &ActEdit) -> Result<Vec<u8>, String> {
    let mut act = Act::parse(data).map_err(|e| e.to_string())?;
    let version = act.version;
    let n_actions = act.actions.len();
    match edit {
        ActEdit::CopyAction => act
            .copy_action(action, action + 1)
            .map_err(|e| e.to_string())?,
        ActEdit::RemoveAction => {
            if n_actions <= 1 {
                return Err("an animation keeps at least one action".into());
            }
            act.remove_action(action).map_err(|e| e.to_string())?;
        }
        ActEdit::MoveAction { to } => {
            if action >= n_actions || *to >= n_actions {
                return Err("no such action".into());
            }
            let a = act.actions.remove(action);
            act.actions.insert(*to, a);
        }
        ActEdit::SetInterval { ms } => {
            if !(ms.is_finite() && *ms >= 1.0 && *ms <= 60_000.0) {
                return Err("the interval must be between 1 and 60000 ms".into());
            }
            act.set_interval(action, ms / 25.0)
                .map_err(|e| e.to_string())?;
        }
        ActEdit::Interpolate {
            count,
            smooth,
            whole,
            wrap,
            keep_speed,
        } => {
            let ease = if *smooth {
                rgrf_formats::edit::Ease::Smooth
            } else {
                rgrf_formats::edit::Ease::Linear
            };
            let from = (!whole).then_some(frame);
            act.interpolate(action, from, *count, ease, *wrap)
                .map_err(|e| e.to_string())?;
            if *whole && *keep_speed {
                let a = &mut act.actions[action];
                a.interval = (a.interval / (*count + 1) as f32).max(1.0);
            }
        }
        ActEdit::SetSound { name } => {
            let name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
            act.set_frame_sound(action, frame, name)
                .map_err(|e| e.to_string())?;
        }
        _ => return apply_in_action(act, action, frame, edit),
    }
    act.to_bytes(version).map_err(|e| e.to_string())
}

fn apply_in_action(
    mut act: Act,
    action: usize,
    frame: usize,
    edit: &ActEdit,
) -> Result<Vec<u8>, String> {
    let version = act.version;
    let a = act
        .actions
        .get_mut(action)
        .ok_or_else(|| format!("the animation has no action {action}"))?;
    let n_frames = a.frames.len();
    match edit {
        ActEdit::CopyFrame => {
            let f = a
                .frames
                .get(frame)
                .ok_or_else(|| format!("no frame {frame}"))?
                .clone();
            a.frames.insert(frame + 1, f);
        }
        ActEdit::MoveFrame { to } => {
            if frame >= n_frames || *to >= n_frames {
                return Err("no such frame".into());
            }
            let f = a.frames.remove(frame);
            a.frames.insert(*to, f);
        }
        ActEdit::RemoveFrame => {
            if n_frames <= 1 {
                return Err("an action keeps at least one frame".into());
            }
            if frame >= n_frames {
                return Err(format!("no frame {frame}"));
            }
            a.frames.remove(frame);
        }
        _ => {
            let f = a
                .frames
                .get_mut(frame)
                .ok_or_else(|| format!("no frame {frame}"))?;
            match edit {
                ActEdit::SetLayer {
                    layer,
                    offset_x,
                    offset_y,
                    sprite_index,
                    sprite_type,
                    mirror,
                    scale_x,
                    scale_y,
                    rotation,
                    color,
                } => {
                    let l = f
                        .layers
                        .get_mut(*layer)
                        .ok_or_else(|| format!("no layer {layer}"))?;
                    if let Some(v) = offset_x {
                        l.offset_x = finite(*v, "x")?.round();
                    }
                    if let Some(v) = offset_y {
                        l.offset_y = finite(*v, "y")?.round();
                    }
                    if let Some(v) = sprite_index {
                        if *v < -1 {
                            return Err("the image number is -1 (empty) or more".into());
                        }
                        l.sprite_index = *v;
                    }
                    if let Some(v) = sprite_type {
                        if !matches!(v, 0 | 1) {
                            return Err("the image type is 0 (palette) or 1 (true colour)".into());
                        }
                        l.sprite_type = *v;
                    }
                    if let Some(v) = mirror {
                        l.mirror = *v;
                    }
                    if let Some(v) = scale_x {
                        l.scale_x = finite(*v, "scale")?;
                        if version < V2_4 {
                            l.scale_y = l.scale_x;
                        }
                    }
                    if let Some(v) = scale_y {
                        if version < V2_4 {
                            return Err("this ACT version has one scale for both directions".into());
                        }
                        l.scale_y = finite(*v, "scale")?;
                    }
                    if let Some(v) = rotation {
                        l.rotation = v.rem_euclid(360);
                    }
                    if let Some(v) = color {
                        l.color = *v;
                    }
                }
                ActEdit::AddLayer => {
                    if f.layers.len() >= MAX_ITEMS {
                        return Err("too many layers".into());
                    }
                    f.layers.push(Layer {
                        sprite_index: 0,
                        ..Layer::default()
                    });
                }
                ActEdit::RemoveLayer { layer } => {
                    if *layer >= f.layers.len() {
                        return Err(format!("no layer {layer}"));
                    }
                    f.layers.remove(*layer);
                }
                ActEdit::MoveLayer { layer, to } => {
                    if *layer >= f.layers.len() || *to >= f.layers.len() {
                        return Err("no such layer".into());
                    }
                    let l = f.layers.remove(*layer);
                    f.layers.insert(*to, l);
                }
                ActEdit::SetAnchor { anchor, x, y } => {
                    let an = f
                        .anchors
                        .get_mut(*anchor)
                        .ok_or_else(|| format!("no anchor {anchor}"))?;
                    if let Some(v) = x {
                        an.x = *v;
                    }
                    if let Some(v) = y {
                        an.y = *v;
                    }
                }
                ActEdit::AddAnchor => {
                    if version < V2_3 {
                        return Err("this ACT version has no anchors".into());
                    }
                    if f.anchors.len() >= MAX_ITEMS {
                        return Err("too many anchors".into());
                    }
                    f.anchors.push(Anchor {
                        unknown: [0; 4],
                        x: 0,
                        y: 0,
                        other: 0,
                    });
                }
                ActEdit::RemoveAnchor { anchor } => {
                    if *anchor >= f.anchors.len() {
                        return Err(format!("no anchor {anchor}"));
                    }
                    f.anchors.remove(*anchor);
                }
                _ => unreachable!(),
            }
        }
    }
    act.to_bytes(version).map_err(|e| e.to_string())
}

/// One frame as data, with a picture when the sprite is there and every layer can be drawn.
pub fn view(
    act_data: &[u8],
    spr_data: Option<&[u8]>,
    action: usize,
    frame: usize,
) -> Result<ActFrameView, String> {
    view_with(act_data, spr_data, action, frame, &[])
}

/// [`view`] with reference animations drawn in the same picture.
pub fn view_with(
    act_data: &[u8],
    spr_data: Option<&[u8]>,
    action: usize,
    frame: usize,
    references: &[Reference<'_>],
) -> Result<ActFrameView, String> {
    let act = Act::parse(act_data).map_err(|e| e.to_string())?;
    let actions = act.actions.len();
    let action = action.min(actions.saturating_sub(1));
    let a = act
        .actions
        .get(action)
        .ok_or("the animation has no actions")?;
    let frame = frame.min(a.frames.len().saturating_sub(1));
    let f = a.frames.get(frame).ok_or("the action has no frames")?;
    let spr = spr_data
        .map(|d| Spr::parse(d).map_err(|e| e.to_string()))
        .transpose()?;
    let mut v = ActFrameView {
        version: act.version.to_string(),
        actions,
        frames: a.frames.len(),
        action,
        frame,
        interval_ms: (a.interval * 25.0).round() as u32,
        layers: f
            .layers
            .iter()
            .map(|l| LayerView {
                sprite_index: l.sprite_index,
                sprite_type: l.sprite_type,
                offset_x: l.offset_x,
                offset_y: l.offset_y,
                mirror: l.mirror,
                scale_x: l.scale_x,
                scale_y: l.scale_y,
                rotation: l.rotation,
                color: l.color,
                bounds: None,
            })
            .collect(),
        anchors: f
            .anchors
            .iter()
            .map(|an| AnchorView { x: an.x, y: an.y })
            .collect(),
        anchors_supported: act.version >= V2_3,
        separate_scale: act.version >= V2_4,
        sounds_supported: act.version >= V2_1,
        sound: usize::try_from(f.sound_id)
            .ok()
            .and_then(|i| act.sounds.get(i))
            .map(sound_name),
        sounds: act.sounds.iter().map(sound_name).collect(),
        indexed_images: spr.as_ref().map_or(0, |s| s.indexed.len()),
        rgba_images: spr.as_ref().map_or(0, |s| s.rgba.len()),
        png: None,
        width: 0,
        height: 0,
        origin_x: 0,
        origin_y: 0,
        scale: 1,
        error: None,
        references: Vec::new(),
    };
    let Some(spr) = spr else {
        v.error = Some("the matching .spr is not in this archive".into());
        return Ok(v);
    };
    // a reference that cannot be read is named, the others are still drawn
    let mut loaded = Vec::new();
    for r in references {
        let parsed = Act::parse(r.act).map_err(|e| format!("not a readable ACT: {e}"));
        let parsed = parsed.and_then(|ra| {
            let d = r.spr.ok_or("its .spr is not in this archive")?;
            let rs = Spr::parse(d).map_err(|e| format!("its .spr is not readable: {e}"))?;
            Ok((ra, rs))
        });
        let problem = match &parsed {
            Err(e) => Some(e.clone()),
            Ok((ra, _))
                if ra
                    .actions
                    .get(action)
                    .and_then(|x| x.frames.get(frame))
                    .is_none() =>
            {
                Some(format!("has no frame {} in action {action}", frame + 1))
            }
            Ok(_) if f.anchors.is_empty() => {
                Some("this frame has no anchor, so it is drawn at the origin".into())
            }
            Ok(_) => None,
        };
        v.references.push(ReferenceView {
            name: r.name.clone(),
            problem,
        });
        if let Ok(p) = parsed {
            loaded.push((p, r.under));
        }
    }
    let mut parts = vec![Part::new(&act, &spr)];
    parts.extend(loaded.iter().map(|((ra, rs), under)| Part {
        anchored: true,
        under: *under,
        ..Part::new(ra, rs)
    }));
    // the picture keeps the size of the whole action, so frames can be compared while stepping
    let bounds = match bounds_of_parts(&parts, action, 0..a.frames.len()) {
        Ok(b) => b.unwrap_or(Bounds {
            min_x: -32,
            min_y: -32,
            max_x: 32,
            max_y: 32,
        }),
        Err(e) => {
            v.error = Some(e.to_string());
            return Ok(v);
        }
    };
    let canvas = match render_parts(&parts, action, frame, bounds) {
        Ok(c) => c,
        Err(e) => {
            v.error = Some(e.to_string());
            return Ok(v);
        }
    };
    let (w, h) = (canvas.width as usize, canvas.height as usize);
    let scale = if w <= 100 && h <= 100 {
        4
    } else if w <= 250 {
        2
    } else {
        1
    };
    let img = RgbaImage {
        width: (w * scale) as u16,
        height: (h * scale) as u16,
        pixels: upscale(&canvas.pixels, w, h, scale),
    };
    v.png = Some(B64.encode(rgba_to_png(&img).map_err(|e| e.to_string())?));
    v.width = (w * scale) as u32;
    v.height = (h * scale) as u32;
    v.scale = scale as u32;
    v.origin_x = -bounds.min_x * scale as i32;
    v.origin_y = -bounds.min_y * scale as i32;
    let k = scale as i32;
    for (l, b) in v
        .layers
        .iter_mut()
        .zip(layer_bounds(&act, &spr, action, frame))
    {
        l.bounds = b.map(|b| {
            [
                (b.min_x - bounds.min_x) * k,
                (b.min_y - bounds.min_y) * k,
                (b.max_x - bounds.min_x) * k,
                (b.max_y - bounds.min_y) * k,
            ]
        });
    }
    Ok(v)
}

/// The topmost layer with a visible pixel at a point of the frame (frame coordinates, origin 0).
pub fn layer_at_point(
    act_data: &[u8],
    spr_data: &[u8],
    action: usize,
    frame: usize,
    x: f64,
    y: f64,
) -> Result<Option<usize>, String> {
    let act = Act::parse(act_data).map_err(|e| e.to_string())?;
    let spr = Spr::parse(spr_data).map_err(|e| e.to_string())?;
    Ok(layer_at(&act, &spr, action, frame, x, y))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgrf_formats::act::{Action, Frame, V2_5};
    use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, Version};

    fn sample() -> (Vec<u8>, Vec<u8>) {
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[255, 0, 0, 0]);
        let spr = Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 4,
                height: 4,
                pixels: vec![1; 16],
            }],
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        };
        let layer = Layer {
            sprite_index: 0,
            ..Layer::default()
        };
        let frame = Frame {
            layers: vec![layer],
            ..Frame::default()
        };
        let act = Act {
            version: V2_5,
            reserved: [0; 10],
            actions: vec![Action {
                frames: vec![frame],
                interval: 4.0,
            }],
            sounds: vec![],
            trailing: vec![],
            warnings: vec![],
        };
        (
            act.to_bytes(V2_5).unwrap(),
            spr.to_bytes(Version::V2_1).unwrap(),
        )
    }

    fn edit(json: serde_json::Value) -> ActEdit {
        serde_json::from_value(json).unwrap()
    }

    fn parsed(data: &[u8]) -> Act {
        Act::parse(data).unwrap()
    }

    #[test]
    fn adds_in_between_frames() {
        let (act, _) = sample();
        let moved = apply(&act, 0, 0, &edit(serde_json::json!({"op": "copy_frame"}))).unwrap();
        let moved = apply(
            &moved,
            0,
            1,
            &edit(serde_json::json!({"op": "set_layer", "layer": 0, "offset_x": 10})),
        )
        .unwrap();
        let one = apply(
            &moved,
            0,
            0,
            &edit(serde_json::json!({"op": "interpolate", "count": 1})),
        )
        .unwrap();
        let xs: Vec<f32> = parsed(&one).actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].offset_x)
            .collect();
        assert_eq!(xs, [0.0, 5.0, 10.0]);
        let whole = apply(
            &moved,
            0,
            0,
            &edit(serde_json::json!({"op": "interpolate", "count": 3, "whole": true, "wrap": true, "keep_speed": true})),
        )
        .unwrap();
        let a = &parsed(&whole).actions[0];
        // 2 frames and 2 gaps (one wraps) of 3 each, the frame time shrinks to a quarter
        assert_eq!((a.frames.len(), a.interval), (8, 1.0));
        // the last frame has no next frame to blend with
        assert!(
            apply(
                &moved,
                0,
                1,
                &edit(serde_json::json!({"op": "interpolate", "count": 1}))
            )
            .is_err()
        );
    }

    #[test]
    fn edits_layers_anchors_and_frames() {
        let (act, spr) = sample();
        let v = view(&act, Some(&spr), 0, 0).unwrap();
        assert_eq!((v.layers.len(), v.frames, v.scale), (1, 1, 4));
        assert!(v.png.is_some() && v.error.is_none());
        assert!(v.anchors_supported && v.separate_scale);
        // the only layer fills the picture (picture pixels, scaled like the picture)
        assert_eq!(
            v.layers[0].bounds,
            Some([0, 0, v.width as i32, v.height as i32])
        );
        assert_eq!(layer_at_point(&act, &spr, 0, 0, 0.0, 0.0).unwrap(), Some(0));
        assert_eq!(layer_at_point(&act, &spr, 0, 0, 99.0, 0.0).unwrap(), None);

        // fields that are not given stay as they are; rotation wraps into 0..360
        let a = apply(
            &act,
            0,
            0,
            &edit(serde_json::json!({"op": "set_layer", "layer": 0, "offset_x": 10, "rotation": -90, "mirror": true})),
        )
        .unwrap();
        let l = &parsed(&a).actions[0].frames[0].layers[0];
        assert_eq!(
            (l.offset_x, l.rotation, l.mirror, l.sprite_index),
            (10.0, 270, true, 0)
        );

        // layers: add, move to the bottom, remove
        let a = apply(&a, 0, 0, &edit(serde_json::json!({"op": "add_layer"}))).unwrap();
        let a = apply(
            &a,
            0,
            0,
            &edit(serde_json::json!({"op": "move_layer", "layer": 1, "to": 0})),
        )
        .unwrap();
        let layers = &parsed(&a).actions[0].frames[0].layers;
        assert_eq!(
            (layers.len(), layers[0].offset_x, layers[1].offset_x),
            (2, 0.0, 10.0)
        );
        let a = apply(
            &a,
            0,
            0,
            &edit(serde_json::json!({"op": "remove_layer", "layer": 0})),
        )
        .unwrap();
        assert_eq!(parsed(&a).actions[0].frames[0].layers.len(), 1);

        // anchors
        let a = apply(&a, 0, 0, &edit(serde_json::json!({"op": "add_anchor"}))).unwrap();
        let a = apply(
            &a,
            0,
            0,
            &edit(serde_json::json!({"op": "set_anchor", "anchor": 0, "x": 3, "y": -4})),
        )
        .unwrap();
        let an = &parsed(&a).actions[0].frames[0].anchors[0];
        assert_eq!((an.x, an.y), (3, -4));

        // frames: a copy is inserted after, the last frame cannot go
        let one = apply(&a, 0, 0, &edit(serde_json::json!({"op": "remove_frame"})));
        assert!(one.is_err(), "an action keeps at least one frame");
        let two = apply(&a, 0, 0, &edit(serde_json::json!({"op": "copy_frame"}))).unwrap();
        assert_eq!(parsed(&two).actions[0].frames.len(), 2);
        let back = apply(&two, 0, 1, &edit(serde_json::json!({"op": "remove_frame"}))).unwrap();
        assert_eq!(parsed(&back).actions[0].frames.len(), 1);
        assert_eq!(
            view(&two, Some(&spr), 0, 99).unwrap().frame,
            1,
            "frame is clamped"
        );

        // wrong input is an error, never a damaged file
        for bad in [
            serde_json::json!({"op": "set_layer", "layer": 5, "offset_x": 1}),
            serde_json::json!({"op": "set_layer", "layer": 0, "sprite_index": -7}),
            serde_json::json!({"op": "set_layer", "layer": 0, "sprite_type": 2}),
            serde_json::json!({"op": "set_layer", "layer": 0, "scale_x": 1e30}),
            serde_json::json!({"op": "move_layer", "layer": 0, "to": 9}),
            serde_json::json!({"op": "remove_anchor", "anchor": 0}),
        ] {
            assert!(apply(&act, 0, 0, &edit(bad.clone())).is_err(), "{bad}");
        }
        assert!(apply(&act, 3, 0, &edit(serde_json::json!({"op": "add_layer"}))).is_err());
    }

    #[test]
    fn a_frame_sound_is_set_listed_and_cleared() {
        let (act, _) = sample();
        let set = |d: &[u8], name: serde_json::Value| {
            apply(
                d,
                0,
                0,
                &edit(serde_json::json!({"op": "set_sound", "name": name})),
            )
        };
        let a = set(&act, "effect\\hit.wav".into()).unwrap();
        let v = view(&a, None, 0, 0).unwrap();
        assert!(v.sounds_supported);
        assert_eq!(v.sound.as_deref(), Some("effect\\hit.wav"));
        assert_eq!(v.sounds, ["effect\\hit.wav"]);
        let b = set(&a, serde_json::Value::Null).unwrap();
        let v = view(&b, None, 0, 0).unwrap();
        assert_eq!(v.sound, None);
        assert_eq!(v.sounds.len(), 1, "the table keeps the name");
        assert!(set(&act, "x".repeat(60).into()).is_err());
    }

    #[test]
    fn edits_actions_intervals_and_frame_order() {
        let (act, _) = sample();
        let two = apply(&act, 0, 0, &edit(serde_json::json!({"op": "copy_action"}))).unwrap();
        assert_eq!(parsed(&two).actions.len(), 2);
        // the copy gets its own interval; the original keeps 100 ms
        let t = apply(
            &two,
            1,
            0,
            &edit(serde_json::json!({"op": "set_interval", "ms": 200})),
        )
        .unwrap();
        let acts = parsed(&t).actions;
        assert_eq!((acts[0].interval, acts[1].interval), (4.0, 8.0));
        let moved = apply(
            &t,
            1,
            0,
            &edit(serde_json::json!({"op": "move_action", "to": 0})),
        )
        .unwrap();
        let acts = parsed(&moved).actions;
        assert_eq!((acts[0].interval, acts[1].interval), (8.0, 4.0));
        let one = apply(
            &moved,
            0,
            0,
            &edit(serde_json::json!({"op": "remove_action"})),
        )
        .unwrap();
        assert_eq!(parsed(&one).actions.len(), 1);
        assert!(
            apply(
                &one,
                0,
                0,
                &edit(serde_json::json!({"op": "remove_action"}))
            )
            .is_err()
        );
        assert!(
            apply(
                &one,
                0,
                0,
                &edit(serde_json::json!({"op": "set_interval", "ms": 0}))
            )
            .is_err()
        );
        assert!(
            apply(
                &one,
                0,
                0,
                &edit(serde_json::json!({"op": "move_action", "to": 4}))
            )
            .is_err()
        );
        assert!(apply(&one, 5, 0, &edit(serde_json::json!({"op": "copy_action"}))).is_err());

        // frames: mark the first with an offset, copy it, move the copy to the front
        let f = apply(
            &act,
            0,
            0,
            &edit(serde_json::json!({"op": "set_layer", "layer": 0, "offset_x": 5})),
        )
        .unwrap();
        let f = apply(&f, 0, 0, &edit(serde_json::json!({"op": "copy_frame"}))).unwrap();
        let f = apply(
            &f,
            0,
            1,
            &edit(serde_json::json!({"op": "set_layer", "layer": 0, "offset_x": 9})),
        )
        .unwrap();
        let f = apply(
            &f,
            0,
            1,
            &edit(serde_json::json!({"op": "move_frame", "to": 0})),
        )
        .unwrap();
        let frames = &parsed(&f).actions[0].frames;
        assert_eq!(
            (frames[0].layers[0].offset_x, frames[1].layers[0].offset_x),
            (9.0, 5.0)
        );
        assert!(
            apply(
                &f,
                0,
                0,
                &edit(serde_json::json!({"op": "move_frame", "to": 7}))
            )
            .is_err()
        );
    }

    #[test]
    fn a_layer_that_cannot_be_drawn_gives_data_without_picture() {
        let (act, spr) = sample();
        let a = apply(
            &act,
            0,
            0,
            &edit(serde_json::json!({"op": "set_layer", "layer": 0, "sprite_index": 40})),
        )
        .unwrap();
        let v = view(&a, Some(&spr), 0, 0).unwrap();
        assert_eq!(v.layers[0].sprite_index, 40);
        let v = view(&a, None, 0, 0).unwrap();
        assert!(v.png.is_none() && v.error.is_some());
    }
}
