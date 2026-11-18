//! `rgrf spr replace|add|remove|set-palette` and `rgrf act translate|interval|copy-action|remove-action`.
//!
//! Every command reads a loose file (or a GRF entry) and writes a **new** loose file; it never
//! changes its input. Put the result back into a GRF with `rgrf build`, `merge` or `patch`.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use clap::ValueEnum;
use rgrf_core::NameMode;
use rgrf_formats::act::Act;
use rgrf_formats::edit::{Kind, quantize};
use rgrf_formats::pal::Pal;
use rgrf_formats::png_io::{Frame, frame_from_png};
use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr};
use serde_json::json;

use crate::spr_cmd::load;

#[derive(Clone, Copy, ValueEnum)]
pub enum KindArg {
    Indexed,
    Rgba,
}

impl From<KindArg> for Kind {
    fn from(k: KindArg) -> Self {
        match k {
            KindArg::Indexed => Self::Indexed,
            KindArg::Rgba => Self::Rgba,
        }
    }
}

fn write_new(output: &Path, overwrite: bool, bytes: &[u8]) -> Result<()> {
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    let tmp = output.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, output).with_context(|| format!("cannot write {}", output.display()))?;
    Ok(())
}

fn report(json: bool, what: &str, output: &Path, extra: serde_json::Value) {
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "operation": what,
                "output": output.display().to_string(),
                "details": extra,
            })
        );
    } else {
        println!("{what}: {} written", output.display());
    }
}

fn spr_from(input: &Path, entry: Option<&str>, names: NameMode) -> Result<Spr> {
    let (data, what) = load(input, entry, names)?;
    Spr::parse(&data).with_context(|| format!("{what} is not a readable SPR"))
}

fn act_from(input: &Path, entry: Option<&str>, names: NameMode) -> Result<Act> {
    let (data, what) = load(input, entry, names)?;
    Act::parse(&data).with_context(|| format!("{what} is not a readable ACT"))
}

/// A palette image read as true colour (index 0 transparent, the client's rule).
fn indexed_png_to_rgba(image: &IndexedImage, palette: &[u8; PALETTE_LEN]) -> RgbaImage {
    let mut pixels = Vec::with_capacity(image.pixels.len() * 4);
    for &i in &image.pixels {
        let i = usize::from(i);
        pixels.extend([
            palette[4 * i],
            palette[4 * i + 1],
            palette[4 * i + 2],
            if i == 0 { 0 } else { 255 },
        ]);
    }
    RgbaImage {
        width: image.width,
        height: image.height,
        pixels,
    }
}

enum NewImage {
    Indexed(IndexedImage),
    Rgba(RgbaImage),
}

/// Turns a PNG into an image of `kind` for `spr`.
fn image_from_png(png: &Path, kind: Kind, spr: &Spr, nearest: bool) -> Result<NewImage> {
    let bytes = fs::read(png).with_context(|| format!("cannot read {}", png.display()))?;
    let frame = frame_from_png(&bytes).with_context(|| png.display().to_string())?;
    match kind {
        Kind::Rgba => Ok(NewImage::Rgba(match frame {
            Frame::Rgba(i) => i,
            Frame::Indexed { image, palette } => indexed_png_to_rgba(&image, &palette),
        })),
        Kind::Indexed => {
            let pal = spr
                .palette
                .as_ref()
                .ok_or_else(|| anyhow!("the sprite has no palette, so it has no palette images"))?;
            match frame {
                Frame::Indexed { image, palette }
                    if (0..256).all(|i| palette[4 * i..4 * i + 3] == pal[4 * i..4 * i + 3]) =>
                {
                    Ok(NewImage::Indexed(image))
                }
                Frame::Indexed { image, palette } => {
                    if !nearest {
                        bail!(
                            "{} has another palette than the sprite; use --nearest to map its colours to the sprite's palette",
                            png.display()
                        );
                    }
                    Ok(NewImage::Indexed(quantize(
                        &indexed_png_to_rgba(&image, &palette),
                        pal,
                    )))
                }
                Frame::Rgba(img) => {
                    if !nearest {
                        bail!(
                            "{} is a true-colour PNG; use --nearest to map it to the sprite's palette (or --kind rgba)",
                            png.display()
                        );
                    }
                    Ok(NewImage::Indexed(quantize(&img, pal)))
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn spr_replace(
    input: &Path,
    entry: Option<&str>,
    kind: KindArg,
    frame: usize,
    png: &Path,
    nearest: bool,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let mut spr = spr_from(input, entry, names)?;
    match image_from_png(png, kind.into(), &spr, nearest)? {
        NewImage::Indexed(i) => spr.replace_indexed(frame, i)?,
        NewImage::Rgba(i) => spr.replace_rgba(frame, i)?,
    }
    write_new(output, overwrite, &spr.to_bytes(spr.version)?)?;
    report(
        json,
        "spr replace",
        output,
        serde_json::json!({"frame": frame}),
    );
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn spr_add(
    input: &Path,
    entry: Option<&str>,
    kind: KindArg,
    png: &Path,
    nearest: bool,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let mut spr = spr_from(input, entry, names)?;
    let index = match image_from_png(png, kind.into(), &spr, nearest)? {
        NewImage::Indexed(i) => spr.push_indexed(i)?,
        NewImage::Rgba(i) => spr.push_rgba(i),
    };
    write_new(output, overwrite, &spr.to_bytes(spr.version)?)?;
    if !json {
        println!("new image has index {index} (of its kind)");
    }
    report(json, "spr add", output, serde_json::json!({"index": index}));
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn spr_remove(
    input: &Path,
    entry: Option<&str>,
    kind: KindArg,
    frame: usize,
    act: Option<&Path>,
    act_output: Option<&Path>,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let mut spr = spr_from(input, entry, names)?;
    let mut act_data = match (act, act_output) {
        (Some(a), Some(_)) => Some(act_from(a, None, names)?),
        (None, None) => None,
        _ => bail!("--act and --act-output belong together"),
    };
    spr.remove_image(kind.into(), frame, act_data.as_mut())?;
    // Check both outputs before writing either.
    if let Some(p) = act_output
        && p.exists()
        && !overwrite
    {
        bail!("{} exists already (use --overwrite)", p.display());
    }
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    write_new(output, overwrite, &spr.to_bytes(spr.version)?)?;
    if let (Some(a), Some(p)) = (act_data, act_output) {
        write_new(p, overwrite, &a.to_bytes(a.version)?)?;
    }
    report(
        json,
        "spr remove",
        output,
        serde_json::json!({"frame": frame}),
    );
    Ok(0)
}

#[allow(clippy::too_many_arguments)]
pub fn spr_set_palette(
    input: &Path,
    entry: Option<&str>,
    pal: &Path,
    output: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let mut spr = spr_from(input, entry, names)?;
    if spr.indexed.is_empty() {
        bail!("the sprite has no palette images, a palette would change nothing");
    }
    let p = Pal::parse(&fs::read(pal).with_context(|| format!("cannot read {}", pal.display()))?)
        .with_context(|| pal.display().to_string())?;
    spr.set_palette(&p);
    write_new(output, overwrite, &spr.to_bytes(spr.version)?)?;
    report(json, "spr set-palette", output, serde_json::json!({}));
    Ok(0)
}

pub struct ActOut<'a> {
    pub output: &'a Path,
    pub overwrite: bool,
    pub json: bool,
}

fn save_act(act: &Act, what: &str, out: &ActOut<'_>, details: serde_json::Value) -> Result<i32> {
    write_new(out.output, out.overwrite, &act.to_bytes(act.version)?)?;
    report(out.json, what, out.output, details);
    Ok(0)
}

pub fn act_translate(
    input: &Path,
    entry: Option<&str>,
    action: Option<usize>,
    dx: f32,
    dy: f32,
    names: NameMode,
    out: &ActOut<'_>,
) -> Result<i32> {
    let mut act = act_from(input, entry, names)?;
    act.translate(action, dx, dy)?;
    save_act(
        &act,
        "act translate",
        out,
        serde_json::json!({"dx": dx, "dy": dy}),
    )
}

pub fn act_interval(
    input: &Path,
    entry: Option<&str>,
    action: usize,
    value: f32,
    names: NameMode,
    out: &ActOut<'_>,
) -> Result<i32> {
    let mut act = act_from(input, entry, names)?;
    act.set_interval(action, value)?;
    save_act(
        &act,
        "act interval",
        out,
        serde_json::json!({"action": action, "interval": value}),
    )
}

pub fn act_copy_action(
    input: &Path,
    entry: Option<&str>,
    from: usize,
    to: usize,
    names: NameMode,
    out: &ActOut<'_>,
) -> Result<i32> {
    let mut act = act_from(input, entry, names)?;
    act.copy_action(from, to)?;
    save_act(
        &act,
        "act copy-action",
        out,
        serde_json::json!({"from": from, "to": to}),
    )
}

pub fn act_remove_action(
    input: &Path,
    entry: Option<&str>,
    action: usize,
    names: NameMode,
    out: &ActOut<'_>,
) -> Result<i32> {
    let mut act = act_from(input, entry, names)?;
    act.remove_action(action)?;
    save_act(
        &act,
        "act remove-action",
        out,
        serde_json::json!({"action": action}),
    )
}
