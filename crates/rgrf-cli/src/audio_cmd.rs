//! `rgrf audio`: look at and edit sounds (WAV, also the ADPCM ones of the client).

use std::path::Path;

use anyhow::{Result, anyhow};
use rgrf_core::NameMode;
use rgrf_formats::audio::{Audio, Op};
use serde_json::json;

use crate::edit_cmd::{report, write_new};
use crate::spr_cmd::load;

fn audio_from(input: &Path, entry: Option<&str>, names: NameMode) -> Result<(Audio, String)> {
    let (data, what) = load(input, entry, names)?;
    let a = Audio::from_wav(&data).map_err(|e| anyhow!("{what}: {e}"))?;
    Ok((a, what))
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (a, what) = audio_from(input, entry, names)?;
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
            "{what}: {} Hz, {} channel(s), {} samples, {} ms, peak {db:.1} dB",
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
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (mut a, what) = audio_from(input, entry, names)?;
    let ops: Vec<Op> = ops
        .iter()
        .map(|t| Op::parse(t).map_err(|e| anyhow!("{e}")))
        .collect::<Result<_>>()?;
    a.apply_all(&ops).map_err(|e| anyhow!("{what}: {e}"))?;
    write_new(output, overwrite, &a.to_wav())?;
    report(
        json,
        "audio edit",
        output,
        json!({
            "operations": ops.iter().map(Op::text).collect::<Vec<_>>(),
            "sample_rate": a.rate,
            "channels": a.channels.len(),
            "duration_ms": a.duration_ms(),
        }),
    );
    Ok(0)
}
