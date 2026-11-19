//! `rgrf pal`: look at and edit palettes, and preview a sprite with several of them (M7b).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use rgrf_core::NameMode;
use rgrf_formats::gif_io::upscale;
use rgrf_formats::pal::Pal;
use rgrf_formats::png_io::{Frame, frame_from_png, rgba_to_png};
use rgrf_formats::render::{Part, bounds_of_parts, render_parts};
use rgrf_formats::spr::{RgbaImage, Spr};
use serde_json::json;

use crate::edit_cmd::{report, write_new};
use crate::spr_cmd::load;

/// A palette from a `.pal` file or the palette of an `.spr`, as a loose file or a GRF entry.
fn load_pal(input: &Path, entry: Option<&str>, names: NameMode) -> Result<(Pal, String)> {
    let (data, what) = load(input, entry, names)?;
    if what.to_ascii_lowercase().ends_with(".spr") {
        let spr = Spr::parse(&data).with_context(|| format!("{what} is not a readable SPR"))?;
        let colors = spr
            .palette
            .ok_or_else(|| anyhow!("{what} has no palette (no palette images)"))?;
        return Ok((Pal { colors }, what));
    }
    let pal = Pal::parse(&data).with_context(|| what.clone())?;
    Ok((pal, what))
}

fn hex(c: &[u8]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (pal, what) = load_pal(input, entry, names)?;
    let colors: Vec<String> = (0..256).map(|i| hex(&pal.colors[4 * i..])).collect();
    if json {
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "colors": colors})
        );
    } else {
        println!("{what}: 256 colours (RRGGBB, index 0 is transparent in the client)");
        for (row, chunk) in colors.chunks(8).enumerate() {
            println!("  {:3}: {}", row * 8, chunk.join(" "));
        }
    }
    Ok(0)
}

pub fn extract(
    input: &Path,
    entry: Option<&str>,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (pal, _) = load_pal(input, entry, names)?;
    write_new(output, overwrite, &pal.to_bytes())?;
    report(json, "pal extract", output, json!({}));
    Ok(0)
}

pub fn png(
    input: &Path,
    entry: Option<&str>,
    cell: u16,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (pal, _) = load_pal(input, entry, names)?;
    write_new(output, overwrite, &rgba_to_png(&pal.swatch(cell))?)?;
    report(json, "pal png", output, json!({"cell": cell.clamp(1, 64)}));
    Ok(0)
}

pub fn from_png(input: &Path, output: &Path, overwrite: bool, json: bool) -> Result<i32> {
    let data = fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
    let pal = match frame_from_png(&data).with_context(|| input.display().to_string())? {
        Frame::Indexed { palette, .. } => Pal { colors: palette },
        Frame::Rgba(img) => Pal::from_swatch(&img)?,
    };
    write_new(output, overwrite, &pal.to_bytes())?;
    report(json, "pal from-png", output, json!({}));
    Ok(0)
}

/// `N=RRGGBB` with N from 0 to 255.
pub fn parse_set(s: &str) -> Result<(u8, [u8; 3]), String> {
    let (i, c) = s.split_once('=').ok_or("expected INDEX=RRGGBB")?;
    let i: u8 = i.trim().parse().map_err(|_| "the index is 0 to 255")?;
    let h = c.trim().trim_start_matches('#');
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("the colour is RRGGBB in hex".into());
    }
    let b = |k: usize| u8::from_str_radix(&h[k..k + 2], 16).unwrap_or(0);
    Ok((i, [b(0), b(2), b(4)]))
}

pub fn set(
    input: &Path,
    entry: Option<&str>,
    colors: &[(u8, [u8; 3])],
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (mut pal, _) = load_pal(input, entry, names)?;
    for &(i, c) in colors {
        pal.set_color(i, c);
    }
    write_new(output, overwrite, &pal.to_bytes())?;
    report(json, "pal set", output, json!({"colors": colors.len()}));
    Ok(0)
}

pub struct AdjustArgs<'a> {
    pub input: &'a Path,
    pub entry: Option<&'a str>,
    pub from: u8,
    pub to: u8,
    pub hue: f32,
    pub saturation: f32,
    pub brightness: f32,
    pub output: &'a Path,
    pub overwrite: bool,
    pub names: NameMode,
    pub json: bool,
}

pub fn adjust(a: &AdjustArgs<'_>) -> Result<i32> {
    if a.from > a.to {
        bail!("--from {} is above --to {}", a.from, a.to);
    }
    if a.saturation < 0.0 || a.brightness < 0.0 {
        bail!("--saturation and --brightness are factors of at least 0");
    }
    let (mut pal, _) = load_pal(a.input, a.entry, a.names)?;
    pal.adjust(a.from, a.to, a.hue, a.saturation, a.brightness);
    write_new(a.output, a.overwrite, &pal.to_bytes())?;
    report(
        a.json,
        "pal adjust",
        a.output,
        json!({"from": a.from, "to": a.to, "hue": a.hue,
            "saturation": a.saturation, "brightness": a.brightness}),
    );
    Ok(0)
}

pub struct SheetArgs<'a> {
    pub input: &'a Path,
    pub entry: Option<&'a str>,
    pub spr: Option<&'a Path>,
    pub pals: &'a [String],
    pub action: usize,
    pub frame: usize,
    pub columns: usize,
    pub scale: u32,
    pub output: &'a Path,
    pub overwrite: bool,
    pub names: NameMode,
    pub json: bool,
}

/// One frame of a sprite drawn with its own palette and then with each given palette, side by
/// side (the preview sheet of ACT Editor's palette tool). Palettes are loose files or, when the
/// ACT comes from a GRF, entries of it.
pub fn sheet(a: &SheetArgs<'_>) -> Result<i32> {
    if a.pals.is_empty() {
        bail!("give at least one --pal");
    }
    let (data, what) = load(a.input, a.entry, a.names)?;
    let act = rgrf_formats::act::Act::parse(&data)
        .with_context(|| format!("{what} is not a readable ACT"))?;
    let (sdata, swhat) = match (a.spr, a.entry) {
        (Some(p), _) => load(p, None, a.names)?,
        (None, Some(e)) => {
            let n = e
                .to_ascii_lowercase()
                .strip_suffix(".act")
                .map(|_| format!("{}spr", &e[..e.len() - 3]))
                .ok_or_else(|| anyhow!("'{e}' does not end in .act"))?;
            load(a.input, Some(&n), a.names)?
        }
        (None, None) => {
            let p = a.input.with_extension("spr");
            if !p.exists() {
                bail!("{} does not exist; pass the sprite with --spr", p.display());
            }
            load(&p, None, a.names)?
        }
    };
    let base = Spr::parse(&sdata).with_context(|| format!("{swhat} is not a readable SPR"))?;
    if base.indexed.is_empty() {
        bail!("{swhat} has no palette images, a palette would change nothing");
    }
    let mut sprites = vec![base.clone()];
    for p in a.pals {
        // a loose file wins; otherwise the name is an entry of the same GRF
        let (pal, _) = if a.entry.is_some() && !Path::new(p).is_file() {
            load_pal(a.input, Some(p), a.names)?
        } else {
            load_pal(Path::new(p), None, a.names)?
        };
        let mut s = base.clone();
        s.set_palette(&pal);
        sprites.push(s);
    }
    let first = [Part::new(&act, &sprites[0])];
    let n_frames = act.actions.get(a.action).map_or(0, |x| x.frames.len());
    if a.frame >= n_frames {
        bail!(
            "{what} has no frame {} in action {} ({n_frames} frames)",
            a.frame,
            a.action
        );
    }
    let Some(bounds) = bounds_of_parts(&first, a.action, 0..n_frames)? else {
        bail!("action {} draws nothing", a.action);
    };
    let s = a.scale.max(1) as usize;
    let (cw, ch) = (bounds.width() as usize * s, bounds.height() as usize * s);
    let gap = 4;
    let cols = a.columns.clamp(1, sprites.len());
    let rows = sprites.len().div_ceil(cols);
    let (w, h) = (cols * cw + (cols + 1) * gap, rows * ch + (rows + 1) * gap);
    let (Ok(width), Ok(height)) = (u16::try_from(w), u16::try_from(h)) else {
        bail!(
            "the sheet would be {w} x {h} pixels, too large (use fewer palettes, --columns or --scale)"
        );
    };
    let mut pixels = vec![0u8; w * h * 4];
    for (k, spr) in sprites.iter().enumerate() {
        let c = render_parts(&[Part::new(&act, spr)], a.action, a.frame, bounds)?;
        let px = if s == 1 {
            c.pixels
        } else {
            upscale(&c.pixels, c.width as usize, c.height as usize, s)
        };
        let (x0, y0) = (gap + (k % cols) * (cw + gap), gap + (k / cols) * (ch + gap));
        for y in 0..ch {
            let dst = ((y0 + y) * w + x0) * 4;
            pixels[dst..dst + cw * 4].copy_from_slice(&px[y * cw * 4..(y + 1) * cw * 4]);
        }
    }
    let png = rgba_to_png(&RgbaImage {
        width,
        height,
        pixels,
    })?;
    write_new(a.output, a.overwrite, &png)?;
    report(
        a.json,
        "pal sheet",
        a.output,
        json!({"cells": sprites.len(), "columns": cols, "width": w, "height": h}),
    );
    Ok(0)
}
