//! `rgrf act`: look at ACT animations and render their frames as PNG (M7b).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rgrf_core::NameMode;
use rgrf_formats::act::{Act, sound_name};
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::render::{bounds_of, render_frame};
use rgrf_formats::spr::{RgbaImage, Spr};
use serde_json::json;

use crate::spr_cmd::load;

fn parse_act(data: &[u8], what: &str) -> Result<Act> {
    Act::parse(data).with_context(|| format!("{what} is not a readable ACT"))
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let act = parse_act(&data, &what)?;
    let frames: usize = act.actions.iter().map(|a| a.frames.len()).sum();
    let layers = act.layers().count();
    if json {
        let actions: Vec<_> = act
            .actions
            .iter()
            .map(|a| json!({"frames": a.frames.len(), "interval": a.interval}))
            .collect();
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "bytes": data.len(),
                "version": act.version.to_string(),
                "actions": actions,
                "frames": frames,
                "layers": layers,
                "sounds": act.sounds.iter().map(sound_name).collect::<Vec<_>>(),
                "warnings": act.warnings,
            })
        );
    } else {
        println!("{what}");
        println!(
            "  version {}, {} bytes, {} actions, {frames} frames, {layers} layers, {} sounds",
            act.version,
            data.len(),
            act.actions.len(),
            act.sounds.len()
        );
        for (i, a) in act.actions.iter().enumerate() {
            println!(
                "  action {i:>3}: {:>3} frames, interval {} ({} ms)",
                a.frames.len(),
                a.interval,
                (a.interval * 25.0).round()
            );
        }
        for (i, s) in act.sounds.iter().enumerate() {
            println!("  sound {i:>3}: {}", sound_name(s));
        }
        for w in &act.warnings {
            println!("  warning: {w}");
        }
    }
    Ok(if act.warnings.is_empty() { 0 } else { 2 })
}

/// The SPR belonging to an ACT: `spr` if given, else the same path with `.spr`.
fn load_spr(input: &Path, entry: Option<&str>, spr: Option<&Path>, names: NameMode) -> Result<Spr> {
    let (data, what) = match (spr, entry) {
        (Some(p), _) => load(p, None, names)?,
        (None, Some(e)) => {
            let stem = e
                .get(..e.len().saturating_sub(4))
                .filter(|_| e.to_ascii_lowercase().ends_with(".act"))
                .ok_or_else(|| anyhow!("'{e}' does not end in .act"))?;
            load(input, Some(&format!("{stem}.spr")), names)?
        }
        (None, None) => {
            let p = input.with_extension("spr");
            if !p.exists() {
                bail!("{} does not exist; pass the sprite with --spr", p.display());
            }
            load(&p, None, names)?
        }
    };
    Spr::parse(&data).with_context(|| format!("{what} is not a readable SPR"))
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    input: &Path,
    entry: Option<&str>,
    spr: Option<&Path>,
    action: usize,
    frame: Option<usize>,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let act = parse_act(&data, &what)?;
    let spr = load_spr(input, entry, spr, names)?;
    let a = act.actions.get(action).ok_or_else(|| {
        anyhow!(
            "{what} has no action {action} ({} actions)",
            act.actions.len()
        )
    })?;
    let wanted: Vec<usize> = match frame {
        Some(f) => {
            if f >= a.frames.len() {
                bail!(
                    "action {action} has no frame {f} ({} frames)",
                    a.frames.len()
                );
            }
            vec![f]
        }
        None => (0..a.frames.len()).collect(),
    };
    // One canvas for all frames of the action, so that an animation does not jump around.
    let Some(bounds) = bounds_of(&act, &spr, action, 0..a.frames.len())? else {
        bail!("action {action} draws nothing (no layer with an image)");
    };
    let single = frame.is_some();
    let target = |f: usize| -> PathBuf {
        if single {
            output.to_path_buf()
        } else {
            output.join(format!("{f:03}.png"))
        }
    };
    if !overwrite {
        for &f in &wanted {
            if target(f).exists() {
                bail!("{} exists already (use --overwrite)", target(f).display());
            }
        }
    }
    if !single {
        fs::create_dir_all(output)
            .with_context(|| format!("cannot create {}", output.display()))?;
    }
    let mut written = Vec::new();
    for &f in &wanted {
        let c = render_frame(&act, &spr, action, f, bounds)?;
        let (Ok(width), Ok(height)) = (u16::try_from(c.width), u16::try_from(c.height)) else {
            bail!(
                "rendered frame is {} x {}, too large for a PNG here",
                c.width,
                c.height
            );
        };
        let png = rgba_to_png(&RgbaImage {
            width,
            height,
            pixels: c.pixels,
        })?;
        fs::write(target(f), png)
            .with_context(|| format!("cannot write {}", target(f).display()))?;
        written.push(target(f));
    }
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "action": action,
                "origin": {"x": -bounds.min_x, "y": -bounds.min_y},
                "width": bounds.width(),
                "height": bounds.height(),
                "files": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })
        );
    } else {
        println!(
            "{what}: action {action}, {} frame(s), {} x {} px, origin at ({}, {}) -> {}",
            written.len(),
            bounds.width(),
            bounds.height(),
            -bounds.min_x,
            -bounds.min_y,
            output.display()
        );
    }
    Ok(if act.warnings.is_empty() { 0 } else { 2 })
}
