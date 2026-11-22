//! ACT Editor's "Save as garment" (`SaveGarmentDialog.cs`, with permission, see
//! `docs/05-licensing.md`): one animation becomes the animations of a garment (cape, wings) for
//! every job and both genders. Either the animation is copied as it is (moved by an offset), or
//! the animations of the kRO garment whose sprite is the closest in picture sizes are taken, and
//! their offsets are corrected for the size differences of the pictures. That keeps the offsets
//! kRO's artists set for each job.

use crate::act::Act;
use crate::spr::Spr;

/// ACT Editor's list of the jobs (`def_garment_paths.txt`, CP949): a line is one job name, or
/// `male#female` when the two differ; `//` starts a comment.
pub const DEFAULT_JOB_PATHS: &[u8] = include_bytes!("../data/def_garment_paths.txt");

/// The job names of a list as bytes (the encoding of the list stays as it is): `(male, female)`.
pub fn job_paths(text: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    text.split(|&b| b == b'\n')
        .filter_map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let line = match line.windows(2).position(|w| w == b"//") {
                Some(at) => &line[..at],
                None => line,
            };
            let end = line
                .iter()
                .rposition(|&b| b != b' ' && b != b'\t')
                .map_or(0, |i| i + 1);
            let line = &line[..end];
            if line.len() <= 1 {
                return None;
            }
            Some(match line.iter().position(|&b| b == b'#') {
                Some(at) => (line[..at].to_vec(), line[at + 1..].to_vec()),
                None => (line.to_vec(), line.to_vec()),
            })
        })
        .collect()
}

/// The size of every picture of a sprite (palette pictures first, then true-colour ones).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sizes {
    pub sizes: Vec<(u32, u32)>,
    /// Number of palette pictures: where a layer of type 1 starts counting.
    pub indexed: usize,
}

impl Sizes {
    pub fn of(spr: &Spr) -> Self {
        let mut sizes: Vec<_> = spr
            .indexed
            .iter()
            .map(|i| (u32::from(i.width), u32::from(i.height)))
            .collect();
        sizes.extend(
            spr.rgba
                .iter()
                .map(|i| (u32::from(i.width), u32::from(i.height))),
        );
        Self {
            sizes,
            indexed: spr.indexed.len(),
        }
    }

    /// The sizes once the transparent border of every picture is cut off (ACT Editor compares
    /// with both: artists often leave a border).
    pub fn trimmed(spr: &Spr) -> Self {
        Self::of(&trim_sprite(spr))
    }

    fn of_layer(&self, l: &crate::act::Layer) -> Option<(u32, u32)> {
        let i = usize::try_from(l.sprite_index).ok()?;
        let i = if l.sprite_type == 1 {
            self.indexed + i
        } else {
            i
        };
        self.sizes.get(i).copied()
    }
}

/// The rectangle of a picture that is not transparent: `(x0, y0, x1, y1)`; `None` for a picture
/// without a visible pixel.
fn box_of(
    w: usize,
    h: usize,
    opaque: impl Fn(usize, usize) -> bool,
) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if opaque(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    (x1 > x0 && y1 > y0).then_some((x0, y0, x1, y1))
}

fn crop<T: Copy>(
    src: &[T],
    w: usize,
    per: usize,
    (x0, y0, x1, y1): (usize, usize, usize, usize),
) -> Vec<T> {
    (y0..y1)
        .flat_map(|y| src[(y * w + x0) * per..(y * w + x1) * per].iter().copied())
        .collect()
}

/// A copy of the sprite with the transparent border of every picture cut off (`GrfImage.Trim`);
/// a picture without a visible pixel stays as it is.
pub fn trim_sprite(spr: &Spr) -> Spr {
    let mut out = spr.clone();
    for img in &mut out.indexed {
        let (w, h) = (usize::from(img.width), usize::from(img.height));
        if let Some(b) = box_of(w, h, |x, y| img.pixels[y * w + x] != 0) {
            img.pixels = crop(&img.pixels, w, 1, b);
            (img.width, img.height) = ((b.2 - b.0) as u16, (b.3 - b.1) as u16);
        }
    }
    for img in &mut out.rgba {
        let (w, h) = (usize::from(img.width), usize::from(img.height));
        if let Some(b) = box_of(w, h, |x, y| img.pixels[(y * w + x) * 4 + 3] != 0) {
            img.pixels = crop(&img.pixels, w, 4, b);
            (img.width, img.height) = ((b.2 - b.0) as u16, (b.3 - b.1) as u16);
        }
    }
    out
}

/// How alike the first nine pictures of two sprites are in size (`ActMatch.Match`): 1.0 for equal
/// sizes; 0 when the sprites have a different number of pictures (counted up to nine).
pub fn match_ratio(a: &Sizes, b: &Sizes) -> f64 {
    let n = a.sizes.len().min(9);
    if n != b.sizes.len().min(9) || n == 0 {
        return 0.0;
    }
    let rate = |x: u32, y: u32| {
        let m = x.max(y);
        if m == 0 {
            1.0
        } else {
            1.0 - f64::from(x.abs_diff(y)) / f64::from(m)
        }
    };
    let sum: f64 = a.sizes[..n]
        .iter()
        .zip(&b.sizes[..n])
        .map(|(p, q)| rate(p.0, q.0) + rate(p.1, q.1))
        .sum();
    sum / (2 * n) as f64
}

/// The better of the two ways to compare (as is, or without borders) and whether the trimmed
/// sizes won, which are then the ones the offsets are corrected with.
pub fn best_match(original: &Sizes, trimmed: &Sizes, kro: &Sizes) -> (f64, bool) {
    let (a, b) = (match_ratio(original, kro), match_ratio(trimmed, kro));
    if b > a { (b, true) } else { (a, false) }
}

/// Moves the whole animation (`_adjustAct`): `dx` to the right in the directions that look to the
/// right and to the left in the others (actions 0 to 3 of each group of eight against 4 to 7), `dy`
/// down everywhere.
pub fn shift(act: &mut Act, dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }
    for (i, action) in act.actions.iter_mut().enumerate() {
        let x = if i % 8 < 4 { dx } else { -dx };
        for l in action.frames.iter_mut().flat_map(|f| f.layers.iter_mut()) {
            l.offset_x += x as f32;
            l.offset_y += dy as f32;
        }
    }
}

/// Fits the animation of a kRO garment to the pictures of another sprite (`_fixSprites`): the
/// layers take the size of the new picture, and where the new picture is larger or smaller than
/// kRO's, the offset follows by half the difference (down for the directions 7 and 0, sideways
/// for 2 and 6), so that the picture stays where the artist put kRO's. Then it is moved by the
/// user's offset.
pub fn fit_to_sprite(act: &mut Act, kro: &Sizes, new: &Sizes, dx: i32, dy: i32) {
    for l in act
        .actions
        .iter_mut()
        .flat_map(|a| a.frames.iter_mut())
        .flat_map(|f| f.layers.iter_mut())
    {
        if let Some((w, h)) = new.of_layer(l) {
            l.width = w as i32;
            l.height = h as i32;
        }
    }
    shift(act, dx, dy);
    for (aid, action) in act.actions.iter_mut().enumerate() {
        for l in action.frames.iter_mut().flat_map(|f| f.layers.iter_mut()) {
            let (Some(old), Some(now)) = (kro.of_layer(l), new.of_layer(l)) else {
                continue;
            };
            // C# integer division: truncates toward zero
            let (w, h) = (
                (now.0 as i64 - old.0 as i64) / 2,
                (now.1 as i64 - old.1 as i64) / 2,
            );
            match aid % 8 {
                7 | 0 => l.offset_y += h as f32,
                2 => l.offset_x += w as f32,
                6 => l.offset_x -= w as f32,
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::act::{Action, Frame, Layer};
    use crate::spr::{IndexedImage, PALETTE_LEN, Version};

    fn sizes(v: &[(u32, u32)]) -> Sizes {
        Sizes {
            sizes: v.to_vec(),
            indexed: v.len(),
        }
    }

    fn act8(layer: Layer) -> Act {
        let mut a = Act::template(crate::act_ops::Template::Empty);
        a.actions = (0..8)
            .map(|_| Action {
                frames: vec![Frame {
                    layers: vec![layer.clone()],
                    ..Frame::default()
                }],
                ..Action::default()
            })
            .collect();
        a
    }

    #[test]
    fn the_job_list_has_male_and_female_names_and_skips_comments() {
        let jobs = job_paths(b"// head\r\nabc\t\t// c\r\n\r\nm#f\r\nx\r\n");
        assert_eq!(
            jobs,
            [
                (b"abc".to_vec(), b"abc".to_vec()),
                (b"m".to_vec(), b"f".to_vec())
            ]
        );
        // ACT Editor's own list: about 200 jobs, some with different female names
        let all = job_paths(DEFAULT_JOB_PATHS);
        assert!(all.len() > 150, "{}", all.len());
        assert!(all.iter().any(|(m, f)| m != f));
        assert!(all.iter().any(|(m, _)| m == b"windhawk"));
    }

    #[test]
    fn sprites_are_matched_by_the_sizes_of_their_first_pictures() {
        let a = sizes(&[(10, 20), (30, 40)]);
        assert_eq!(match_ratio(&a, &a), 1.0);
        let b = sizes(&[(5, 20), (30, 20)]);
        // widths 0.5 and 1, heights 1 and 0.5
        assert!((match_ratio(&a, &b) - 0.75).abs() < 1e-9);
        assert_eq!(match_ratio(&a, &sizes(&[(10, 20)])), 0.0);
        // a sprite with a transparent border matches better once the border is cut off
        let mut pal = Box::new([0u8; PALETTE_LEN]);
        pal[4..8].copy_from_slice(&[9, 9, 9, 0]);
        let mut px = vec![0u8; 16];
        px[5] = 1;
        px[6] = 1;
        let spr = Spr {
            version: Version::V2_1,
            indexed: vec![IndexedImage {
                width: 4,
                height: 4,
                pixels: px,
            }],
            rgba: vec![],
            palette: Some(pal),
            warnings: vec![],
        };
        assert_eq!(Sizes::of(&spr).sizes, [(4, 4)]);
        assert_eq!(Sizes::trimmed(&spr).sizes, [(2, 1)]);
        assert_eq!(trim_sprite(&spr).indexed[0].pixels, [1, 1]);
        let (ratio, trimmed) =
            best_match(&Sizes::of(&spr), &Sizes::trimmed(&spr), &sizes(&[(2, 1)]));
        assert!(trimmed && ratio == 1.0);
    }

    #[test]
    fn the_offset_turns_round_for_the_directions_that_look_left() {
        let mut a = act8(Layer::default());
        shift(&mut a, 5, -2);
        let x: Vec<f32> = a
            .actions
            .iter()
            .map(|a| a.frames[0].layers[0].offset_x)
            .collect();
        assert_eq!(x, [5.0, 5.0, 5.0, 5.0, -5.0, -5.0, -5.0, -5.0]);
        assert!(
            a.actions
                .iter()
                .all(|a| a.frames[0].layers[0].offset_y == -2.0)
        );
    }

    #[test]
    fn a_larger_picture_moves_the_offset_by_half_the_difference() {
        let layer = Layer {
            sprite_index: 0,
            ..Layer::default()
        };
        let mut a = act8(layer);
        // kRO's picture is 10 x 20, the new one 16 x 25: the differences 6 and 5 halve to 3 and 2
        fit_to_sprite(&mut a, &sizes(&[(10, 20)]), &sizes(&[(16, 25)]), 0, 0);
        let at = |n: usize| {
            let l = &a.actions[n].frames[0].layers[0];
            (l.offset_x, l.offset_y, l.width, l.height)
        };
        assert_eq!(at(0), (0.0, 2.0, 16, 25));
        assert_eq!(at(7), (0.0, 2.0, 16, 25));
        assert_eq!(at(2), (3.0, 0.0, 16, 25));
        assert_eq!(at(6), (-3.0, 0.0, 16, 25));
        assert_eq!(at(1), (0.0, 0.0, 16, 25));
    }
}
