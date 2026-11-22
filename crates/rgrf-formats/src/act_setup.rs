//! ACT Editor's "Setup Headgear" and "Setup Head" (`HeadEditorDialog`, `AnimationMenu.cs`, with
//! permission, see `docs/05-licensing.md`). A headgear or head animation follows the head
//! animation of the body: every frame shows the headgear image that belongs to the head image the
//! reference shows in that frame, shifted by the same amount. So the artist only places each
//! image once against its reference image, and the whole animation (all directions, frames and
//! mirrored copies) is built from those placements.
//!
//! "Setup Garment" (`HeadEditorDialog` with flag 2) is the same with the body as the reference:
//! ACT Editor switched it off ("just too buggy"). Its intent is rebuilt here without the bugs
//! (see `docs/10-feature-matrix.md`): no limit to the first five body images, no head poses and
//! no half-pixel rule, which only make sense for a head.

use thiserror::Error;

use crate::act::{Act, Layer};
use crate::spr::Spr;

/// Which animation is set up. A head has an empty layer in front of its picture (the layer that
/// the body animation attaches to), a headgear does not. A garment follows the body animation
/// instead of the head one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupKind {
    Headgear,
    Head,
    Garment,
}

/// Where the animation's own image sits against one image of the reference head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    /// Number of the image in the animation's sprite, palette images first and then true-colour
    /// ones; `-1` leaves the frames that show the reference image as they are.
    pub image: i32,
    /// How far the image lies from the reference image (as drawn without mirroring).
    pub dx: i32,
    pub dy: i32,
}

/// A reference image, the frame that shows it first and what the animation shows there now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Model {
    pub reference_image: usize,
    /// First frame of the reference that shows the image without mirroring (action, frame).
    pub usage: Option<(usize, usize)>,
    pub placement: Placement,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SetupError {
    #[error("the reference animation shows none of its images")]
    EmptyReference,
    #[error("the sprite has no image {0}")]
    NoImage(i32),
}

fn absolute(spr: &Spr, l: &Layer) -> i32 {
    if l.sprite_index < 0 {
        -1
    } else if l.sprite_type == 1 {
        spr.indexed.len() as i32 + l.sprite_index
    } else {
        l.sprite_index
    }
}

fn first_picture(frame: &crate::act::Frame) -> Option<&Layer> {
    frame.layers.iter().find(|l| l.sprite_index > -1)
}

fn image_width(spr: &Spr, abs: i32) -> Option<u16> {
    let i = usize::try_from(abs).ok()?;
    match spr.indexed.get(i) {
        Some(img) => Some(img.width),
        None => spr.rgba.get(i - spr.indexed.len()).map(|img| img.width),
    }
}

/// The first frame of `reference` that shows palette image `image` without mirroring.
fn first_usage(reference: &Act, image: usize) -> Option<(usize, usize, &Layer)> {
    reference
        .layers()
        .find(|(_, _, _, l)| l.sprite_type == 0 && l.sprite_index == image as i32 && !l.mirror)
        .map(|(a, f, _, l)| (a, f, l))
}

/// The frame of `reference` that goes with frame `frame` of action `action` of the animation.
/// Standing and sitting actions of a head have three frames (looking ahead, right, left); an
/// animation with more frames shows each of them for a third of its frames. Other references
/// (a body) have no such poses.
fn reference_frame(act: &Act, reference: &Act, action: usize, frame: usize, poses: bool) -> usize {
    let ref_frames = reference.actions[action].frames.len();
    let own = act.actions[action].frames.len();
    let looks = poses && ((ref_frames == 3 && action < 8) || (16..24).contains(&action));
    if looks && own / 3 > 0 {
        let group = own / 3;
        let part = (frame / group).min(2);
        return part.min(ref_frames.saturating_sub(1));
    }
    if frame >= ref_frames {
        return if ref_frames > 0 {
            frame % ref_frames
        } else {
            0
        };
    }
    frame
}

/// What the animation shows for each of the first `images` images of the reference now: the
/// starting point of the dialog, so that applying it unchanged keeps the animation as it is.
pub fn current_models(act: &Act, spr: &Spr, reference: &Act, images: usize) -> Vec<Model> {
    (0..images)
        .map(|i| {
            let none = Model {
                reference_image: i,
                usage: None,
                placement: Placement {
                    image: -1,
                    dx: 0,
                    dy: 0,
                },
            };
            let Some((ua, uf, ref_layer)) = first_usage(reference, i) else {
                return none;
            };
            // the first of the non-mirrored uses with a picture in the animation
            let found = reference
                .layers()
                .filter(|(_, _, _, l)| {
                    l.sprite_type == 0 && l.sprite_index == i as i32 && !l.mirror
                })
                .find_map(|(a, f, _, _)| {
                    act.actions
                        .get(a)
                        .and_then(|ac| ac.frames.get(f))
                        .and_then(first_picture)
                });
            match found {
                Some(l) => Model {
                    reference_image: i,
                    usage: Some((ua, uf)),
                    placement: Placement {
                        image: absolute(spr, l),
                        dx: (l.offset_x - ref_layer.offset_x).round() as i32,
                        dy: (l.offset_y - ref_layer.offset_y).round() as i32,
                    },
                },
                None => Model {
                    usage: Some((ua, uf)),
                    ..none
                },
            }
        })
        .collect()
}

/// Builds the frames of `act` from the placements (one per reference image, `placements[i]` is
/// for reference image `i`). Every frame ends up with one picture layer, plus the empty first
/// layer of a head. Returns the number of frames set.
pub fn apply(
    act: &mut Act,
    spr: &Spr,
    reference: &Act,
    reference_spr: &Spr,
    placements: &[Placement],
    kind: SetupKind,
) -> Result<usize, SetupError> {
    if reference.layers().all(|(_, _, _, l)| l.sprite_index < 0) {
        return Err(SetupError::EmptyReference);
    }
    let total = spr.indexed.len() + spr.rgba.len();
    for p in placements {
        if p.image >= total as i32 || p.image < -1 {
            return Err(SetupError::NoImage(p.image));
        }
    }
    // scale and rotation come from what the animation shows now; read them before it changes
    let before: Vec<Option<(f32, f32, i32)>> =
        current_models(act, spr, reference, placements.len())
            .iter()
            .map(|m| {
                let (a, f) = m.usage?;
                let layer = act
                    .actions
                    .get(a)
                    .and_then(|ac| ac.frames.get(f))
                    .and_then(first_picture)?;
                Some((layer.scale_x, layer.scale_y, layer.rotation))
            })
            .collect();

    let mut changed = 0;
    let actions = act.actions.len().min(reference.actions.len());
    for aid in 0..actions {
        for fid in 0..act.actions[aid].frames.len() {
            // one layer per frame
            {
                let frame = &mut act.actions[aid].frames[fid];
                frame.layers.truncate(1);
                if frame.layers.is_empty() {
                    frame.layers.push(Layer::default());
                }
            }
            if reference.actions[aid].frames.is_empty() {
                continue;
            }
            let rf = reference_frame(act, reference, aid, fid, kind != SetupKind::Garment);
            let ref_frame = &reference.actions[aid].frames[rf];
            let Some(ref_layer) = first_picture(ref_frame) else {
                continue;
            };
            if ref_layer.sprite_type != 0 || ref_layer.sprite_index as usize >= placements.len() {
                continue;
            }
            let ri = ref_layer.sprite_index as usize;
            let place = placements[ri];
            if place.image < 0 {
                continue;
            }
            let (sx, sy, rot) = before[ri].unwrap_or((1.0, 1.0, 0));
            let ref_anchor = ref_frame.anchors.first().map(|a| (a.x, a.y));

            let frame = &mut act.actions[aid].frames[fid];
            if let (Some(own), Some((ax, ay))) = (frame.anchors.first_mut(), ref_anchor) {
                own.x = ax;
                own.y = ay;
            }
            let layer = &mut frame.layers[0];
            if (place.image as usize) < spr.indexed.len() {
                layer.sprite_type = 0;
                layer.sprite_index = place.image;
            } else {
                layer.sprite_type = 1;
                layer.sprite_index = place.image - spr.indexed.len() as i32;
            }
            layer.mirror = ref_layer.mirror;
            layer.offset_x = ref_layer.offset_x;
            layer.offset_y = ref_layer.offset_y;
            layer.scale_x = sx;
            layer.scale_y = sy;
            layer.rotation = rot;

            let (mut vx, vy) = (place.dx, place.dy);
            if layer.mirror {
                vx = -vx;
                // a mirrored picture of odd and even width sits half a pixel apart
                let own_w = image_width(spr, place.image).unwrap_or(0) % 2;
                let ref_w =
                    image_width(reference_spr, absolute(reference_spr, ref_layer)).unwrap_or(0) % 2;
                if kind != SetupKind::Garment && own_w != ref_w {
                    if own_w == 0 {
                        vx -= 1;
                    }
                    if ref_w == 0 {
                        vx += 1;
                    }
                }
                if layer.rotation > 0 {
                    let r = 360 - layer.rotation;
                    layer.rotation = if r < 0 { r + 360 } else { r };
                }
            }
            layer.offset_x += vx as f32;
            layer.offset_y += vy as f32;

            if kind == SetupKind::Head {
                let mut empty = frame.layers[0].clone();
                empty.sprite_index = -1;
                frame.layers.insert(0, empty);
            }
            changed += 1;
        }
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Frame};
    use crate::spr::IndexedImage;

    fn layer(image: i32, x: f32, y: f32, mirror: bool) -> Layer {
        Layer {
            sprite_index: image,
            offset_x: x,
            offset_y: y,
            mirror,
            ..Layer::default()
        }
    }

    fn act(actions: Vec<Vec<Layer>>) -> Act {
        let mut a = Act::template(crate::act_ops::Template::Empty);
        a.actions = actions
            .into_iter()
            .map(|frames| Action {
                frames: frames
                    .into_iter()
                    .map(|l| Frame {
                        layers: vec![l],
                        ..Frame::default()
                    })
                    .collect(),
                ..Action::default()
            })
            .collect();
        a
    }

    fn spr(widths: &[u16]) -> Spr {
        Spr {
            version: crate::act::V2_1,
            indexed: widths
                .iter()
                .map(|&w| IndexedImage {
                    width: w,
                    height: 2,
                    pixels: vec![1; usize::from(w) * 2],
                })
                .collect(),
            rgba: Vec::new(),
            palette: Some(Box::new([0; crate::spr::PALETTE_LEN])),
            warnings: Vec::new(),
        }
    }

    // the reference head: action 0 looks ahead, right and left, action 1 is the same mirrored
    fn reference() -> Act {
        act(vec![
            vec![
                layer(0, 0.0, -30.0, false),
                layer(1, 2.0, -30.0, false),
                layer(2, -2.0, -30.0, false),
            ],
            vec![
                layer(0, 0.0, -30.0, true),
                layer(1, -2.0, -30.0, true),
                layer(2, 2.0, -30.0, true),
            ],
        ])
    }

    #[test]
    fn what_the_animation_shows_now_is_read_against_the_reference() {
        let r = reference();
        let a = act(vec![
            vec![
                layer(1, 5.0, -40.0, false),
                layer(0, 7.0, -39.0, false),
                layer(1, 0.0, 0.0, false),
            ],
            vec![layer(0, 0.0, 0.0, false)],
        ]);
        let m = current_models(&a, &spr(&[4, 4]), &r, 3);
        assert_eq!(m.len(), 3);
        assert_eq!(m[0].usage, Some((0, 0)));
        // reference image 0 is shown in frame 0 of action 0, where the animation has image 1
        assert_eq!(
            m[0].placement,
            Placement {
                image: 1,
                dx: 5,
                dy: -10
            }
        );
        assert_eq!(m[1].placement.image, 0);
        assert_eq!(m[1].placement.dx, 5);
    }

    #[test]
    fn applying_the_current_state_keeps_it_and_a_shift_follows_the_mirror() {
        let r = reference();
        let rs = spr(&[3, 4, 5]);
        let own = spr(&[4, 4, 4]);
        let mut a = act(vec![
            vec![
                layer(0, 1.0, -31.0, false),
                layer(1, 4.0, -31.0, false),
                layer(2, 0.0, -31.0, false),
            ],
            vec![
                layer(0, 0.0, 0.0, true),
                layer(1, 0.0, 0.0, true),
                layer(2, 0.0, 0.0, true),
            ],
        ]);
        let models = current_models(&a, &own, &r, 3);
        let mut p: Vec<Placement> = models.iter().map(|m| m.placement).collect();
        assert_eq!(
            p[0],
            Placement {
                image: 0,
                dx: 1,
                dy: -1
            }
        );
        let n = apply(&mut a, &own, &r, &rs, &p, SetupKind::Headgear).unwrap();
        assert_eq!(n, 6);
        let l = &a.actions[0].frames[1].layers[0];
        assert_eq!((l.offset_x, l.offset_y, l.sprite_index), (4.0, -31.0, 1));
        // the mirrored frame: the shift is turned around (the widths 4 and 3 differ by a pixel,
        // the even picture against the odd reference: one pixel further)
        let m = &a.actions[1].frames[0].layers[0];
        assert!(m.mirror);
        assert_eq!((m.offset_x, m.offset_y), (-1.0 - 1.0, -31.0));
        // moving image 1 two pixels to the right moves its frames in both actions
        p[1].dx += 2;
        apply(&mut a, &own, &r, &rs, &p, SetupKind::Headgear).unwrap();
        assert_eq!(a.actions[0].frames[1].layers[0].offset_x, 6.0);
        assert_eq!(a.actions[1].frames[1].layers[0].offset_x, -6.0);
    }

    #[test]
    fn a_head_gets_an_empty_layer_in_front_and_an_unset_image_is_skipped() {
        let r = reference();
        let rs = spr(&[4, 4, 4]);
        let own = spr(&[4, 4]);
        let mut a = act(vec![
            vec![layer(0, 9.0, 9.0, false); 3],
            vec![layer(0, 9.0, 9.0, false); 3],
        ]);
        let p = [
            Placement {
                image: 1,
                dx: 0,
                dy: 0,
            },
            Placement {
                image: -1,
                dx: 0,
                dy: 0,
            },
            Placement {
                image: 0,
                dx: 0,
                dy: 0,
            },
        ];
        let n = apply(&mut a, &own, &r, &rs, &p, SetupKind::Head).unwrap();
        assert_eq!(n, 4);
        let f = &a.actions[0].frames[0];
        assert_eq!(f.layers.len(), 2);
        assert_eq!(f.layers[0].sprite_index, -1);
        assert_eq!(f.layers[1].sprite_index, 1);
        // frame 1 shows reference image 1: not set, so it stays as it was (one layer)
        assert_eq!(a.actions[0].frames[1].layers.len(), 1);
        assert_eq!(a.actions[0].frames[1].layers[0].offset_x, 9.0);
        // an image the sprite does not have is refused, and so is a reference without pictures
        let bad = [Placement {
            image: 7,
            dx: 0,
            dy: 0,
        }];
        assert_eq!(
            apply(&mut a, &own, &r, &rs, &bad, SetupKind::Head),
            Err(SetupError::NoImage(7))
        );
        let empty = act(vec![vec![Layer::default()]]);
        assert_eq!(
            apply(&mut a, &own, &empty, &rs, &p, SetupKind::Head),
            Err(SetupError::EmptyReference)
        );
    }

    #[test]
    fn a_garment_follows_the_body_frame_by_frame_without_head_rules() {
        // a body: three frames in action 0, its mirror in action 1, the pictures 0 to 2
        let body = reference();
        let rs = spr(&[3, 4, 5]);
        let own = spr(&[4, 4, 4]);
        let mut a = act(vec![
            vec![layer(0, 9.0, 9.0, false); 6],
            vec![layer(0, 9.0, 9.0, true); 3],
        ]);
        let p = [
            Placement {
                image: 0,
                dx: 1,
                dy: 0,
            },
            Placement {
                image: 1,
                dx: 1,
                dy: 0,
            },
            Placement {
                image: 2,
                dx: 1,
                dy: 0,
            },
        ];
        let n = apply(&mut a, &own, &body, &rs, &p, SetupKind::Garment).unwrap();
        assert_eq!(n, 9);
        // six frames against three: the body frames repeat, no third of the frames per pose
        let used: Vec<i32> = a.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].sprite_index)
            .collect();
        assert_eq!(used, [0, 1, 2, 0, 1, 2]);
        // one layer only, and a mirrored frame turns the shift around without a half pixel
        assert!(a.actions[0].frames.iter().all(|f| f.layers.len() == 1));
        let m = &a.actions[1].frames[0].layers[0];
        assert!(m.mirror);
        assert_eq!((m.offset_x, m.offset_y), (-1.0, -30.0));
    }

    #[test]
    fn an_animation_with_more_frames_shows_each_head_pose_for_a_share_of_them() {
        let r = reference();
        let rs = spr(&[4, 4, 4]);
        let own = spr(&[4, 4, 4]);
        let mut a = act(vec![vec![layer(0, 0.0, 0.0, false); 6]]);
        let p = [
            Placement {
                image: 0,
                dx: 0,
                dy: 0,
            },
            Placement {
                image: 1,
                dx: 0,
                dy: 0,
            },
            Placement {
                image: 2,
                dx: 0,
                dy: 0,
            },
        ];
        apply(&mut a, &own, &r, &rs, &p, SetupKind::Headgear).unwrap();
        let used: Vec<i32> = a.actions[0]
            .frames
            .iter()
            .map(|f| f.layers[0].sprite_index)
            .collect();
        assert_eq!(used, [0, 0, 1, 1, 2, 2]);
    }
}
