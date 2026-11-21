//! `rgrf rare`: client files that are seldom needed: BSON (as JSON), EBM emblems, icons,
//! Thumbs.db, IMF and FNA.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::ico::IcoPicture;
use rgrf_formats::png_io::rgba_to_png;
use rgrf_formats::{bson, ebm, fna, ico, imf, thumbsdb};
use serde_json::json;

use crate::spr_cmd::load;

fn write_new(path: &Path, data: &[u8], overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        bail!("{} exists (use --overwrite)", path.display());
    }
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, data).with_context(|| format!("cannot write {}", path.display()))
}

pub struct Io<'a> {
    pub input: &'a Path,
    pub entry: Option<&'a str>,
    pub output: Option<&'a Path>,
    pub overwrite: bool,
    pub names: NameMode,
    pub json: bool,
}

pub fn io<'a>(
    input: &'a Path,
    entry: &'a Option<String>,
    output: &'a Option<PathBuf>,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Io<'a> {
    Io {
        input,
        entry: entry.as_deref(),
        output: output.as_deref(),
        overwrite,
        names,
        json,
    }
}

/// Text to the output file, or to the screen.
fn emit(io: &Io, text: &str) -> Result<()> {
    match io.output {
        Some(p) => write_new(p, text.as_bytes(), io.overwrite),
        None => {
            print!("{text}");
            Ok(())
        }
    }
}

pub fn bson_to_json(io: &Io) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let doc = bson::parse(&data).with_context(|| format!("{what} is not a readable BSON file"))?;
    emit(io, &bson::to_json(&doc))?;
    Ok(0)
}

pub fn json_to_bson(input: &Path, output: &Path, overwrite: bool) -> Result<i32> {
    let text = std::fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;
    let doc = bson::from_json(&text)
        .with_context(|| format!("{} is not usable JSON", input.display()))?;
    write_new(output, &bson::to_bytes(&doc), overwrite)?;
    println!("Wrote {}", output.display());
    Ok(0)
}

pub fn ebm_to_bmp(io: &Io) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let bmp = ebm::to_bmp(&data).with_context(|| format!("{what} is not an emblem"))?;
    let Some(out) = io.output else {
        bail!("--output is needed")
    };
    write_new(out, &bmp, io.overwrite)?;
    println!("Wrote {}", out.display());
    Ok(0)
}

pub fn bmp_to_ebm(input: &Path, output: &Path, overwrite: bool) -> Result<i32> {
    let bmp = std::fs::read(input).with_context(|| format!("cannot read {}", input.display()))?;
    if !bmp.starts_with(b"BM") {
        bail!("{} is not a BMP", input.display());
    }
    write_new(output, &ebm::from_bmp(&bmp), overwrite)?;
    println!("Wrote {}", output.display());
    Ok(0)
}

/// `ico`: the pictures of the file; with `dir` each written as a PNG.
pub fn ico_cmd(io: &Io, dir: Option<&Path>) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let pics = ico::parse(&data).with_context(|| format!("{what} is not an icon"))?;
    let mut written = Vec::new();
    for (i, p) in pics.iter().enumerate() {
        if let Some(dir) = dir {
            let png = match &p.picture {
                IcoPicture::Png(b) => b.clone(),
                IcoPicture::Rgba(img) => rgba_to_png(img)?,
            };
            let path = dir.join(format!("{i}_{}x{}.png", p.width, p.height));
            write_new(&path, &png, io.overwrite)?;
            written.push(path.display().to_string());
        }
    }
    if io.json {
        let items: Vec<_> = pics
            .iter()
            .map(|p| json!({"width": p.width, "height": p.height, "bits": p.bits, "png": matches!(p.picture, IcoPicture::Png(_))}))
            .collect();
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "pictures": items, "written": written})
        );
    } else {
        for (i, p) in pics.iter().enumerate() {
            println!(
                "{i}: {} x {}, {} bit{}",
                p.width,
                p.height,
                p.bits,
                if matches!(p.picture, IcoPicture::Png(_)) {
                    " (PNG)"
                } else {
                    ""
                }
            );
        }
        if !written.is_empty() {
            println!("{} pictures written", written.len());
        }
    }
    Ok(0)
}

/// `thumbs`: the pictures of a Thumbs.db; with `dir` the JPEGs are written there.
pub fn thumbs_cmd(io: &Io, dir: Option<&Path>) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let db =
        thumbsdb::parse(&data).with_context(|| format!("{what} is not a readable Thumbs.db"))?;
    let mut written = 0;
    for t in &db.thumbs {
        if let (Some(dir), Some(j)) = (dir, &t.jpeg) {
            let name = t.name.rsplit(['\\', '/']).next().unwrap_or(&t.name);
            let safe: String = name
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || "._- ".contains(c) {
                        c
                    } else {
                        '_'
                    }
                })
                .collect();
            write_new(&dir.join(format!("{}_{safe}.jpg", t.id)), j, io.overwrite)?;
            written += 1;
        }
    }
    if io.json {
        let items: Vec<_> = db
            .thumbs
            .iter()
            .map(|t| json!({"id": t.id, "name": t.name, "jpeg_bytes": t.jpeg.as_ref().map(Vec::len)}))
            .collect();
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "width": db.width, "height": db.height, "thumbnails": items, "written": written})
        );
    } else {
        println!(
            "{} thumbnails ({} x {})",
            db.thumbs.len(),
            db.width,
            db.height
        );
        for t in &db.thumbs {
            println!(
                "{:>6}  {}{}",
                t.id,
                t.name,
                if t.jpeg.is_none() {
                    "  (not readable)"
                } else {
                    ""
                }
            );
        }
        if dir.is_some() {
            println!("{written} JPEG files written");
        }
    }
    Ok(0)
}

pub fn imf_cmd(io: &Io) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let f = imf::Imf::parse(&data).with_context(|| format!("{what} is not an IMF file"))?;
    if io.json {
        let layers: Vec<_> = f
            .layers
            .iter()
            .map(|l| {
                l.iter()
                    .map(|a| {
                        a.iter()
                            .map(|x| json!([x.priority, x.x, x.y]))
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "checksum": f.checksum, "layers": layers})
        );
    } else {
        print!("{}", f.to_text());
    }
    Ok(0)
}

pub fn fna_cmd(io: &Io) -> Result<i32> {
    let (data, what) = load(io.input, io.entry, io.names)?;
    let f = fna::Fna::parse(&data).with_context(|| format!("{what} is not an FNA file"))?;
    let names: Vec<String> = f
        .files
        .iter()
        .map(|n| GrfPath::new(n.clone()).display(io.names))
        .collect();
    if io.json {
        println!(
            "{}",
            json!({"schema_version": crate::SCHEMA_VERSION, "source": what, "files": names})
        );
    } else {
        for n in &names {
            println!("{n}");
        }
    }
    Ok(0)
}

/// Where `--output` of the commands that write a folder points to.
pub fn dir_of(p: &Option<PathBuf>) -> Option<&Path> {
    p.as_deref()
}
