//! Editing sounds of the client: cut, trim, fade, volume, resample, a few effects. A sound is read
//! with [`wav::decode`] (PCM and both ADPCM kinds; MP3 and Ogg Vorbis with the feature
//! `lossy-audio`), edited as planar `f32` and written as 16-bit PCM WAV (MP3 and Ogg again as what
//! they were, see [`Audio::decode`]). The operations have a short text form (`gain:3`, `trim:200-900`, `echo:250,0.4,0.3`) that the
//! CLI and the GUI share, see [`Op::parse`].

use crate::wav::{self, WavError};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AudioError {
    #[error(transparent)]
    Wav(#[from] WavError),
    #[error("unknown operation '{0}'")]
    Unknown(String),
    #[error("{0}: {1}")]
    Args(&'static str, String),
    #[error("{0}")]
    Lossy(String),
    #[error("MP3 and Ogg Vorbis sounds can only be edited in a build with the feature lossy-audio")]
    NoLossy,
    #[error("the range {0}-{1} ms is empty or outside the sound ({2} ms long)")]
    Range(u32, u32, u32),
}

/// A time range in milliseconds; `end` is exclusive and `None` means "to the end".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub start: u32,
    pub end: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Keeps only the range.
    Trim(Range),
    /// Removes the range.
    Cut(Range),
    /// Replaces the range by silence.
    Silence(Range),
    /// Volume change in dB, on the range or everywhere.
    Gain(f32, Option<Range>),
    /// Scales the loudest peak to this level in dB (at most 0).
    Normalize(f32),
    /// Fade-in over `ms` at the start of the range (or the sound).
    FadeIn(u32, Option<Range>),
    /// Fade-out over `ms` at the end of the range (or the sound).
    FadeOut(u32, Option<Range>),
    Reverse(Option<Range>),
    /// All channels mixed into one.
    Mono,
    /// A new sample rate (the sound keeps its duration and pitch).
    Resample(u32),
    /// Plays faster or slower, which also changes the pitch (the rate stays).
    Speed(f32),
    /// Repeats the sound after `delay_ms`, each time `feedback` as loud, mixed with `mix`.
    Echo {
        delay_ms: u32,
        feedback: f32,
        mix: f32,
    },
    /// A room: `room` 0..1 is its size, `mix` the amount of reverb.
    Reverb {
        room: f32,
        mix: f32,
    },
    LowPass(f32, Option<Range>),
    HighPass(f32, Option<Range>),
}

/// The file formats of sounds the editor reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Wav,
    Mp3,
    Ogg,
}

impl Format {
    /// The format by the first bytes of a file.
    pub fn detect(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"RIFF") {
            Some(Self::Wav)
        } else if bytes.starts_with(b"OggS") {
            Some(Self::Ogg)
        } else if bytes.starts_with(b"ID3")
            || (bytes.len() > 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0)
        {
            Some(Self::Mp3)
        } else {
            None
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Mp3 => "mp3",
            Self::Ogg => "ogg",
        }
    }
}

/// What a sound was read from: the format and, for MP3 and Ogg, its average bit rate (the edited
/// sound is written with about the same).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Source {
    pub format: Format,
    pub kbps: Option<u32>,
}

/// Planar samples in -1..1.
#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub rate: u32,
    pub channels: Vec<Vec<f32>>,
}

fn num<T: std::str::FromStr>(op: &'static str, s: &str) -> Result<T, AudioError> {
    s.trim()
        .parse()
        .map_err(|_| AudioError::Args(op, format!("'{s}' is not a number")))
}

fn parse_range(op: &'static str, s: &str) -> Result<Range, AudioError> {
    let (a, b) = s
        .split_once('-')
        .ok_or_else(|| AudioError::Args(op, format!("'{s}' is not START-END in ms")))?;
    let start = if a.trim().is_empty() { 0 } else { num(op, a)? };
    let end = if b.trim().is_empty() {
        None
    } else {
        Some(num(op, b)?)
    };
    Ok(Range { start, end })
}

impl Op {
    /// `name[:arguments][@START-END]`, times in milliseconds: `trim:100-900`, `cut:0-250`,
    /// `silence:300-400`, `gain:-3`, `gain:6@0-500`, `normalize:-1`, `fadein:200`, `fadeout:500`,
    /// `reverse`, `mono`, `resample:22050`, `speed:1.25`, `echo:250,0.4,0.3`, `reverb:0.5,0.25`,
    /// `lowpass:4000`, `highpass:200`.
    pub fn parse(text: &str) -> Result<Self, AudioError> {
        let (head, at) = match text.split_once('@') {
            Some((h, r)) => (h, Some(parse_range("range", r)?)),
            None => (text, None),
        };
        let (name, args) = head.split_once(':').unwrap_or((head, ""));
        let list: Vec<&str> = if args.is_empty() {
            Vec::new()
        } else {
            args.split(',').collect()
        };
        let arg = |op: &'static str, i: usize, default: Option<f32>| -> Result<f32, AudioError> {
            match list.get(i) {
                Some(s) => num(op, s),
                None => {
                    default.ok_or_else(|| AudioError::Args(op, "an argument is missing".into()))
                }
            }
        };
        Ok(match name.trim().to_ascii_lowercase().as_str() {
            "trim" => Self::Trim(parse_range("trim", args)?),
            "cut" => Self::Cut(parse_range("cut", args)?),
            "silence" => Self::Silence(parse_range("silence", args)?),
            "gain" => Self::Gain(arg("gain", 0, None)?, at),
            "normalize" => Self::Normalize(arg("normalize", 0, Some(-1.0))?.min(0.0)),
            "fadein" => Self::FadeIn(arg("fadein", 0, None)? as u32, at),
            "fadeout" => Self::FadeOut(arg("fadeout", 0, None)? as u32, at),
            "reverse" => Self::Reverse(at),
            "mono" => Self::Mono,
            "resample" => {
                let r = arg("resample", 0, None)? as u32;
                if !(2000..=192_000).contains(&r) {
                    return Err(AudioError::Args(
                        "resample",
                        "the rate must be 2000 to 192000 Hz".into(),
                    ));
                }
                Self::Resample(r)
            }
            "speed" => {
                let f = arg("speed", 0, None)?;
                if !(0.25..=4.0).contains(&f) {
                    return Err(AudioError::Args(
                        "speed",
                        "the factor must be 0.25 to 4".into(),
                    ));
                }
                Self::Speed(f)
            }
            "echo" => Self::Echo {
                delay_ms: (arg("echo", 0, Some(250.0))? as u32).clamp(10, 2000),
                feedback: arg("echo", 1, Some(0.4))?.clamp(0.0, 0.95),
                mix: arg("echo", 2, Some(0.3))?.clamp(0.0, 1.0),
            },
            "reverb" => Self::Reverb {
                room: arg("reverb", 0, Some(0.5))?.clamp(0.0, 1.0),
                mix: arg("reverb", 1, Some(0.25))?.clamp(0.0, 1.0),
            },
            "lowpass" => Self::LowPass(arg("lowpass", 0, None)?, at),
            "highpass" => Self::HighPass(arg("highpass", 0, None)?, at),
            other => return Err(AudioError::Unknown(other.into())),
        })
    }

    /// The text form of the operation (see [`Op::parse`]).
    pub fn text(&self) -> String {
        let r = |r: &Range| {
            format!(
                "{}-{}",
                r.start,
                r.end.map_or(String::new(), |e| e.to_string())
            )
        };
        let at = |o: &Option<Range>| o.as_ref().map_or(String::new(), |x| format!("@{}", r(x)));
        match self {
            Self::Trim(x) => format!("trim:{}", r(x)),
            Self::Cut(x) => format!("cut:{}", r(x)),
            Self::Silence(x) => format!("silence:{}", r(x)),
            Self::Gain(d, x) => format!("gain:{d}{}", at(x)),
            Self::Normalize(d) => format!("normalize:{d}"),
            Self::FadeIn(m, x) => format!("fadein:{m}{}", at(x)),
            Self::FadeOut(m, x) => format!("fadeout:{m}{}", at(x)),
            Self::Reverse(x) => format!("reverse{}", at(x)),
            Self::Mono => "mono".into(),
            Self::Resample(h) => format!("resample:{h}"),
            Self::Speed(f) => format!("speed:{f}"),
            Self::Echo {
                delay_ms,
                feedback,
                mix,
            } => format!("echo:{delay_ms},{feedback},{mix}"),
            Self::Reverb { room, mix } => format!("reverb:{room},{mix}"),
            Self::LowPass(h, x) => format!("lowpass:{h}{}", at(x)),
            Self::HighPass(h, x) => format!("highpass:{h}{}", at(x)),
        }
    }
}

/// One biquad (RBJ cookbook), run over a channel.
fn biquad(x: &mut [f32], rate: u32, hz: f32, high: bool) {
    let hz = hz.clamp(10.0, rate as f32 * 0.49);
    let w = 2.0 * std::f32::consts::PI * hz / rate as f32;
    let (sin, cos) = w.sin_cos();
    let alpha = sin / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
    let (b0, b1, b2) = if high {
        ((1.0 + cos) / 2.0, -(1.0 + cos), (1.0 + cos) / 2.0)
    } else {
        ((1.0 - cos) / 2.0, 1.0 - cos, (1.0 - cos) / 2.0)
    };
    let (a0, a1, a2) = (1.0 + alpha, -2.0 * cos, 1.0 - alpha);
    let (b0, b1, b2, a1, a2) = (b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0);
    let (mut x1, mut x2, mut y1, mut y2) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
    for s in x.iter_mut() {
        let y = b0 * *s + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
        x2 = x1;
        x1 = *s;
        y2 = y1;
        y1 = y;
        *s = y;
    }
}

/// A fourth-order filter: two biquads in a row.
fn filter(x: &mut [f32], rate: u32, hz: f32, high: bool) {
    biquad(x, rate, hz, high);
    biquad(x, rate, hz, high);
}

/// Catmull-Rom interpolation of `x` at the fractional position `pos`.
fn cubic(x: &[f32], pos: f64) -> f32 {
    let i = pos.floor() as isize;
    let t = (pos - pos.floor()) as f32;
    let at = |k: isize| x[k.clamp(0, x.len() as isize - 1) as usize];
    let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
    p1 + 0.5
        * t
        * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)))
}

/// The samples of `x` played at `ratio` times the speed (shorter for ratio above 1).
fn stretch(x: &[f32], ratio: f64) -> Vec<f32> {
    if x.is_empty() {
        return Vec::new();
    }
    let n = ((x.len() as f64) / ratio).round().max(1.0) as usize;
    (0..n).map(|k| cubic(x, k as f64 * ratio)).collect()
}

impl Audio {
    pub fn from_wav(bytes: &[u8]) -> Result<Self, AudioError> {
        let (rate, channels) = wav::decode(bytes)?;
        Ok(Self { rate, channels })
    }

    pub fn to_wav(&self) -> Vec<u8> {
        wav::encode_pcm16(self.rate, &self.channels)
    }

    /// Reads a WAV, MP3 or Ogg Vorbis file by its first bytes.
    pub fn decode(bytes: &[u8]) -> Result<(Self, Source), AudioError> {
        let format = Format::detect(bytes).unwrap_or(Format::Wav);
        if format == Format::Wav {
            let a = Self::from_wav(bytes)?;
            return Ok((a, Source { format, kbps: None }));
        }
        #[cfg(feature = "lossy-audio")]
        {
            let d = crate::lossy::decode(bytes, format.extension())
                .map_err(|e| AudioError::Lossy(e.0))?;
            let a = Self {
                rate: d.rate,
                channels: d.channels,
            };
            let kbps = crate::lossy::average_kbps(bytes.len(), f64::from(a.duration_ms()) / 1000.0);
            Ok((a, Source { format, kbps }))
        }
        #[cfg(not(feature = "lossy-audio"))]
        Err(AudioError::NoLossy)
    }

    /// Writes the sound in the format it was read in.
    pub fn encode(&self, source: &Source) -> Result<Vec<u8>, AudioError> {
        match source.format {
            Format::Wav => Ok(self.to_wav()),
            #[cfg(feature = "lossy-audio")]
            Format::Mp3 => crate::lossy::encode_mp3(self.rate, &self.channels, source.kbps)
                .map_err(|e| AudioError::Lossy(e.0)),
            #[cfg(feature = "lossy-audio")]
            Format::Ogg => crate::lossy::encode_ogg(self.rate, &self.channels, source.kbps)
                .map_err(|e| AudioError::Lossy(e.0)),
            #[cfg(not(feature = "lossy-audio"))]
            _ => Err(AudioError::NoLossy),
        }
    }

    /// Samples per channel.
    pub fn len(&self) -> usize {
        self.channels.first().map_or(0, Vec::len)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn duration_ms(&self) -> u32 {
        (self.len() as u64 * 1000 / u64::from(self.rate.max(1))) as u32
    }

    fn ms_to_sample(&self, ms: u32) -> usize {
        (u64::from(ms) * u64::from(self.rate) / 1000) as usize
    }

    /// The sample range of a time range (the end clipped to the sound).
    fn span(&self, r: &Range) -> Result<(usize, usize), AudioError> {
        let a = self.ms_to_sample(r.start);
        let b = r
            .end
            .map_or(self.len(), |e| self.ms_to_sample(e))
            .min(self.len());
        if a >= b {
            return Err(AudioError::Range(
                r.start,
                r.end.unwrap_or(0),
                self.duration_ms(),
            ));
        }
        Ok((a, b))
    }

    fn whole_or(&self, r: &Option<Range>) -> Result<(usize, usize), AudioError> {
        match r {
            Some(r) => self.span(r),
            None => Ok((0, self.len())),
        }
    }

    /// The loudest sample (absolute value) over all channels.
    pub fn peak(&self) -> f32 {
        self.channels
            .iter()
            .flatten()
            .fold(0.0f32, |m, &x| m.max(x.abs()))
    }

    /// Min and max per bucket over all channels, `buckets` of them, for drawing the wave.
    pub fn peaks(&self, buckets: usize) -> Vec<(f32, f32)> {
        let n = self.len();
        if n == 0 || buckets == 0 {
            return Vec::new();
        }
        (0..buckets)
            .map(|b| {
                let (lo, hi) = (
                    b * n / buckets,
                    ((b + 1) * n / buckets).max(b * n / buckets + 1),
                );
                let mut m = (0.0f32, 0.0f32);
                for c in &self.channels {
                    for &x in &c[lo.min(n - 1)..hi.min(n)] {
                        m = (m.0.min(x), m.1.max(x));
                    }
                }
                m
            })
            .collect()
    }

    pub fn apply(&mut self, op: &Op) -> Result<(), AudioError> {
        let rate = self.rate;
        match op {
            Op::Trim(r) => {
                let (a, b) = self.span(r)?;
                for c in &mut self.channels {
                    *c = c[a..b].to_vec();
                }
            }
            Op::Cut(r) => {
                let (a, b) = self.span(r)?;
                for c in &mut self.channels {
                    c.drain(a..b);
                }
            }
            Op::Silence(r) => {
                let (a, b) = self.span(r)?;
                for c in &mut self.channels {
                    c[a..b].fill(0.0);
                }
            }
            Op::Gain(db, r) => {
                let (a, b) = self.whole_or(r)?;
                let g = 10f32.powf(db / 20.0);
                for c in &mut self.channels {
                    c[a..b].iter_mut().for_each(|x| *x *= g);
                }
            }
            Op::Normalize(db) => {
                let p = self.peak();
                if p > 0.0 {
                    let g = 10f32.powf(db.min(0.0) / 20.0) / p;
                    for c in &mut self.channels {
                        c.iter_mut().for_each(|x| *x *= g);
                    }
                }
            }
            Op::FadeIn(ms, r) => {
                let (a, b) = self.whole_or(r)?;
                let n = self.ms_to_sample(*ms).clamp(1, b - a);
                for c in &mut self.channels {
                    for (k, x) in c[a..a + n].iter_mut().enumerate() {
                        *x *= k as f32 / n as f32;
                    }
                }
            }
            Op::FadeOut(ms, r) => {
                let (a, b) = self.whole_or(r)?;
                let n = self.ms_to_sample(*ms).clamp(1, b - a);
                for c in &mut self.channels {
                    for (k, x) in c[b - n..b].iter_mut().enumerate() {
                        *x *= 1.0 - (k + 1) as f32 / n as f32;
                    }
                }
            }
            Op::Reverse(r) => {
                let (a, b) = self.whole_or(r)?;
                for c in &mut self.channels {
                    c[a..b].reverse();
                }
            }
            Op::Mono => {
                if self.channels.len() > 1 {
                    let n = self.channels.len() as f32;
                    let mixed: Vec<f32> = (0..self.len())
                        .map(|i| self.channels.iter().map(|c| c[i]).sum::<f32>() / n)
                        .collect();
                    self.channels = vec![mixed];
                }
            }
            Op::Resample(new) => {
                if *new != rate {
                    let ratio = f64::from(rate) / f64::from(*new);
                    for c in &mut self.channels {
                        if *new < rate {
                            filter(c, rate, *new as f32 * 0.45, false);
                        }
                        *c = stretch(c, ratio);
                    }
                    self.rate = *new;
                }
            }
            Op::Speed(f) => {
                for c in &mut self.channels {
                    *c = stretch(c, f64::from(*f));
                }
            }
            Op::Echo {
                delay_ms,
                feedback,
                mix,
            } => {
                let d = self.ms_to_sample(*delay_ms).max(1);
                // enough repeats to fade below about -60 dB, at most 4 seconds of tail
                let repeats = if *feedback < 0.01 {
                    1
                } else {
                    (-3.0 / feedback.log10()).ceil().clamp(1.0, 40.0) as usize
                };
                let tail = (d * repeats).min(4 * rate as usize);
                for c in &mut self.channels {
                    let dry = c.clone();
                    c.resize(dry.len() + tail, 0.0);
                    let mut out = c.clone();
                    for (i, o) in out.iter_mut().enumerate() {
                        let mut gain = *feedback;
                        let mut at = i as isize - d as isize;
                        while at >= 0 && gain > 0.001 {
                            if let Some(&s) = dry.get(at as usize) {
                                *o += s * gain * mix;
                            }
                            gain *= *feedback;
                            at -= d as isize;
                        }
                    }
                    *c = out;
                }
            }
            Op::Reverb { room, mix } => {
                let tail = (rate as f32 * (0.4 + 1.6 * room)) as usize;
                for c in &mut self.channels {
                    let len = c.len();
                    c.resize(len + tail, 0.0);
                    let wet = reverb_channel(c, rate, *room);
                    for (x, w) in c.iter_mut().zip(wet) {
                        *x = *x * (1.0 - mix * 0.5) + w * mix;
                    }
                }
            }
            Op::LowPass(hz, r) | Op::HighPass(hz, r) => {
                let (a, b) = self.whole_or(r)?;
                let high = matches!(op, Op::HighPass(..));
                for c in &mut self.channels {
                    filter(&mut c[a..b], rate, *hz, high);
                }
            }
        }
        Ok(())
    }

    pub fn apply_all(&mut self, ops: &[Op]) -> Result<(), AudioError> {
        ops.iter().try_for_each(|op| self.apply(op))
    }
}

/// The reverberation of one channel: four damped comb filters side by side, two all-passes after them.
fn reverb_channel(x: &[f32], rate: u32, room: f32) -> Vec<f32> {
    let scale = rate as f32 / 44_100.0;
    let feedback = 0.7 + 0.28 * room;
    let damp = 0.2f32;
    let mut out = vec![0.0f32; x.len()];
    for d in [1557.0f32, 1617.0, 1491.0, 1422.0] {
        let len = ((d * scale) as usize).max(1);
        let mut buf = vec![0.0f32; len];
        let (mut pos, mut store) = (0usize, 0.0f32);
        for (i, &s) in x.iter().enumerate() {
            let y = buf[pos];
            store = y * (1.0 - damp) + store * damp;
            buf[pos] = s * 0.15 + store * feedback;
            pos = (pos + 1) % len;
            out[i] += y;
        }
    }
    for d in [225.0f32, 556.0] {
        let len = ((d * scale) as usize).max(1);
        let mut buf = vec![0.0f32; len];
        let mut pos = 0usize;
        for o in out.iter_mut() {
            let b = buf[pos];
            let y = -*o + b;
            buf[pos] = *o + b * 0.5;
            pos = (pos + 1) % len;
            *o = y;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, ms: u32, hz: f32) -> Audio {
        let n = (u64::from(rate) * u64::from(ms) / 1000) as usize;
        Audio {
            rate,
            channels: vec![
                (0..n)
                    .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
                    .collect(),
            ],
        }
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    #[test]
    fn operations_have_a_text_form_that_round_trips() {
        for t in [
            "trim:100-900",
            "cut:0-250",
            "silence:300-400",
            "gain:-3",
            "gain:6@0-500",
            "normalize:-1",
            "fadein:200",
            "fadeout:500@100-1000",
            "reverse",
            "mono",
            "resample:22050",
            "speed:1.25",
            "echo:250,0.4,0.3",
            "reverb:0.5,0.25",
            "lowpass:4000",
            "highpass:200@0-",
        ] {
            let op = Op::parse(t).unwrap();
            assert_eq!(Op::parse(&op.text()).unwrap(), op, "{t}");
        }
        assert!(matches!(Op::parse("nope"), Err(AudioError::Unknown(_))));
        assert!(Op::parse("gain").is_err());
        assert!(Op::parse("resample:5").is_err());
        assert!(Op::parse("trim:abc").is_err());
    }

    #[test]
    fn trim_cut_and_silence_work_in_milliseconds() {
        let mut a = tone(8000, 1000, 440.0);
        a.apply(&Op::parse("trim:200-700").unwrap()).unwrap();
        assert_eq!(a.duration_ms(), 500);
        a.apply(&Op::parse("cut:0-100").unwrap()).unwrap();
        assert_eq!(a.len(), 3200);
        a.apply(&Op::parse("silence:0-100").unwrap()).unwrap();
        assert!(a.channels[0][..800].iter().all(|&x| x == 0.0));
        assert!(a.channels[0][800..].iter().any(|&x| x != 0.0));
        assert!(
            a.apply(&Op::parse("cut:900-950").unwrap()).is_err(),
            "outside the sound"
        );
    }

    #[test]
    fn gain_normalize_and_fades_change_the_level() {
        let mut a = tone(8000, 500, 300.0);
        let before = rms(&a.channels[0]);
        a.apply(&Op::Gain(-6.0, None)).unwrap();
        assert!((rms(&a.channels[0]) / before - 0.5012).abs() < 0.01);
        a.apply(&Op::Normalize(0.0)).unwrap();
        assert!((a.peak() - 1.0).abs() < 1e-3);
        a.apply(&Op::parse("fadein:100").unwrap()).unwrap();
        a.apply(&Op::parse("fadeout:100").unwrap()).unwrap();
        assert!(a.channels[0][0].abs() < 1e-3 && a.channels[0][a.len() - 1].abs() < 1e-3);
        assert!(a.channels[0][a.len() / 2].abs() > 0.1 || a.peak() > 0.5);
    }

    #[test]
    fn resample_keeps_the_duration_and_speed_changes_it() {
        let mut a = tone(44_100, 1000, 440.0);
        a.apply(&Op::Resample(22_050)).unwrap();
        assert_eq!((a.rate, a.duration_ms()), (22_050, 1000));
        // the tone is still there at about the same level
        assert!(
            (rms(&a.channels[0]) - 0.3535).abs() < 0.03,
            "{}",
            rms(&a.channels[0])
        );
        let mut b = tone(8000, 1000, 440.0);
        b.apply(&Op::Speed(2.0)).unwrap();
        assert_eq!((b.rate, b.duration_ms()), (8000, 500));
    }

    #[test]
    fn filters_cut_the_wrong_side_and_effects_add_a_tail() {
        let mut low = tone(16_000, 500, 200.0);
        let mut high = tone(16_000, 500, 6000.0);
        let (l0, h0) = (rms(&low.channels[0]), rms(&high.channels[0]));
        low.apply(&Op::LowPass(1000.0, None)).unwrap();
        high.apply(&Op::LowPass(1000.0, None)).unwrap();
        assert!(rms(&low.channels[0]) > 0.9 * l0);
        assert!(rms(&high.channels[0]) < 0.05 * h0);
        let mut a = tone(8000, 200, 400.0);
        let n = a.len();
        a.apply(&Op::parse("echo:100,0.5,0.5").unwrap()).unwrap();
        assert!(a.len() > n);
        let n = a.len();
        a.apply(&Op::parse("reverb:0.5,0.3").unwrap()).unwrap();
        assert!(a.len() > n && a.peak().is_finite());
    }

    #[test]
    fn a_wav_round_trips_and_stereo_becomes_mono() {
        let mut a = tone(22_050, 100, 500.0);
        a.channels.push(a.channels[0].iter().map(|x| -x).collect());
        let back = Audio::from_wav(&a.to_wav()).unwrap();
        assert_eq!(
            (back.rate, back.channels.len(), back.len()),
            (22_050, 2, a.len())
        );
        assert!((back.channels[0][10] - a.channels[0][10]).abs() < 1e-3);
        let mut m = back;
        m.apply(&Op::Mono).unwrap();
        assert!(
            m.channels.len() == 1 && m.peak() < 1e-3,
            "opposite channels cancel"
        );
        assert_eq!(m.peaks(10).len(), 10);
        assert!(Audio::from_wav(b"nope").is_err());
    }
}
