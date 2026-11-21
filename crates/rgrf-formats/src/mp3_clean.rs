//! A lossless clean-up of MP3 files: what is not sound goes (ID3v1, ID3v2 and APE tags, bytes
//! before the first and after the last frame, junk between frames), the frames themselves stay
//! byte for byte as they are, so the sound is the same. Only MPEG Layer III is handled.
//!
//! The client plays the file from the first frame on; tags that a converter left behind (for
//! example the `major_brand` of an MP4 conversion) and cut-off frames at the end are what a
//! decoder warns about.

const BITRATES_V1: [u32; 15] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];
const BITRATES_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
const RATES_V1: [u32; 3] = [44100, 48000, 32000];

/// The header of one Layer III frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Header {
    /// 3 = MPEG 1, 2 = MPEG 2, 0 = MPEG 2.5
    version: u8,
    rate: u32,
    len: usize,
}

fn header(b: &[u8]) -> Option<Header> {
    if b.len() < 4 || b[0] != 0xff || b[1] & 0xe0 != 0xe0 {
        return None;
    }
    let version = (b[1] >> 3) & 3;
    let layer = (b[1] >> 1) & 3;
    if version == 1 || layer != 1 {
        return None;
    }
    let (br, sr, pad) = (
        (b[2] >> 4) as usize,
        ((b[2] >> 2) & 3) as usize,
        u32::from((b[2] >> 1) & 1),
    );
    if br == 0 || br == 15 || sr == 3 {
        return None;
    }
    let rate = match version {
        3 => RATES_V1[sr],
        2 => RATES_V1[sr] / 2,
        _ => RATES_V1[sr] / 4,
    };
    let len = if version == 3 {
        144 * BITRATES_V1[br] * 1000 / rate + pad
    } else {
        72 * BITRATES_V2[br] * 1000 / rate + pad
    };
    Some(Header {
        version,
        rate,
        len: len as usize,
    })
}

/// Whether `n` frames of the same kind follow each other from `at` (the end of the data also ends
/// the chain).
fn chain(data: &[u8], at: usize, n: usize) -> Option<Header> {
    let first = header(data.get(at..)?)?;
    let mut pos = at;
    for _ in 0..n {
        let h = header(data.get(pos..)?)?;
        if (h.version, h.rate) != (first.version, first.rate) {
            return None;
        }
        pos += h.len;
        if pos == data.len() {
            return Some(first);
        }
        if pos > data.len() {
            return None;
        }
    }
    Some(first)
}

/// What was taken out.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Removed {
    pub id3v2_bytes: usize,
    pub id3v1_bytes: usize,
    pub ape_bytes: usize,
    /// Bytes before the first frame, after the last, and between frames that are not frames.
    pub junk_bytes: usize,
}

impl Removed {
    pub fn total(&self) -> usize {
        self.id3v2_bytes + self.id3v1_bytes + self.ape_bytes + self.junk_bytes
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Cleaned {
    pub data: Vec<u8>,
    pub frames: usize,
    pub removed: Removed,
}

/// Cleans the file; `None` when it holds no Layer III frames.
pub fn clean(data: &[u8]) -> Option<Cleaned> {
    let mut removed = Removed::default();
    let mut start = 0;
    while data.len() >= start + 10 && &data[start..start + 3] == b"ID3" {
        let size = data[start + 6..start + 10]
            .iter()
            .fold(0usize, |n, &x| n << 7 | usize::from(x & 0x7f));
        let footer = if data[start + 5] & 0x10 != 0 { 10 } else { 0 };
        let end = start + 10 + size + footer;
        if end > data.len() {
            break;
        }
        start = end;
    }
    removed.id3v2_bytes = start;
    let mut end = data.len();
    loop {
        if end >= start + 128 && &data[end - 128..end - 125] == b"TAG" {
            end -= 128;
            removed.id3v1_bytes += 128;
        } else if end >= start + 32 && &data[end - 32..end - 24] == b"APETAGEX" {
            let size = u32::from_le_bytes(data[end - 20..end - 16].try_into().ok()?) as usize;
            let has_header =
                u32::from_le_bytes(data[end - 12..end - 8].try_into().ok()?) & (1 << 31) != 0;
            let total = size + if has_header { 32 } else { 0 };
            if total == 0 || total > end - start {
                break;
            }
            end -= total;
            removed.ape_bytes += total;
        } else {
            break;
        }
    }
    let body = &data[..end];
    // the first place where three frames follow each other, then frame after frame
    let mut pos = start;
    let first = loop {
        if pos + 4 > body.len() {
            return None;
        }
        if chain(body, pos, 3).is_some() {
            break pos;
        }
        pos += 1;
    };
    removed.junk_bytes += first - start;
    let kind = header(&body[first..])?;
    let mut out = Vec::with_capacity(body.len() - first);
    let mut frames = 0;
    let mut pos = first;
    while pos < body.len() {
        match header(&body[pos..]) {
            Some(h)
                if (h.version, h.rate) == (kind.version, kind.rate)
                    && pos + h.len <= body.len() =>
            {
                out.extend_from_slice(&body[pos..pos + h.len]);
                pos += h.len;
                frames += 1;
            }
            _ => {
                // junk: look for the next place where frames of this kind follow each other
                let mut next = pos + 1;
                while next + 4 <= body.len()
                    && chain(body, next, 3).map(|h| (h.version, h.rate))
                        != Some((kind.version, kind.rate))
                {
                    next += 1;
                }
                if next + 4 > body.len() {
                    removed.junk_bytes += body.len() - pos;
                    break;
                }
                removed.junk_bytes += next - pos;
                pos = next;
            }
        }
    }
    (frames > 0).then_some(Cleaned {
        data: out,
        frames,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A frame of MPEG 1 at 128 kbit/s and 44.1 kHz (417 bytes, 418 with padding) with a pattern.
    fn frame(seed: u8) -> Vec<u8> {
        let mut f = vec![0xff, 0xfb, 0x90, 0x00];
        f.extend((0..413).map(|i| (i as u8).wrapping_mul(seed)));
        f
    }

    fn id3v2(size: usize) -> Vec<u8> {
        let mut t = b"ID3\x04\x00\x00".to_vec();
        t.extend([0, 0, (size >> 7) as u8 & 0x7f, size as u8 & 0x7f]);
        t.extend(vec![b'x'; size]);
        t
    }

    #[test]
    fn tags_and_cut_off_frames_go_and_the_frames_stay_as_they_are() {
        let frames: Vec<Vec<u8>> = (1..=8).map(frame).collect();
        let sound: Vec<u8> = frames.concat();
        let mut file = id3v2(300);
        file.extend(&sound);
        file.extend(&frames[0][..200]); // a frame that was cut off
        file.extend(b"TAG");
        file.extend(vec![0u8; 125]);
        let c = clean(&file).unwrap();
        assert_eq!(c.data, sound);
        assert_eq!(c.frames, 8);
        assert_eq!(c.removed.id3v2_bytes, 310);
        assert_eq!(c.removed.id3v1_bytes, 128);
        assert_eq!(c.removed.junk_bytes, 200);
        assert_eq!(c.removed.total(), 638);
        // a clean file is returned as it is
        assert_eq!(clean(&sound).unwrap().removed.total(), 0);
        assert_eq!(clean(&sound).unwrap().data, sound);
    }

    #[test]
    fn junk_before_and_between_frames_is_dropped() {
        let frames: Vec<Vec<u8>> = (1..=10).map(frame).collect();
        let mut file = vec![0x12; 37];
        file.extend(frames[..5].concat());
        file.extend(vec![0x55; 90]);
        file.extend(frames[5..].concat());
        let c = clean(&file).unwrap();
        assert_eq!(c.data, frames.concat());
        assert_eq!(c.removed.junk_bytes, 127);
    }

    #[test]
    fn what_is_not_mp3_gives_nothing() {
        assert!(clean(b"RIFF....WAVEfmt ").is_none());
        assert!(clean(&[]).is_none());
        // a frame that does not end where the data ends, with nothing after it, is no proof
        let mut one = frame(1);
        one.extend([1, 2, 3]);
        assert!(clean(&one).is_none());
    }
}
