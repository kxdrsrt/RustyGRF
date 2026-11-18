//! The frame editor of an animation: one frame as data and picture, and edits of its layers,
//! anchors and frames. Works on file bytes, so `backend` can apply it to a staged copy.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rgrf_formats::act::{Act, Anchor, Layer, V2_3, V2_4};
use rgrf_formats::gif_io::upscale;
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::render::{Bounds, bounds_of, render_frame};
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
                ActEdit::CopyFrame | ActEdit::RemoveFrame => unreachable!(),
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
            })
            .collect(),
        anchors: f
            .anchors
            .iter()
            .map(|an| AnchorView { x: an.x, y: an.y })
            .collect(),
        anchors_supported: act.version >= V2_3,
        separate_scale: act.version >= V2_4,
        indexed_images: spr.as_ref().map_or(0, |s| s.indexed.len()),
        rgba_images: spr.as_ref().map_or(0, |s| s.rgba.len()),
        png: None,
        width: 0,
        height: 0,
        origin_x: 0,
        origin_y: 0,
        scale: 1,
        error: None,
    };
    let Some(spr) = spr else {
        v.error = Some("the matching .spr is not in this archive".into());
        return Ok(v);
    };
    // the picture keeps the size of the whole action, so frames can be compared while stepping
    let bounds = match bounds_of(&act, &spr, action, 0..a.frames.len()) {
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
    let canvas = match render_frame(&act, &spr, action, frame, bounds) {
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
    Ok(v)
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
    fn edits_layers_anchors_and_frames() {
        let (act, spr) = sample();
        let v = view(&act, Some(&spr), 0, 0).unwrap();
        assert_eq!((v.layers.len(), v.frames, v.scale), (1, 1, 4));
        assert!(v.png.is_some() && v.error.is_none());
        assert!(v.anchors_supported && v.separate_scale);

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
