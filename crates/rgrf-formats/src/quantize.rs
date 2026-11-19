//! Colour reduction to at most 256 colours (for 8-bit BMP textures), with the transparency key kept
//! exact.
//!
//! The client draws magenta (255, 0, 255) as transparent; roBrowser takes every colour with
//! R > 230, G < 20 and B > 230 (JPEG noise). Colours in that range keep their exact value and get
//! their own palette entries, and no other colour is ever moved into the range. So what was
//! transparent stays transparent and nothing else becomes transparent, under either rule.
//!
//! Method: weighted median cut on the distinct colours (the box with the largest squared error is
//! split at the weighted median of its widest channel), then a few rounds of k-means on the same
//! colours. No dithering: textures are scaled and filtered by the client, where dither noise shows.

use std::collections::HashMap;

/// Whether a colour is in the transparency range (see the module comment).
pub fn is_key(c: [u8; 3]) -> bool {
    c[0] > 230 && c[1] < 20 && c[2] > 230
}

/// More distinct transparency colours than this and the image is left alone (it is noisy, like a
/// JPEG saved as BMP, and reserving palette entries for all of them would ruin the rest).
pub const MAX_KEY_COLORS: usize = 32;

const KMEANS_ROUNDS: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct Quantized {
    pub palette: Vec<[u8; 3]>,
    /// One palette index per pixel.
    pub indices: Vec<u8>,
    /// Peak signal-to-noise ratio over all pixels and channels in dB; infinite when nothing
    /// changed.
    pub psnr: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum QuantizeError {
    #[error("the image has no pixels")]
    Empty,
    #[error("the image has {0} transparency colours (more than {MAX_KEY_COLORS})")]
    KeyColors(usize),
    #[error("at most {0} colours leave no room for the image")]
    TooFewColors(usize),
}

/// Reduces `pixels` to at most `max_colors` (2..=256) colours. An image that already has that few
/// keeps its colours exactly.
pub fn quantize(pixels: &[[u8; 3]], max_colors: usize) -> Result<Quantized, QuantizeError> {
    let max_colors = max_colors.min(256);
    if pixels.is_empty() {
        return Err(QuantizeError::Empty);
    }
    let mut hist: HashMap<[u8; 3], u32> = HashMap::new();
    for p in pixels {
        *hist.entry(*p).or_default() += 1;
    }
    // sorted, so the result never depends on hash order
    let mut colors: Vec<([u8; 3], u32)> = hist.into_iter().collect();
    colors.sort_unstable_by_key(|c| c.0);

    if colors.len() <= max_colors {
        let palette: Vec<[u8; 3]> = colors.iter().map(|c| c.0).collect();
        let at: HashMap<[u8; 3], u8> = palette
            .iter()
            .enumerate()
            .map(|(i, c)| (*c, i as u8))
            .collect();
        return Ok(Quantized {
            indices: pixels.iter().map(|p| at[p]).collect(),
            palette,
            psnr: f64::INFINITY,
        });
    }

    let (keys, mut normal): (Vec<_>, Vec<_>) = colors.into_iter().partition(|c| is_key(c.0));
    if keys.len() > MAX_KEY_COLORS {
        return Err(QuantizeError::KeyColors(keys.len()));
    }
    let k = max_colors.saturating_sub(keys.len());
    if k < 1 {
        return Err(QuantizeError::TooFewColors(max_colors));
    }

    let mut centers = median_cut(&mut normal, k);
    refine(&normal, &mut centers);
    let mut palette: Vec<[u8; 3]> = centers
        .iter()
        .map(|c| {
            let mut p = c.map(|v| v.round().clamp(0.0, 255.0) as u8);
            if is_key(p) {
                p[1] = 20; // just outside the range; only a mean of colours outside it lands here
            }
            p
        })
        .collect();
    let normal_count = palette.len();
    palette.extend(keys.iter().map(|c| c.0));

    let mut at: HashMap<[u8; 3], u8> = HashMap::with_capacity(normal.len() + keys.len());
    let mut err = 0f64;
    for (c, n) in &normal {
        let i = nearest(&palette[..normal_count], *c);
        err += f64::from(dist(*c, palette[i])) * f64::from(*n);
        at.insert(*c, i as u8);
    }
    for (i, (c, _)) in keys.iter().enumerate() {
        at.insert(*c, (normal_count + i) as u8);
    }
    let mse = err / (pixels.len() as f64 * 3.0);
    Ok(Quantized {
        indices: pixels.iter().map(|p| at[p]).collect(),
        palette,
        psnr: psnr(mse),
    })
}

/// PSNR in dB for a mean squared error per channel (8-bit samples).
pub fn psnr(mse: f64) -> f64 {
    if mse <= 0.0 {
        f64::INFINITY
    } else {
        10.0 * (255.0f64 * 255.0 / mse).log10()
    }
}

fn dist(a: [u8; 3], b: [u8; 3]) -> u32 {
    (0..3)
        .map(|i| {
            let d = i32::from(a[i]) - i32::from(b[i]);
            (d * d) as u32
        })
        .sum()
}

fn nearest(palette: &[[u8; 3]], c: [u8; 3]) -> usize {
    let mut best = (u32::MAX, 0);
    for (i, p) in palette.iter().enumerate() {
        let d = dist(*p, c);
        if d < best.0 {
            best = (d, i);
            if d == 0 {
                break;
            }
        }
    }
    best.1
}

/// Count, sums and sums of squares of a run of weighted colours.
struct Stats {
    n: f64,
    sum: [f64; 3],
    sq: [f64; 3],
}

impl Stats {
    fn of(colors: &[([u8; 3], u32)]) -> Self {
        let mut s = Stats {
            n: 0.0,
            sum: [0.0; 3],
            sq: [0.0; 3],
        };
        for (c, w) in colors {
            let w = f64::from(*w);
            s.n += w;
            for i in 0..3 {
                let v = f64::from(c[i]);
                s.sum[i] += v * w;
                s.sq[i] += v * v * w;
            }
        }
        s
    }
    fn var(&self, i: usize) -> f64 {
        self.sq[i] - self.sum[i] * self.sum[i] / self.n
    }
    fn sse(&self) -> f64 {
        (0..3).map(|i| self.var(i)).sum()
    }
    fn mean(&self) -> [f64; 3] {
        self.sum.map(|s| s / self.n)
    }
}

/// Splits the colours into at most `k` boxes and returns their weighted means. Reorders `colors`.
fn median_cut(colors: &mut [([u8; 3], u32)], k: usize) -> Vec<[f64; 3]> {
    // boxes as ranges of `colors`, with their squared error
    let mut boxes: Vec<(usize, usize, f64)> = vec![(0, colors.len(), Stats::of(colors).sse())];
    while boxes.len() < k {
        let Some((at, _)) = boxes
            .iter()
            .enumerate()
            .filter(|(_, b)| b.1 - b.0 > 1 && b.2 > 0.0)
            .max_by(|a, b| a.1.2.total_cmp(&b.1.2))
        else {
            break;
        };
        let (lo, hi, _) = boxes[at];
        let part = &mut colors[lo..hi];
        let stats = Stats::of(part);
        let axis = (0..3)
            .max_by(|&a, &b| stats.var(a).total_cmp(&stats.var(b)))
            .unwrap_or(0);
        part.sort_unstable_by_key(|c| (c.0[axis], c.0));
        let half = stats.n / 2.0;
        let mut acc = 0.0;
        let mut cut = 1;
        for (i, (_, w)) in part.iter().enumerate() {
            acc += f64::from(*w);
            if acc >= half {
                cut = i + 1;
                break;
            }
        }
        let cut = cut.clamp(1, part.len() - 1);
        let (a, b) = part.split_at(cut);
        let (sa, sb) = (Stats::of(a).sse(), Stats::of(b).sse());
        boxes[at] = (lo, lo + cut, sa);
        boxes.push((lo + cut, hi, sb));
    }
    boxes
        .iter()
        .map(|&(lo, hi, _)| Stats::of(&colors[lo..hi]).mean())
        .collect()
}

/// A few rounds of k-means (Lloyd) on the weighted colours; a centre that loses all its colours
/// stays where it is.
fn refine(colors: &[([u8; 3], u32)], centers: &mut [[f64; 3]]) {
    let mut assign = vec![usize::MAX; colors.len()];
    for _ in 0..KMEANS_ROUNDS {
        let pal: Vec<[f32; 3]> = centers.iter().map(|c| c.map(|v| v as f32)).collect();
        let mut changed = false;
        for (j, (c, _)) in colors.iter().enumerate() {
            let c = c.map(f32::from);
            let mut best = (f32::MAX, 0);
            for (i, p) in pal.iter().enumerate() {
                let d = (p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2) + (p[2] - c[2]).powi(2);
                if d < best.0 {
                    best = (d, i);
                }
            }
            if assign[j] != best.1 {
                assign[j] = best.1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
        let mut acc = vec![(0f64, [0f64; 3]); centers.len()];
        for (j, (c, w)) in colors.iter().enumerate() {
            let (n, s) = &mut acc[assign[j]];
            let w = f64::from(*w);
            *n += w;
            for i in 0..3 {
                s[i] += f64::from(c[i]) * w;
            }
        }
        for (center, (n, s)) in centers.iter_mut().zip(acc) {
            if n > 0.0 {
                *center = s.map(|v| v / n);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn few_colours_stay_exact() {
        let px = [[1, 2, 3], [255, 0, 255], [1, 2, 3], [9, 9, 9]];
        let q = quantize(&px, 256).unwrap();
        assert_eq!(q.psnr, f64::INFINITY);
        assert_eq!(q.palette.len(), 3);
        let back: Vec<_> = q.indices.iter().map(|&i| q.palette[i as usize]).collect();
        assert_eq!(back, px);
    }

    #[test]
    fn a_gradient_is_reduced_and_the_key_survives_exactly() {
        // 64 x 64 colours of a red-green ramp, plus magenta and a near-magenta pixel
        let mut px: Vec<[u8; 3]> = (0..4096u32)
            .map(|i| [(i % 64 * 4) as u8, (i / 64 * 4) as u8, 128])
            .collect();
        px.push([255, 0, 255]);
        px.push([240, 10, 250]);
        let q = quantize(&px, 64).unwrap();
        assert!(q.palette.len() <= 64);
        assert!(q.psnr > 30.0 && q.psnr.is_finite(), "psnr {}", q.psnr);
        let back: Vec<_> = q.indices.iter().map(|&i| q.palette[i as usize]).collect();
        assert_eq!(back[4096], [255, 0, 255]);
        assert_eq!(back[4097], [240, 10, 250]);
        assert!(back[..4096].iter().all(|c| !is_key(*c)));
    }

    #[test]
    fn colours_next_to_the_key_are_never_made_transparent() {
        // pinks just outside the range: too much green, or too little red; their mean is inside
        let mut px = vec![[255, 0, 255]; 50];
        for i in 0..600u32 {
            let v = (i % 10) as u8;
            px.push(if i % 2 == 0 {
                [240 + v, 20 + v, 245]
            } else {
                [221 + v, v, 245]
            });
        }
        // magenta plus one colour for all pinks: their mean (about 235, 14, 245) is in the range
        let q = quantize(&px, 2).unwrap();
        assert_eq!(q.palette[0], [235, 20, 245]);
        // magenta stays exact; none of the pinks ends up in the range
        for (p, &i) in px.iter().zip(&q.indices) {
            let out = q.palette[i as usize];
            assert_eq!(is_key(*p), is_key(out), "{p:?} -> {out:?}");
        }
    }

    #[test]
    fn noisy_transparency_is_refused() {
        let px: Vec<[u8; 3]> = (0..300u32)
            .map(|i| [235 + (i % 20) as u8, (i / 20) as u8, 255])
            .chain((0..300u32).map(|i| [0, (i % 256) as u8, (i / 256) as u8]))
            .collect();
        assert_eq!(quantize(&px, 256), Err(QuantizeError::KeyColors(300)));
    }

    #[test]
    fn the_result_does_not_depend_on_the_run() {
        let px: Vec<[u8; 3]> = (0..20000u32)
            .map(|i| {
                [
                    (i * 7 % 251) as u8,
                    (i * 13 % 241) as u8,
                    (i * 3 % 239) as u8,
                ]
            })
            .collect();
        assert_eq!(quantize(&px, 256).unwrap(), quantize(&px, 256).unwrap());
    }
}
