//! `rgrf spr`: look at SPR sprites, export their frames as PNG and build an SPR from PNGs (M7a).

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use rgrf_core::{Archive, DecodeStatus, GrfPath, NameMode};
use rgrf_formats::png_io::{Frame, frame_from_png, indexed_to_png, rgba_to_png};
use rgrf_formats::spr::{IndexedImage, PALETTE_LEN, RgbaImage, Spr, Version};
use serde_json::json;

/// Reads the SPR bytes from a loose file, or from `entry` inside the GRF `input`.
pub(crate) fn load(
    input: &Path,
    entry: Option<&str>,
    names: NameMode,
) -> Result<(Vec<u8>, String)> {
    let Some(entry) = entry else {
        let data = fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
        return Ok((data, input.display().to_string()));
    };
    let a = Archive::open(input)?;
    let wanted = GrfPath::from_display(entry, names)
        .ok_or_else(|| anyhow!("'{entry}' cannot be written in the name mode {names:?}"))?;
    let Some(i) = a.find(&wanted) else {
        bail!("no entry named '{entry}' (names are compared case-insensitively, / equals \\)");
    };
    let d = a.read(i)?;
    if d.status == DecodeStatus::Encrypted {
        bail!("entry is encrypted; decryption is not supported yet");
    }
    Ok((d.data, entry.to_owned()))
}

fn parse(data: &[u8], what: &str) -> Result<Spr> {
    Spr::parse(data).with_context(|| format!("{what} is not a readable SPR"))
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let spr = parse(&data, &what)?;
    if json {
        let frames: Vec<_> = spr
            .indexed
            .iter()
            .map(|i| json!({"kind": "indexed", "width": i.width, "height": i.height}))
            .chain(
                spr.rgba
                    .iter()
                    .map(|i| json!({"kind": "rgba", "width": i.width, "height": i.height})),
            )
            .collect();
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "bytes": data.len(),
                "version": spr.version.to_string(),
                "indexed_images": spr.indexed.len(),
                "rgba_images": spr.rgba.len(),
                "has_palette": spr.palette.is_some(),
                "frames": frames,
                "warnings": spr.warnings,
            })
        );
    } else {
        println!("{what}");
        println!(
            "  version {}, {} bytes, {} indexed + {} true-colour images, palette: {}",
            spr.version,
            data.len(),
            spr.indexed.len(),
            spr.rgba.len(),
            if spr.palette.is_some() { "yes" } else { "no" }
        );
        for (i, f) in spr.indexed.iter().enumerate() {
            println!("  {i:>3}  indexed  {} x {}", f.width, f.height);
        }
        for (i, f) in spr.rgba.iter().enumerate() {
            println!(
                "  {:>3}  rgba     {} x {}",
                spr.indexed.len() + i,
                f.width,
                f.height
            );
        }
        for w in &spr.warnings {
            println!("  warning: {w}");
        }
    }
    Ok(if spr.warnings.is_empty() { 0 } else { 2 })
}

pub fn export(
    input: &Path,
    entry: Option<&str>,
    dest: &Path,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let spr = parse(&data, &what)?;
    let total = spr.indexed.len() + spr.rgba.len();
    let width = total.to_string().len().max(3);
    let file = |i: usize| dest.join(format!("{i:0width$}.png"));
    if !overwrite {
        for i in 0..total {
            if file(i).exists() {
                bail!("{} exists already (use --overwrite)", file(i).display());
            }
        }
    }
    fs::create_dir_all(dest).with_context(|| format!("cannot create {}", dest.display()))?;
    let mut written = Vec::with_capacity(total);
    if let Some(pal) = &spr.palette {
        for (i, img) in spr.indexed.iter().enumerate() {
            fs::write(file(i), indexed_to_png(img, pal)?)?;
            written.push(file(i));
        }
    }
    for (i, img) in spr.rgba.iter().enumerate() {
        let n = spr.indexed.len() + i;
        fs::write(file(n), rgba_to_png(img)?)?;
        written.push(file(n));
    }
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "frames": written.len(),
                "files": written.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            })
        );
    } else {
        println!(
            "{what}: {} frames written to {}",
            written.len(),
            dest.display()
        );
    }
    Ok(if spr.warnings.is_empty() { 0 } else { 2 })
}

/// Builds an SPR from the PNGs in `dir` (sorted by name). 8-bit indexed PNGs become palette images
/// (all must share one palette), everything else true-colour images; palette images come first.
pub fn build(
    dir: &Path,
    output: &Path,
    version: Version,
    overwrite: bool,
    json: bool,
) -> Result<i32> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .collect();
    files.sort();
    if files.is_empty() {
        bail!("no .png files in {}", dir.display());
    }
    if output.exists() && !overwrite {
        bail!("{} exists already (use --overwrite)", output.display());
    }
    let mut indexed: Vec<IndexedImage> = Vec::new();
    let mut rgba: Vec<RgbaImage> = Vec::new();
    let mut palette: Option<Box<[u8; PALETTE_LEN]>> = None;
    for f in &files {
        let bytes = fs::read(f).with_context(|| format!("cannot read {}", f.display()))?;
        match frame_from_png(&bytes).with_context(|| f.display().to_string())? {
            Frame::Indexed { image, palette: p } => {
                match &palette {
                    None => palette = Some(p),
                    Some(first) if first[..] != p[..] => bail!(
                        "{} uses a different palette than the first indexed PNG; all palette images of one SPR share one palette",
                        f.display()
                    ),
                    Some(_) => {}
                }
                indexed.push(image);
            }
            Frame::Rgba(img) => rgba.push(img),
        }
    }
    let spr = Spr {
        version,
        indexed,
        rgba,
        palette,
        warnings: Vec::new(),
    };
    let bytes = spr.to_bytes(version)?;
    let tmp = output.with_extension("spr.tmp");
    fs::write(&tmp, &bytes).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, output).with_context(|| format!("cannot write {}", output.display()))?;
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "output": output.display().to_string(),
                "version": version.to_string(),
                "indexed_images": spr.indexed.len(),
                "rgba_images": spr.rgba.len(),
                "bytes": bytes.len(),
            })
        );
    } else {
        println!(
            "{}: version {version}, {} indexed + {} true-colour images, {} bytes",
            output.display(),
            spr.indexed.len(),
            spr.rgba.len(),
            bytes.len()
        );
    }
    Ok(0)
}
