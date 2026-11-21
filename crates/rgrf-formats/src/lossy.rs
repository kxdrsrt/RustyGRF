//! MP3 and Ogg Vorbis sounds for the editor: decoded to planar samples with Symphonia (pure Rust)
//! and encoded again with LAME (MP3) or libvorbis with the aoTuV patches (Ogg). Only built with
//! the feature `lossy-audio`, because LAME is LGPL (see `docs/05-licensing.md`).

use std::io::Cursor;

use mp3lame_encoder::{Bitrate, Builder, DualPcm, FlushNoGap, MonoPcm, Quality};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("{0}")]
pub struct LossyError(pub String);

fn fail<T>(what: impl std::fmt::Display) -> Result<T, LossyError> {
    Err(LossyError(what.to_string()))
}

/// A sound as planar samples.
pub struct Decoded {
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
}

/// The MP3 frames without the ID3 tags: the tag of the client's music is found as frames by the
/// reader otherwise.
fn without_id3(bytes: &[u8]) -> &[u8] {
    let mut b = bytes;
    while b.len() >= 10 && &b[..3] == b"ID3" {
        let size = b[6..10]
            .iter()
            .fold(0usize, |n, &x| n << 7 | usize::from(x & 0x7f));
        let footer = if b[5] & 0x10 != 0 { 10 } else { 0 };
        let end = 10 + size + footer;
        if end > b.len() {
            break;
        }
        b = &b[end..];
    }
    if b.len() >= 128 && &b[b.len() - 128..b.len() - 125] == b"TAG" {
        b = &b[..b.len() - 128];
    }
    b
}

/// Decodes an MP3 or Ogg Vorbis file (`hint` is the extension, `mp3` or `ogg`).
pub fn decode(bytes: &[u8], hint: &str) -> Result<Decoded, LossyError> {
    let bytes = if hint == "mp3" {
        without_id3(bytes)
    } else {
        bytes
    };
    let mss = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), Default::default());
    let mut h = Hint::new();
    h.with_extension(hint);
    let mut format = symphonia::default::get_probe()
        .probe(
            &h,
            mss,
            FormatOptions::default(),
            MetadataOptions::default(),
        )
        .map_err(|e| LossyError(format!("not a readable sound: {e}")))?;
    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| LossyError("no audio track".into()))?;
    let id = track.id;
    let params = track
        .codec_params
        .as_ref()
        .and_then(|p| p.audio())
        .ok_or_else(|| LossyError("no audio track".into()))?;
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| LossyError(format!("cannot decode: {e}")))?;
    let mut rate = 0;
    let mut planar: Vec<Vec<f32>> = Vec::new();
    let mut block: Vec<f32> = Vec::new();
    loop {
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(SymError::ResetRequired) => break,
            Err(e) => return fail(e),
        };
        if packet.track_id != id {
            continue;
        }
        match decoder.decode(&packet) {
            Ok(buf) => {
                let n = buf.spec().channels().count();
                if n == 0 {
                    continue;
                }
                rate = buf.spec().rate();
                if planar.is_empty() {
                    planar = vec![Vec::new(); n];
                } else if planar.len() != n {
                    return fail("the number of channels changes inside the file");
                }
                block.resize(buf.samples_interleaved(), 0.0);
                buf.copy_to_slice_interleaved(&mut block);
                for frame in block.chunks_exact(n) {
                    for (c, &s) in planar.iter_mut().zip(frame) {
                        c.push(s);
                    }
                }
            }
            // a damaged frame is skipped, as players do
            Err(SymError::DecodeError(_)) => {}
            Err(e) => return fail(e),
        }
    }
    if planar.is_empty() || rate == 0 {
        return fail("the file has no sound");
    }
    Ok(Decoded {
        rate,
        channels: planar,
    })
}

fn pcm16(c: &[f32]) -> Vec<i16> {
    c.iter()
        .map(|&x| (x * 32768.0).round().clamp(-32768.0, 32767.0) as i16)
        .collect()
}

fn snap(kbps: u32) -> Bitrate {
    const ALL: [(u32, Bitrate); 16] = [
        (8, Bitrate::Kbps8),
        (16, Bitrate::Kbps16),
        (24, Bitrate::Kbps24),
        (32, Bitrate::Kbps32),
        (40, Bitrate::Kbps40),
        (48, Bitrate::Kbps48),
        (64, Bitrate::Kbps64),
        (80, Bitrate::Kbps80),
        (96, Bitrate::Kbps96),
        (112, Bitrate::Kbps112),
        (128, Bitrate::Kbps128),
        (160, Bitrate::Kbps160),
        (192, Bitrate::Kbps192),
        (224, Bitrate::Kbps224),
        (256, Bitrate::Kbps256),
        (320, Bitrate::Kbps320),
    ];
    ALL.iter()
        .min_by_key(|(k, _)| k.abs_diff(kbps))
        .map_or(Bitrate::Kbps192, |&(_, b)| b)
}

/// Average bit rate of a file in kbit/s from its size and the length of its sound.
pub fn average_kbps(file_bytes: usize, seconds: f64) -> Option<u32> {
    (seconds > 0.0).then(|| (file_bytes as f64 * 8.0 / seconds / 1000.0).round() as u32)
}

/// MP3 with LAME at a constant bit rate near `kbps` (default 192).
pub fn encode_mp3(
    rate: u32,
    channels: &[Vec<f32>],
    kbps: Option<u32>,
) -> Result<Vec<u8>, LossyError> {
    const RATES: [u32; 9] = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000];
    if !RATES.contains(&rate) {
        return fail(format!(
            "MP3 cannot hold {rate} Hz (8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100 or 48000)"
        ));
    }
    if channels.is_empty() || channels.len() > 2 {
        return fail("MP3 holds one or two channels");
    }
    let setup = |what: &'static str| move |e| LossyError(format!("MP3 encoder ({what}): {e:?}"));
    let mut b = Builder::new().ok_or_else(|| LossyError("MP3 encoder not available".into()))?;
    b.set_num_channels(channels.len() as u8)
        .map_err(setup("channels"))?;
    b.set_sample_rate(rate).map_err(setup("rate"))?;
    // LAME otherwise picks a lower rate for a low bit rate (24 kHz at 64 kbit/s): the sound keeps its own
    b.set_output_sample_rate(std::num::NonZeroU32::new(rate))
        .map_err(setup("output rate"))?;
    b.set_brate(snap(kbps.unwrap_or(192)))
        .map_err(setup("bit rate"))?;
    b.set_quality(Quality::Best).map_err(setup("quality"))?;
    let mut enc = b.build().map_err(setup("start"))?;
    let mut out = Vec::new();
    let left = pcm16(&channels[0]);
    let n = left.len();
    out.reserve(mp3lame_encoder::max_required_buffer_size(n));
    let wrote = if channels.len() == 2 {
        let right = pcm16(&channels[1]);
        enc.encode_to_vec(
            DualPcm {
                left: &left,
                right: &right,
            },
            &mut out,
        )
    } else {
        enc.encode_to_vec(MonoPcm(&left), &mut out)
    };
    wrote.map_err(|e| LossyError(format!("MP3 encoder: {e:?}")))?;
    enc.flush_to_vec::<FlushNoGap>(&mut out)
        .map_err(|e| LossyError(format!("MP3 encoder: {e:?}")))?;
    Ok(out)
}

/// Ogg Vorbis (libvorbis with the aoTuV patches) at an average bit rate near `kbps`, or at the
/// default quality without one.
pub fn encode_ogg(
    rate: u32,
    channels: &[Vec<f32>],
    kbps: Option<u32>,
) -> Result<Vec<u8>, LossyError> {
    use std::num::{NonZeroU8, NonZeroU32};
    use vorbis_rs::{VorbisBitrateManagementStrategy as Strategy, VorbisEncoderBuilder};
    let ch = u8::try_from(channels.len())
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or_else(|| LossyError("no channels".into()))?;
    let rate = NonZeroU32::new(rate).ok_or_else(|| LossyError("sample rate 0".into()))?;
    let strategy = match kbps.and_then(|k| NonZeroU32::new(k * 1000)) {
        Some(average_bitrate) => Strategy::Abr { average_bitrate },
        None => Strategy::QualityVbr {
            target_quality: 0.5,
        },
    };
    let mut out = Vec::new();
    {
        let mut b = VorbisEncoderBuilder::new(rate, ch, &mut out)
            .map_err(|e| LossyError(format!("Ogg encoder: {e}")))?;
        b.bitrate_management_strategy(strategy);
        let mut enc = b
            .build()
            .map_err(|e| LossyError(format!("Ogg encoder: {e}")))?;
        let n = channels[0].len();
        let mut at = 0;
        while at < n {
            let end = (at + 4096).min(n);
            let block: Vec<&[f32]> = channels.iter().map(|c| &c[at..end]).collect();
            enc.encode_audio_block(&block)
                .map_err(|e| LossyError(format!("Ogg encoder: {e}")))?;
            at = end;
        }
        enc.finish()
            .map_err(|e| LossyError(format!("Ogg encoder: {e}")))?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, ms: u32) -> Vec<Vec<f32>> {
        let n = (rate * ms / 1000) as usize;
        let s: Vec<f32> = (0..n)
            .map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / rate as f32).sin())
            .collect();
        vec![s.clone(), s.iter().map(|x| x * 0.5).collect()]
    }

    fn rms(c: &[f32]) -> f32 {
        (c.iter().map(|x| x * x).sum::<f32>() / c.len().max(1) as f32).sqrt()
    }

    #[test]
    fn mp3_and_ogg_survive_a_round_trip() {
        let t = tone(44100, 800);
        for (name, bytes) in [
            ("mp3", encode_mp3(44100, &t, Some(128)).unwrap()),
            ("ogg", encode_ogg(44100, &t, None).unwrap()),
        ] {
            let d = decode(&bytes, name).unwrap();
            assert_eq!((d.rate, d.channels.len()), (44100, 2), "{name}");
            // encoders add a little silence in front and behind
            let len = d.channels[0].len();
            assert!((35_000..=38_500).contains(&len), "{name}: {len} samples");
            // the loud channel stays about twice as loud as the quiet one
            let ratio = rms(&d.channels[0]) / rms(&d.channels[1]);
            assert!((1.7..2.3).contains(&ratio), "{name}: ratio {ratio}");
            assert!(rms(&d.channels[0]) > 0.25, "{name}");
        }
    }

    #[test]
    fn what_mp3_cannot_hold_is_refused() {
        assert!(encode_mp3(12345, &tone(8000, 10), None).is_err());
        assert!(encode_mp3(44100, &[], None).is_err());
        assert!(decode(b"not a sound at all", "mp3").is_err());
    }

    #[test]
    fn mono_works() {
        let t = vec![tone(22050, 300).remove(0)];
        let d = decode(&encode_mp3(22050, &t, None).unwrap(), "mp3").unwrap();
        assert_eq!((d.rate, d.channels.len()), (22050, 1));
    }
}
