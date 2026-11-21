//! `rgrf audio`: look at and edit sounds (WAV, also the ADPCM ones of the client, MP3 and Ogg Vorbis).

use std::path::Path;

use anyhow::{Result, anyhow};
use rgrf_core::NameMode;
use rgrf_formats::audio::{Audio, Format, Op, Source};
use serde_json::json;

use crate::edit_cmd::{report, write_new};
use crate::spr_cmd::load;

fn audio_from(
    input: &Path,
    entry: Option<&str>,
    names: NameMode,
) -> Result<(Audio, Source, String)> {
    let (data, what) = load(input, entry, names)?;
    let (a, source) = Audio::decode(&data).map_err(|e| anyhow!("{what}: {e}"))?;
    Ok((a, source, what))
}

/// What the result is written as.
pub struct Output {
    pub format: Option<Format>,
    pub kbps: Option<u32>,
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (a, source, what) = audio_from(input, entry, names)?;
    let peak = a.peak();
    let db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        f32::NEG_INFINITY
    };
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "name": what,
                "format": source.format.extension(),
                "kbps": source.kbps,
                "sample_rate": a.rate,
                "channels": a.channels.len(),
                "samples": a.len(),
                "duration_ms": a.duration_ms(),
                "peak": peak,
                "peak_db": db,
            })
        );
    } else {
        println!(
            "{what}: {}, {} Hz, {} channel(s), {} samples, {} ms, peak {db:.1} dB",
            source.format.extension(),
            a.rate,
            a.channels.len(),
            a.len(),
            a.duration_ms()
        );
    }
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn edit(
    input: &Path,
    entry: Option<&str>,
    ops: &[String],
    output: &Path,
    overwrite: bool,
    out: Output,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (mut a, mut source, what) = audio_from(input, entry, names)?;
    if let Some(f) = out.format {
        if f != source.format {
            source.kbps = None;
        }
        source.format = f;
    }
    if out.kbps.is_some() {
        source.kbps = out.kbps;
    }
    let ops: Vec<Op> = ops
        .iter()
        .map(|t| Op::parse(t).map_err(|e| anyhow!("{e}")))
        .collect::<Result<_>>()?;
    a.apply_all(&ops).map_err(|e| anyhow!("{what}: {e}"))?;
    let bytes = a.encode(&source).map_err(|e| anyhow!("{what}: {e}"))?;
    write_new(output, overwrite, &bytes)?;
    report(
        json,
        "audio edit",
        output,
        json!({
            "format": source.format.extension(),
            "operations": ops.iter().map(Op::text).collect::<Vec<_>>(),
            "sample_rate": a.rate,
            "channels": a.channels.len(),
            "duration_ms": a.duration_ms(),
        }),
    );
    Ok(0)
}
