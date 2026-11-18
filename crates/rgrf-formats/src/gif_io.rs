//! Animated GIF from rendered frames.
//!
//! GIF has 256 colours per frame and no partial transparency: each frame is quantised (NeuQuant)
//! and pixels with alpha 0 become transparent. Edges blended by the renderer are therefore a little
//! rougher than the PNG output.

use crate::render::Canvas;

#[derive(Debug, thiserror::Error)]
pub enum GifError {
    #[error("no frames to encode")]
    NoFrames,
    #[error("frames differ in size; render them with the same bounds")]
    SizeMismatch,
    #[error("a GIF is at most 65535 x 65535 pixels, this one is {0} x {1}")]
    TooLarge(u32, u32),
    #[error("GIF encoding failed: {0}")]
    Encode(#[from] gif::EncodingError),
}

/// Frame time in GIF units (1/100 s) for an ACT interval (units of 25 ms), at least 2 because most
/// viewers treat smaller values as 10.
pub fn delay_cs(interval: f32) -> u16 {
    ((interval * 2.5).round().max(2.0)).min(f32::from(u16::MAX)) as u16
}

/// Encodes `frames` (same size) with `delays` in 1/100 s, looping forever. With `scale` above 1
/// every pixel becomes `scale x scale` pixels (nearest neighbour).
pub fn encode_gif(frames: &[Canvas], delays: &[u16], scale: u32) -> Result<Vec<u8>, GifError> {
    let first = frames.first().ok_or(GifError::NoFrames)?;
    let scale = scale.max(1);
    let (w, h) = (first.width * scale, first.height * scale);
    let (Ok(w16), Ok(h16)) = (u16::try_from(w), u16::try_from(h)) else {
        return Err(GifError::TooLarge(w, h));
    };
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, w16, h16, &[])?;
        enc.set_repeat(gif::Repeat::Infinite)?;
        for (i, c) in frames.iter().enumerate() {
            if (c.width, c.height) != (first.width, first.height) {
                return Err(GifError::SizeMismatch);
            }
            let mut px = if scale == 1 {
                c.pixels.clone()
            } else {
                upscale(
                    &c.pixels,
                    c.width as usize,
                    c.height as usize,
                    scale as usize,
                )
            };
            let mut f = gif::Frame::from_rgba_speed(w16, h16, &mut px, 10);
            f.delay = delays.get(i).copied().unwrap_or(4);
            f.dispose = gif::DisposalMethod::Background;
            enc.write_frame(&f)?;
        }
    }
    Ok(out)
}

pub fn upscale(px: &[u8], w: usize, h: usize, s: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * s * h * s * 4];
    for y in 0..h * s {
        for x in 0..w * s {
            let (sx, sy) = (x / s, y / s);
            let d = 4 * (y * w * s + x);
            out[d..d + 4].copy_from_slice(&px[4 * (sy * w + sx)..4 * (sy * w + sx) + 4]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::Bounds;

    fn canvas(w: u32, h: u32, rgba: [u8; 4]) -> Canvas {
        Canvas {
            width: w,
            height: h,
            bounds: Bounds {
                min_x: 0,
                min_y: 0,
                max_x: w as i32,
                max_y: h as i32,
            },
            pixels: rgba.repeat((w * h) as usize),
        }
    }

    #[test]
    fn frames_round_trip_through_the_gif_decoder() {
        let frames = [canvas(3, 2, [255, 0, 0, 255]), canvas(3, 2, [0, 0, 255, 0])];
        let bytes = encode_gif(&frames, &[10, 20], 2).unwrap();
        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts.read_info(bytes.as_slice()).unwrap();
        assert_eq!((dec.width(), dec.height()), (6, 4));
        let f1 = dec.read_next_frame().unwrap().unwrap().clone();
        assert_eq!(f1.delay, 10);
        assert_eq!(&f1.buffer[..4], &[255, 0, 0, 255]);
        let f2 = dec.read_next_frame().unwrap().unwrap().clone();
        assert_eq!(f2.delay, 20);
        assert_eq!(f2.buffer[3], 0, "alpha 0 stays transparent");
        assert!(dec.read_next_frame().unwrap().is_none());
    }

    #[test]
    fn errors_and_delays() {
        assert!(matches!(encode_gif(&[], &[], 1), Err(GifError::NoFrames)));
        let a = canvas(2, 2, [1, 2, 3, 255]);
        let b = canvas(3, 2, [1, 2, 3, 255]);
        assert!(matches!(
            encode_gif(&[a, b], &[4, 4], 1),
            Err(GifError::SizeMismatch)
        ));
        assert_eq!(delay_cs(4.0), 10);
        assert_eq!(delay_cs(6.0), 15);
        assert_eq!(delay_cs(0.1), 2);
    }
}
