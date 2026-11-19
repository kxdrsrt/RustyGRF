//! `rgrf act`: look at ACT animations and render their frames as PNG or an animated GIF (M7b).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rgrf_core::NameMode;
use rgrf_formats::act::{Act, sound_name};
use rgrf_formats::gif_io::{delay_cs, encode_gif, upscale};
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::render::{Part, bounds_of_parts, render_parts};
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

/// Name of the SPR that belongs to an ACT path (same path, `.spr`).
fn spr_name(act_path: &str) -> Result<String> {
    if !act_path.to_ascii_lowercase().ends_with(".act") {
        bail!("'{act_path}' does not end in .act");
    }
    Ok(format!("{}spr", &act_path[..act_path.len() - 3]))
}

/// An ACT with its SPR, from a GRF entry or from loose files.
fn load_pair(
    input: &Path,
    entry: Option<&str>,
    spr_override: Option<&Path>,
    names: NameMode,
) -> Result<(Act, Spr, String)> {
    let (data, what) = load(input, entry, names)?;
    let act = parse_act(&data, &what)?;
    let (sdata, swhat) = match (spr_override, entry) {
        (Some(p), _) => load(p, None, names)?,
        (None, Some(e)) => load(input, Some(&spr_name(e)?), names)?,
        (None, None) => {
            let p = input.with_extension("spr");
            if !p.exists() {
                bail!("{} does not exist; pass the sprite with --spr", p.display());
            }
            load(&p, None, names)?
        }
    };
    let spr = Spr::parse(&sdata).with_context(|| format!("{swhat} is not a readable SPR"))?;
    Ok((act, spr, what))
}

pub struct RenderArgs<'a> {
    pub input: &'a Path,
    pub entry: Option<&'a str>,
    pub spr: Option<&'a Path>,
    /// More ACTs drawn on top (loose files, or entries of the same GRF), each with its own SPR.
    pub with: &'a [String],
    pub no_anchor: bool,
    pub action: usize,
    pub frame: Option<usize>,
    pub gif: bool,
    pub scale: u32,
    pub output: &'a Path,
    pub overwrite: bool,
    pub names: NameMode,
    pub json: bool,
}

pub fn render(a: &RenderArgs<'_>) -> Result<i32> {
    if a.gif && a.frame.is_some() {
        bail!("--gif draws a whole action; leave out --frame");
    }
    let (base_act, base_spr, what) = load_pair(a.input, a.entry, a.spr, a.names)?;
    let mut loaded = Vec::new();
    for w in a.with {
        let pair = if a.entry.is_some() {
            load_pair(a.input, Some(w), None, a.names)?
        } else {
            load_pair(Path::new(w), None, None, a.names)?
        };
        loaded.push(pair);
    }
    let mut parts = vec![Part::new(&base_act, &base_spr)];
    parts.extend(loaded.iter().map(|(act, spr, _)| Part {
        act,
        spr,
        anchored: !a.no_anchor,
        under: false,
    }));

    let n_frames = base_act
        .actions
        .get(a.action)
        .ok_or_else(|| {
            anyhow!(
                "{what} has no action {} ({} actions)",
                a.action,
                base_act.actions.len()
            )
        })?
        .frames
        .len();
    let wanted: Vec<usize> = match a.frame {
        Some(f) => {
            if f >= n_frames {
                bail!("action {} has no frame {f} ({n_frames} frames)", a.action);
            }
            vec![f]
        }
        None => (0..n_frames).collect(),
    };
    // One canvas for all frames of the action, so that an animation does not jump around.
    let Some(bounds) = bounds_of_parts(&parts, a.action, 0..n_frames)? else {
        bail!("action {} draws nothing (no layer with an image)", a.action);
    };
    let single = a.frame.is_some();
    let target = |f: usize| -> PathBuf {
        if a.gif || single {
            a.output.to_path_buf()
        } else {
            a.output.join(format!("{f:03}.png"))
        }
    };
    if !a.overwrite {
        for &f in &wanted {
            if target(f).exists() {
                bail!("{} exists already (use --overwrite)", target(f).display());
            }
        }
    }
    let scale = a.scale.max(1);
    let canvases = wanted
        .iter()
        .map(|&f| Ok(render_parts(&parts, a.action, f, bounds)?))
        .collect::<Result<Vec<_>>>()?;
    let mut written = Vec::new();
    if a.gif {
        let delay = delay_cs(base_act.actions[a.action].interval);
        let bytes = encode_gif(&canvases, &vec![delay; canvases.len()], scale)?;
        fs::write(a.output, bytes)
            .with_context(|| format!("cannot write {}", a.output.display()))?;
        written.push(a.output.to_path_buf());
    } else {
        if !single {
            fs::create_dir_all(a.output)
                .with_context(|| format!("cannot create {}", a.output.display()))?;
        }
        for (&f, c) in wanted.iter().zip(canvases) {
            let s = scale as usize;
            let (w, h) = (c.width as usize * s, c.height as usize * s);
            let (Ok(width), Ok(height)) = (u16::try_from(w), u16::try_from(h)) else {
                bail!("rendered frame is {w} x {h}, too large for a PNG here");
            };
            let pixels = if s == 1 {
                c.pixels
            } else {
                upscale(&c.pixels, c.width as usize, c.height as usize, s)
            };
            let png = rgba_to_png(&RgbaImage {
                width,
                height,
                pixels,
            })?;
            fs::write(target(f), png)
                .with_context(|| format!("cannot write {}", target(f).display()))?;
            written.push(target(f));
        }
    }
    let warned = !base_act.warnings.is_empty();
    if a.json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "action": a.action,
                "parts": parts.len(),
                "origin": {"x": -bounds.min_x * scale as i32, "y": -bounds.min_y * scale as i32},
                "width": bounds.width() * scale,
                "height": bounds.height() * scale,
                "files": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })
        );
    } else {
        println!(
            "{what}: action {}, {} frame(s){}, {} x {} px, origin at ({}, {}) -> {}",
            a.action,
            wanted.len(),
            if parts.len() > 1 {
                format!(", {} parts", parts.len())
            } else {
                String::new()
            },
            bounds.width() * scale,
            bounds.height() * scale,
            -bounds.min_x * scale as i32,
            -bounds.min_y * scale as i32,
            a.output.display()
        );
    }
    Ok(if warned { 2 } else { 0 })
}
