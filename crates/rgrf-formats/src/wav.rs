//! WAV reading for the preview: PCM is played as it is, the two ADPCM variants some RO sounds use
//! (IMA, tag 0x11, and Microsoft, tag 0x02) are decoded to 16-bit PCM so every webview can play them.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum WavError {
    #[error("not a RIFF/WAVE file")]
    NotWav,
    #[error("the WAV has no usable fmt or data chunk")]
    Chunks,
    #[error("unsupported WAV format tag {0:#x}")]
    Format(u16),
    #[error("the ADPCM data is inconsistent")]
    Adpcm,
}

/// What a webview can play: the original bytes, or PCM made from ADPCM.
#[derive(Debug, PartialEq, Eq)]
pub enum Playable {
    AsIs,
    Decoded(Vec<u8>),
}

struct Fmt {
    tag: u16,
    channels: usize,
    rate: u32,
    block_align: usize,
    bits: usize,
    extra: Vec<u8>,
    /// Samples per channel from the `fact` chunk, when there is one.
    samples: Option<usize>,
}

fn le16(b: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([b[i], b[i + 1]])
}

fn chunks(d: &[u8]) -> Result<(Fmt, &[u8]), WavError> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let (mut fmt, mut data, mut samples) = (None, None, None);
    let mut i = 12;
    while i + 8 <= d.len() {
        let id = &d[i..i + 4];
        let size = u32::from_le_bytes([d[i + 4], d[i + 5], d[i + 6], d[i + 7]]) as usize;
        let body = &d[i + 8..(i + 8 + size).min(d.len())];
        if id == b"fmt " && body.len() >= 16 {
            let cb = if body.len() >= 18 {
                usize::from(le16(body, 16))
            } else {
                0
            };
            fmt = Some(Fmt {
                tag: le16(body, 0),
                channels: usize::from(le16(body, 2)),
                rate: u32::from_le_bytes([body[4], body[5], body[6], body[7]]),
                block_align: usize::from(le16(body, 12)),
                bits: usize::from(le16(body, 14)),
                extra: body
                    .get(18..(18 + cb).min(body.len()))
                    .unwrap_or(&[])
                    .to_vec(),
                samples: None,
            });
        } else if id == b"fact" && body.len() >= 4 {
            samples = Some(u32::from_le_bytes([body[0], body[1], body[2], body[3]]) as usize);
        } else if id == b"data" {
            data = Some(body);
        }
        i += 8 + size + (size & 1);
    }
    match (fmt, data) {
        (Some(mut f), Some(b)) if f.channels > 0 => {
            f.samples = samples;
            Ok((f, b))
        }
        _ => Err(WavError::Chunks),
    }
}

const IMA_STEPS: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66,
    73, 80, 88, 97, 107, 118, 130, 143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449,
    494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282, 1411, 1552, 1707, 1878, 2066, 2272,
    2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630, 9493,
    10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const IMA_INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

fn ima(f: &Fmt, data: &[u8]) -> Result<Vec<Vec<i16>>, WavError> {
    let ch = f.channels;
    if f.block_align < 4 * ch {
        return Err(WavError::Adpcm);
    }
    let mut out = vec![Vec::new(); ch];
    for block in data.chunks(f.block_align) {
        if block.len() < 4 * ch {
            break;
        }
        let mut pred = vec![0i32; ch];
        let mut idx = vec![0i32; ch];
        for c in 0..ch {
            pred[c] = i32::from(le16(block, 4 * c) as i16);
            idx[c] = i32::from(block[4 * c + 2]).min(88);
            out[c].push(pred[c] as i16);
        }
        // after the headers the nibbles come in groups of 4 bytes (8 samples) per channel
        let mut at = 4 * ch;
        while at + 4 * ch <= block.len() {
            for c in 0..ch {
                for &byte in &block[at + 4 * c..at + 4 * c + 4] {
                    for nib in [byte & 15, byte >> 4] {
                        let step = IMA_STEPS[idx[c] as usize];
                        let mut diff = step >> 3;
                        if nib & 1 != 0 {
                            diff += step >> 2;
                        }
                        if nib & 2 != 0 {
                            diff += step >> 1;
                        }
                        if nib & 4 != 0 {
                            diff += step;
                        }
                        pred[c] = (pred[c] + if nib & 8 != 0 { -diff } else { diff })
                            .clamp(-32768, 32767);
                        idx[c] = (idx[c] + IMA_INDEX[usize::from(nib)]).clamp(0, 88);
                        out[c].push(pred[c] as i16);
                    }
                }
            }
            at += 4 * ch;
        }
    }
    Ok(out)
}

const MS_ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

fn ms(f: &Fmt, data: &[u8]) -> Result<Vec<Vec<i16>>, WavError> {
    let ch = f.channels;
    // extra: samples per block, number of coefficient pairs, then the pairs
    if f.extra.len() < 4 || ch > 2 {
        return Err(WavError::Adpcm);
    }
    let n_coef = usize::from(le16(&f.extra, 2));
    if f.extra.len() < 4 + 4 * n_coef || n_coef == 0 {
        return Err(WavError::Adpcm);
    }
    let coef: Vec<(i32, i32)> = (0..n_coef)
        .map(|i| {
            (
                i32::from(le16(&f.extra, 4 + 4 * i) as i16),
                i32::from(le16(&f.extra, 6 + 4 * i) as i16),
            )
        })
        .collect();
    let header = 7 * ch;
    if f.block_align < header {
        return Err(WavError::Adpcm);
    }
    let mut out = vec![Vec::new(); ch];
    for block in data.chunks(f.block_align) {
        if block.len() < header {
            break;
        }
        let mut pred = [0usize; 2];
        let mut delta = [0i32; 2];
        let mut s1 = [0i32; 2];
        let mut s2 = [0i32; 2];
        for c in 0..ch {
            pred[c] = usize::from(block[c]).min(n_coef - 1);
        }
        let mut at = ch;
        for d in delta.iter_mut().take(ch) {
            *d = i32::from(le16(block, at) as i16);
            at += 2;
        }
        for s in s1.iter_mut().take(ch) {
            *s = i32::from(le16(block, at) as i16);
            at += 2;
        }
        for s in s2.iter_mut().take(ch) {
            *s = i32::from(le16(block, at) as i16);
            at += 2;
        }
        for c in 0..ch {
            out[c].push(s2[c] as i16);
        }
        for c in 0..ch {
            out[c].push(s1[c] as i16);
        }
        let mut nibble = 0usize;
        for &byte in &block[header..] {
            for nib in [byte >> 4, byte & 15] {
                let c = nibble % ch;
                nibble += 1;
                let (c1, c2) = coef[pred[c]];
                let signed = i32::from(nib) - if nib & 8 != 0 { 16 } else { 0 };
                let p = ((s1[c] * c1 + s2[c] * c2) >> 8) + signed * delta[c];
                let p = p.clamp(-32768, 32767);
                out[c].push(p as i16);
                s2[c] = s1[c];
                s1[c] = p;
                delta[c] = ((MS_ADAPT[usize::from(nib)] * delta[c]) >> 8).max(16);
            }
        }
    }
    Ok(out)
}

fn pcm16_wav(channels: &[Vec<i16>], rate: u32) -> Vec<u8> {
    let ch = channels.len() as u16;
    let n = channels.iter().map(Vec::len).min().unwrap_or(0);
    let data_len = (n * usize::from(ch) * 2) as u32;
    let mut w = Vec::with_capacity(44 + data_len as usize);
    w.extend(b"RIFF");
    w.extend((36 + data_len).to_le_bytes());
    w.extend(b"WAVEfmt ");
    w.extend(16u32.to_le_bytes());
    w.extend(1u16.to_le_bytes());
    w.extend(ch.to_le_bytes());
    w.extend(rate.to_le_bytes());
    w.extend((rate * u32::from(ch) * 2).to_le_bytes());
    w.extend((ch * 2).to_le_bytes());
    w.extend(16u16.to_le_bytes());
    w.extend(b"data");
    w.extend(data_len.to_le_bytes());
    for i in 0..n {
        for c in channels {
            w.extend(c[i].to_le_bytes());
        }
    }
    w
}

/// A sound as planar samples in -1..1: PCM of 8, 16, 24 or 32 bits, 32 or 64-bit float, and the
/// two ADPCM kinds (see [`playable`]). Returns the sample rate and one vector per channel.
pub fn decode(bytes: &[u8]) -> Result<(u32, Vec<Vec<f32>>), WavError> {
    let (f, data) = chunks(bytes)?;
    // WAVE_FORMAT_EXTENSIBLE keeps the real tag in the first two bytes of its sub-format
    let tag = if f.tag == 0xfffe && f.extra.len() >= 8 {
        le16(&f.extra, 6)
    } else {
        f.tag
    };
    let ch = f.channels;
    let int = |bits: usize, i: usize, d: &[u8]| -> f32 {
        match bits {
            8 => (f32::from(d[i]) - 128.0) / 128.0,
            16 => f32::from(i16::from_le_bytes([d[i], d[i + 1]])) / 32768.0,
            24 => (i32::from_le_bytes([0, d[i], d[i + 1], d[i + 2]]) >> 8) as f32 / 8_388_608.0,
            _ => i32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]) as f32 / 2_147_483_648.0,
        }
    };
    let planar = |frame_bytes: usize, read: &dyn Fn(usize) -> f32| -> Vec<Vec<f32>> {
        let n = data.len() / (frame_bytes * ch);
        let mut out = vec![Vec::with_capacity(n); ch];
        for k in 0..n {
            for (c, o) in out.iter_mut().enumerate() {
                o.push(read((k * ch + c) * frame_bytes));
            }
        }
        out
    };
    let channels = match (tag, f.bits) {
        (1, b @ (8 | 16 | 24 | 32)) => planar(b / 8, &|i| int(b, i, data)),
        (3, 32) => planar(4, &|i| {
            f32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
        }),
        (3, 64) => planar(8, &|i| {
            let mut b = [0u8; 8];
            b.copy_from_slice(&data[i..i + 8]);
            f64::from_le_bytes(b) as f32
        }),
        (0x11 | 0x02, _) => {
            let pcm = if tag == 0x11 {
                ima(&f, data)?
            } else {
                ms(&f, data)?
            };
            let n = f.samples.unwrap_or(usize::MAX);
            pcm.into_iter()
                .map(|c| {
                    c.into_iter()
                        .take(n)
                        .map(|s| f32::from(s) / 32768.0)
                        .collect()
                })
                .collect()
        }
        (t, _) => return Err(WavError::Format(t)),
    };
    Ok((f.rate, channels))
}

/// 16-bit PCM WAV of planar samples in -1..1 (clipped), the format every RO client plays.
pub fn encode_pcm16(rate: u32, channels: &[Vec<f32>]) -> Vec<u8> {
    let pcm: Vec<Vec<i16>> = channels
        .iter()
        .map(|c| {
            c.iter()
                .map(|&x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
                .collect()
        })
        .collect();
    pcm16_wav(&pcm, rate)
}

/// PCM stays as it is; IMA and Microsoft ADPCM become 16-bit PCM; everything else is an error.
pub fn playable(bytes: &[u8]) -> Result<Playable, WavError> {
    let (f, data) = chunks(bytes)?;
    let channels = match f.tag {
        1 | 3 => return Ok(Playable::AsIs),
        0x11 => ima(&f, data)?,
        0x02 => ms(&f, data)?,
        t => return Err(WavError::Format(t)),
    };
    let mut channels = channels;
    if let Some(n) = f.samples {
        channels.iter_mut().for_each(|c| c.truncate(n));
    }
    Ok(Playable::Decoded(pcm16_wav(&channels, f.rate)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(tag: u16, ch: u16, align: u16, extra: &[u8], data: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend(tag.to_le_bytes());
        fmt.extend(ch.to_le_bytes());
        fmt.extend(8000u32.to_le_bytes());
        fmt.extend(0u32.to_le_bytes());
        fmt.extend(align.to_le_bytes());
        fmt.extend(4u16.to_le_bytes());
        fmt.extend((extra.len() as u16).to_le_bytes());
        fmt.extend(extra);
        let mut w = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        w.extend((fmt.len() as u32).to_le_bytes());
        w.extend(fmt);
        w.extend(b"data");
        w.extend((data.len() as u32).to_le_bytes());
        w.extend(data);
        w
    }

    #[test]
    fn pcm_is_left_alone() {
        assert_eq!(playable(&wav(1, 1, 2, &[], &[0, 0])), Ok(Playable::AsIs));
    }

    #[test]
    fn ima_block_decodes_header_sample_and_steps() {
        // predictor 100, index 0, then 8 nibbles of 0 (diff = step >> 3 = 0 -> stays 100)
        let mut block = vec![100, 0, 0, 0];
        block.extend([0u8; 4]);
        let Playable::Decoded(w) = playable(&wav(0x11, 1, 8, &[2, 0, 5, 0], &block)).unwrap()
        else {
            panic!()
        };
        let samples: Vec<i16> = w[44..]
            .chunks(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(samples.len(), 9);
        assert!(samples.iter().all(|&s| s == 100), "{samples:?}");
    }

    #[test]
    fn broken_adpcm_is_an_error() {
        assert_eq!(playable(b"nope"), Err(WavError::NotWav));
        assert_eq!(playable(&wav(0x11, 1, 2, &[], &[])), Err(WavError::Adpcm));
        assert_eq!(
            playable(&wav(0x02, 1, 40, &[], &[0; 40])),
            Err(WavError::Adpcm)
        );
        assert_eq!(
            playable(&wav(0x55, 1, 1, &[], &[0])),
            Err(WavError::Format(0x55))
        );
    }
}
