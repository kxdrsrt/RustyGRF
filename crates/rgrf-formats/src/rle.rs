//! Zero-run RLE of SPR indexed images (version 2.1 and later).
//!
//! Only the colour index 0 (transparent) is run-length coded: a `0` byte is followed by the number
//! of zeros (1..=255); every other byte is a literal index. Same scheme as GRF Editor
//! (`GRF/FileFormats/Rle.cs`).

/// Result of [`decode`]; damaged data is still decoded as far as possible, like the client does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub pixels: Vec<u8>,
    /// The data ended before `width * height` pixels were produced (the rest stays 0).
    pub short: bool,
    /// The data wanted more pixels than `width * height` (the surplus is dropped).
    pub overflow: bool,
}

/// Expands `data` to exactly `out_len` pixels.
pub fn decode(data: &[u8], out_len: usize) -> Decoded {
    let mut pixels = vec![0u8; out_len];
    let mut pos = 0usize;
    let mut overflow = false;
    let mut k = 0usize;
    while k < data.len() {
        let b = data[k];
        k += 1;
        if b == 0 {
            // A trailing 0 without a count is ignored.
            let Some(&run) = data.get(k) else { break };
            k += 1;
            pos += usize::from(run);
        } else {
            if let Some(slot) = pixels.get_mut(pos) {
                *slot = b;
            } else {
                overflow = true;
            }
            pos += 1;
        }
    }
    if pos > out_len {
        overflow = true;
    }
    Decoded {
        pixels,
        short: pos < out_len,
        overflow,
    }
}

/// Compresses indexed pixels. Zero runs longer than 255 are split.
pub fn encode(pixels: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pixels.len() / 2 + 16);
    let mut i = 0;
    while i < pixels.len() {
        if pixels[i] == 0 {
            let mut run = 0u8;
            while i < pixels.len() && pixels[i] == 0 && run < 255 {
                run += 1;
                i += 1;
            }
            out.push(0);
            out.push(run);
        } else {
            out.push(pixels[i]);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_runs_and_literals() {
        let mut px = vec![0u8; 10];
        px.extend([5, 6, 0, 7]);
        px.extend(vec![0u8; 300]);
        px.push(9);
        let enc = encode(&px);
        let dec = decode(&enc, px.len());
        assert_eq!(dec.pixels, px);
        assert!(!dec.short && !dec.overflow);
    }

    #[test]
    fn long_zero_run_is_split_at_255() {
        assert_eq!(encode(&[0u8; 256]), vec![0, 255, 0, 1]);
        assert_eq!(encode(&[0u8; 255]), vec![0, 255]);
    }

    #[test]
    fn damaged_data_does_not_panic() {
        // too short, trailing zero without count, run past the end
        assert!(decode(&[1, 2], 5).short);
        assert_eq!(decode(&[1, 0], 1).pixels, vec![1]);
        let d = decode(&[0, 200, 7], 10);
        assert!(d.overflow);
        assert_eq!(d.pixels.len(), 10);
        assert!(decode(&[], 0).pixels.is_empty());
    }
}
