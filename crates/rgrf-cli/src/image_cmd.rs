//! `rgrf image`: look at BMP/TGA/PNG pictures, convert them, and shrink the true-colour BMPs of an
//! archive to 8 bits (M7d).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rgrf_core::edit::{Edit, apply};
use rgrf_core::encode::{Method, encode};
use rgrf_core::progress::Control;
use rgrf_core::repack::RepackOptions;
use rgrf_core::{GrfPath, NameMode};
use rgrf_formats::bmp::{self, Stored};
use rgrf_formats::png_io::{Frame, frame_from_png, rgba_to_png};
use rgrf_formats::quantize::is_key;
use rgrf_formats::spr::RgbaImage;
use serde_json::json;

use crate::spr_cmd::load;

/// A picture in any of the formats the client uses (JPEG is not decoded).
fn decode(data: &[u8], what: &str) -> Result<(&'static str, RgbaImage)> {
    if data.starts_with(b"BM") {
        return Ok((
            "bmp",
            bmp::parse(data).with_context(|| format!("{what} is not a readable BMP"))?,
        ));
    }
    if data.starts_with(b"\x89PNG") {
        let frame =
            frame_from_png(data).with_context(|| format!("{what} is not a readable PNG"))?;
        return Ok((
            "png",
            match frame {
                Frame::Rgba(img) => img,
                Frame::Indexed { image, palette } => RgbaImage {
                    width: image.width,
                    height: image.height,
                    pixels: image
                        .pixels
                        .iter()
                        .flat_map(|&i| {
                            let c = &palette[4 * usize::from(i)..4 * usize::from(i) + 3];
                            [c[0], c[1], c[2], 255]
                        })
                        .collect(),
                },
            },
        ));
    }
    if data.starts_with(&[0xff, 0xd8]) {
        bail!("{what} is a JPEG; reading JPEG is not supported");
    }
    if what.to_ascii_lowercase().ends_with(".tga") {
        return Ok((
            "tga",
            rgrf_formats::tga::parse(data)
                .with_context(|| format!("{what} is not a readable TGA"))?,
        ));
    }
    bail!("{what} is neither BMP, PNG nor TGA")
}

pub fn info(input: &Path, entry: Option<&str>, names: NameMode, json: bool) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let (format, img) = decode(&data, &what)?;
    let bits = if format == "bmp" {
        bmp::info(&data).ok().map(|i| i.bits)
    } else {
        None
    };
    let px = img.pixels.as_chunks::<4>().0;
    let mut colors: Vec<[u8; 4]> = px.to_vec();
    colors.sort_unstable();
    colors.dedup();
    let key = px.iter().filter(|p| is_key([p[0], p[1], p[2]])).count();
    let alpha = px.iter().filter(|p| p[3] != 255).count();
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "format": format,
                "bytes": data.len(),
                "width": img.width,
                "height": img.height,
                "bits": bits,
                "colors": colors.len(),
                "transparent_key_pixels": key,
                "alpha_pixels": alpha,
            })
        );
    } else {
        println!("{what}");
        let bits = bits.map(|b| format!(", {b} bits")).unwrap_or_default();
        println!(
            "  {format}{bits}, {} x {}, {} bytes",
            img.width,
            img.height,
            data.len()
        );
        println!(
            "  {} colours, {key} magenta (transparent) pixels, {alpha} pixels with alpha",
            colors.len()
        );
    }
    Ok(0)
}

pub struct ConvertArgs<'a> {
    pub output: &'a Path,
    pub bits: Option<u16>,
    pub overwrite: bool,
}

/// Writes the picture as PNG or BMP, by the extension of the output. BMP is 24-bit unless
/// `bits` is 8 (then reduced to 256 colours; the PSNR is printed).
pub fn convert(
    input: &Path,
    entry: Option<&str>,
    names: NameMode,
    args: &ConvertArgs,
    json: bool,
) -> Result<i32> {
    let (data, what) = load(input, entry, names)?;
    let (_, img) = decode(&data, &what)?;
    if args.output.exists() && !args.overwrite {
        bail!("{} exists (use --overwrite)", args.output.display());
    }
    let ext = args
        .output
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mut psnr = None;
    let out = match (ext.as_str(), args.bits) {
        ("png", None) => rgba_to_png(&img)?,
        ("png", Some(_)) => bail!("--bits is for BMP output"),
        ("bmp", None | Some(24)) => bmp::write_rgb(&img),
        ("bmp", Some(8)) => {
            let (b, p, _) = bmp::write_reduced(&img, 256)?;
            psnr = Some(p);
            b
        }
        ("bmp", Some(b)) => bail!("--bits {b}: BMP output is 8 or 24 bits"),
        _ => bail!("write .png or .bmp (the format follows the extension of the output)"),
    };
    fs::write(args.output, &out)
        .with_context(|| format!("cannot write {}", args.output.display()))?;
    let psnr_text = psnr.map(fmt_psnr);
    if json {
        println!(
            "{}",
            json!({
                "schema_version": crate::SCHEMA_VERSION,
                "source": what,
                "output": args.output.display().to_string(),
                "bytes": out.len(),
                "psnr_db": psnr.filter(|p| p.is_finite()),
                "lossless": psnr.is_none_or(f64::is_infinite),
            })
        );
    } else {
        println!(
            "{} -> {} ({} bytes)",
            what,
            args.output.display(),
            out.len()
        );
        if let Some(p) = psnr_text {
            println!("  reduced to 256 colours: {p}");
        }
    }
    Ok(0)
}

fn fmt_psnr(p: f64) -> String {
    if p.is_finite() {
        format!("PSNR {p:.1} dB")
    } else {
        "lossless".into()
    }
}

pub struct ReduceDirArgs<'a> {
    pub only: &'a str,
    pub min_psnr: f64,
    /// Compression the size decision is made with (the one the archive will use).
    pub method: Method,
    pub report: Option<&'a Path>,
    pub overwrite: bool,
    /// Effort of the lossless PNG pass (Zopfli iterations); `None` leaves the PNGs alone.
    pub png: Option<u64>,
    /// The `jpegtran` program for the lossless JPEG pass; `None` leaves the JPEGs alone.
    pub jpegtran: Option<&'a Path>,
}

/// What became of one file of the source folder.
enum Dealt {
    Reduced {
        rel: String,
        psnr: f64,
        before: u64,
        after: u64,
    },
    Kept(&'static str),
    Copied,
    /// A PNG or JPEG with the same picture and fewer bytes (the file sizes).
    Lossless {
        png: bool,
        before: u64,
        after: u64,
    },
}

/// A JPEG with optimal Huffman tables and no metadata, through `jpegtran`: the same picture,
/// since only the coding of the coefficients changes. Baseline stays baseline (the client's
/// loader is not known to read progressive files).
pub fn jpegtran_optimize(program: &Path, data: &[u8]) -> Result<Option<Vec<u8>>> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new(program)
        .args(["-copy", "none", "-optimize"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("cannot run {}", program.display()))?;
    let mut stdin = child.stdin.take().context("no stdin")?;
    let input = data.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&input));
    let out = child.wait_with_output()?;
    let _ = writer.join();
    let ok = out.status.success()
        && out.stdout.len() < data.len()
        && out.stdout.starts_with(&[0xff, 0xd8])
        && out.stdout.ends_with(&[0xff, 0xd9]);
    Ok(ok.then_some(out.stdout))
}

fn walk_files(
    dir: &Path,
    rel: &Path,
    files: &mut Vec<std::path::PathBuf>,
    dirs: &mut Vec<std::path::PathBuf>,
) -> Result<()> {
    for e in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let e = e?;
        let t = e.file_type()?;
        let r = rel.join(e.file_name());
        if t.is_dir() {
            dirs.push(r.clone());
            walk_files(&e.path(), &r, files, dirs)?;
        } else if t.is_file() {
            files.push(r);
        }
    }
    Ok(())
}

/// Writes `dst` as a complete copy of the folder `src` in which every matching true-colour BMP is
/// an 8-bit one where that keeps `min_psnr` dB and the file, compressed with `method`, gets
/// smaller (`reduce` for folders: the optimised base to build archives from). `src` is only read.
pub fn reduce_dir(src: &Path, dst: &Path, args: &ReduceDirArgs, json: bool) -> Result<i32> {
    let started = Instant::now();
    if !src.is_dir() {
        bail!("{} is not a folder", src.display());
    }
    let src_abs = src.canonicalize()?;
    // the output may not exist yet, nor its parents: resolve the nearest folder that does
    let mut tail = Vec::new();
    let mut base = dst.to_path_buf();
    let mut dst_abs = loop {
        match base.canonicalize() {
            Ok(p) => break p,
            Err(_) => {
                let name = base
                    .file_name()
                    .context("not a usable output path")?
                    .to_owned();
                tail.push(name);
                base = match base.parent() {
                    Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
                    _ => std::env::current_dir()?,
                };
            }
        }
    };
    dst_abs.extend(tail.iter().rev());
    if dst_abs.starts_with(&src_abs) || src_abs.starts_with(&dst_abs) {
        bail!("the output and the source folder must not contain each other");
    }
    if dst.exists() && !args.overwrite && fs::read_dir(dst)?.next().is_some() {
        bail!("{} is not empty (use --overwrite)", dst.display());
    }
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk_files(src, Path::new(""), &mut files, &mut dirs)?;
    fs::create_dir_all(dst)?;
    for d in &dirs {
        fs::create_dir_all(dst.join(d))?;
    }
    let method = args.method;
    let results: Vec<Result<Dealt>> = files
        .par_iter()
        .map(|rel| {
            let from = src.join(rel);
            let to = dst.join(rel);
            let shown = rel.to_string_lossy().into_owned();
            let copy = |kept: Option<&'static str>| -> Result<Dealt> {
                fs::copy(&from, &to).with_context(|| format!("cannot copy {shown}"))?;
                Ok(kept.map_or(Dealt::Copied, Dealt::Kept))
            };
            let lower = shown.to_ascii_lowercase();
            if let (Some(iterations), true) = (args.png, lower.ends_with(".png")) {
                let data = fs::read(&from).with_context(|| format!("cannot read {shown}"))?;
                let deflate = |d: &[u8]| encode(Method::Zopfli { iterations }, d);
                if let rgrf_formats::png_opt::Outcome::Smaller(small) =
                    rgrf_formats::png_opt::optimize(&data, &deflate)
                {
                    fs::write(&to, &small).with_context(|| format!("cannot write {shown}"))?;
                    return Ok(Dealt::Lossless {
                        png: true,
                        before: data.len() as u64,
                        after: small.len() as u64,
                    });
                }
                fs::write(&to, &data).with_context(|| format!("cannot write {shown}"))?;
                return Ok(Dealt::Copied);
            }
            if let (Some(tool), true) = (
                args.jpegtran,
                lower.ends_with(".jpg") || lower.ends_with(".jpeg"),
            ) {
                let data = fs::read(&from).with_context(|| format!("cannot read {shown}"))?;
                if let Some(small) = jpegtran_optimize(tool, &data)? {
                    fs::write(&to, &small).with_context(|| format!("cannot write {shown}"))?;
                    return Ok(Dealt::Lossless {
                        png: false,
                        before: data.len() as u64,
                        after: small.len() as u64,
                    });
                }
                fs::write(&to, &data).with_context(|| format!("cannot write {shown}"))?;
                return Ok(Dealt::Copied);
            }
            if !rgrf_core::name::glob_match(args.only, &shown) {
                return copy(None);
            }
            let data = fs::read(&from).with_context(|| format!("cannot read {shown}"))?;
            match bmp::reduce(&data, args.min_psnr) {
                bmp::Reduced::Kept(k) => copy(Some(k.reason())),
                bmp::Reduced::Done { bytes, psnr, .. } => {
                    let before = encode(method, &data).len() as u64;
                    let after = encode(method, &bytes).len() as u64;
                    if after >= before {
                        return copy(Some(bmp::Keep::NotSmaller.reason()));
                    }
                    fs::write(&to, &bytes).with_context(|| format!("cannot write {shown}"))?;
                    Ok(Dealt::Reduced {
                        rel: shown,
                        psnr,
                        before,
                        after,
                    })
                }
            }
        })
        .collect();
    let mut kept: BTreeMap<&str, usize> = BTreeMap::new();
    let mut rows = Vec::new();
    // PNG and JPEG passes: files, bytes before, bytes after
    let (mut png_pass, mut jpeg_pass) = ([0u64; 3], [0u64; 3]);
    let (mut before, mut after, mut worst, mut lossless, mut copied) =
        (0u64, 0u64, f64::INFINITY, 0usize, 0usize);
    for r in results {
        match r? {
            Dealt::Reduced {
                rel,
                psnr,
                before: b,
                after: a,
            } => {
                before += b;
                after += a;
                worst = worst.min(psnr);
                lossless += usize::from(psnr.is_infinite());
                rows.push((rel, psnr, b, a));
            }
            Dealt::Kept(why) => *kept.entry(why).or_default() += 1,
            Dealt::Copied => copied += 1,
            Dealt::Lossless {
                png,
                before: b,
                after: a,
            } => {
                let pass = if png { &mut png_pass } else { &mut jpeg_pass };
                pass[0] += 1;
                pass[1] += b;
                pass[2] += a;
            }
        }
    }
    if let Some(path) = args.report {
        rows.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut csv = String::from("path,psnr_db,compressed_before,compressed_after\n");
        for (name, psnr, b, a) in &rows {
            let psnr = if psnr.is_finite() {
                format!("{psnr:.2}")
            } else {
                "lossless".into()
            };
            csv.push_str(&format!(
                "\"{}\",{psnr},{b},{a}\n",
                name.replace('"', "\"\"")
            ));
        }
        fs::write(path, csv).with_context(|| format!("cannot write {}", path.display()))?;
    }
    let secs = started.elapsed().as_secs_f64();
    let reduced = rows.len();
    if json {
        crate::print_json(&json!({
            "schema_version": crate::SCHEMA_VERSION,
            "files": files.len(),
            "reduced": reduced,
            "lossless": lossless,
            "kept": kept,
            "copied": copied + kept.values().sum::<usize>(),
            "compressed_bytes_before": before,
            "compressed_bytes_after": after,
            "png_files": png_pass[0], "png_bytes_before": png_pass[1], "png_bytes_after": png_pass[2],
            "jpeg_files": jpeg_pass[0], "jpeg_bytes_before": jpeg_pass[1], "jpeg_bytes_after": jpeg_pass[2],
            "worst_psnr_db": (reduced > 0 && worst.is_finite()).then_some(worst),
            "min_psnr_db": args.min_psnr,
            "seconds": secs,
        }))?;
    } else {
        let mib = |b: u64| b as f64 / f64::from(1u32 << 20);
        println!(
            "{} files, {} folders -> {}",
            files.len(),
            dirs.len(),
            dst.display()
        );
        println!(
            "Reduced:  {reduced} to 8 bits ({lossless} lossless), worst {}",
            if reduced > 0 {
                fmt_psnr(worst)
            } else {
                "-".into()
            }
        );
        println!(
            "Compressed: {:.1} -> {:.1} MiB (saves {:.1} MiB)",
            mib(before),
            mib(after),
            mib(before - after)
        );
        for (name, pass) in [("PNG", png_pass), ("JPEG", jpeg_pass)] {
            if pass[0] > 0 {
                println!(
                    "{name}:      {} files made smaller without changing the picture: {:.1} -> {:.1} MiB (saves {:.1} MiB)",
                    pass[0],
                    mib(pass[1]),
                    mib(pass[2]),
                    mib(pass[1] - pass[2])
                );
            }
        }
        let kept: Vec<String> = kept
            .iter()
            .map(|(k, n)| format!("{n} {}", k.replace('_', " ")))
            .collect();
        if !kept.is_empty() {
            println!("Kept:     {}", kept.join(", "));
        }
        println!("Took {secs:.1} s");
    }
    Ok(0)
}

pub struct ReduceArgs<'a> {
    pub only: &'a str,
    pub min_psnr: f64,
    pub report: Option<&'a Path>,
}

/// Writes `output` as `grf` with every matching true-colour BMP stored as 8-bit where that keeps
/// at least `min_psnr` dB and the stored (compressed) file gets smaller.
pub fn reduce(
    grf: &Path,
    output: &Path,
    args: &ReduceArgs,
    opts: &RepackOptions,
    names: NameMode,
    json: bool,
    ctl: &Control,
) -> Result<i32> {
    let started = Instant::now();
    if !opts.dry_run && output.exists() && !opts.overwrite {
        bail!("{} exists (use --overwrite)", output.display());
    }
    let a = crate::crypt_cmd::open_archive(grf)?;
    let idx = a.select(Some(args.only), names);
    let method = opts.method;
    let results: Vec<Result<(GrfPath, Stored), &'static str>> = idx
        .par_iter()
        .map(|&i| {
            let e = &a.entries()[i];
            let d = a.read(i).map_err(|_| "unreadable")?;
            let r = bmp::reduce_for_storage(&d.data, args.min_psnr, u64::from(e.zsize), |b| {
                encode(method, b).len() as u64
            });
            r.map(|s| (e.name.clone(), s)).map_err(|k| k.reason())
        })
        .collect();
    let mut kept: BTreeMap<&str, usize> = BTreeMap::new();
    let mut edits = Vec::new();
    // path, PSNR, stored size before and after (for --report)
    let mut rows: Vec<(GrfPath, f64, u64, u64)> = Vec::new();
    let (mut before, mut after, mut worst, mut lossless) = (0u64, 0u64, f64::INFINITY, 0usize);
    for r in results {
        match r {
            Ok((name, st)) => {
                if args.report.is_some() {
                    rows.push((name.clone(), st.psnr, st.stored_before, st.stored_after));
                }
                before += st.stored_before;
                after += st.stored_after;
                worst = worst.min(st.psnr);
                lossless += usize::from(st.psnr.is_infinite());
                edits.push(Edit::Put {
                    name,
                    data: st.bytes,
                });
            }
            Err(why) => *kept.entry(why).or_default() += 1,
        }
    }
    if let Some(path) = args.report {
        rows.sort_by(|a, b| a.1.total_cmp(&b.1));
        let mut csv = String::from("path,psnr_db,stored_before,stored_after\n");
        for (name, psnr, before, after) in &rows {
            let psnr = if psnr.is_finite() {
                format!("{psnr:.2}")
            } else {
                "lossless".into()
            };
            let shown = name.display(names).replace('"', "\"\"");
            csv.push_str(&format!("\"{shown}\",{psnr},{before},{after}\n"));
        }
        fs::write(path, csv).with_context(|| format!("cannot write {}", path.display()))?;
    }
    let reduced = edits.len();
    let write = !opts.dry_run && reduced > 0;
    let report = if write {
        Some(apply(&a, grf, output, &edits, opts, ctl)?)
    } else {
        None
    };
    let secs = started.elapsed().as_secs_f64();
    if json {
        let mut v = match &report {
            Some(r) => crate::write_json(&r.merge.repack, output, secs),
            None => json!({}),
        };
        let extra = json!({
            "schema_version": crate::SCHEMA_VERSION,
            "matched": idx.len(),
            "reduced": reduced,
            "lossless": lossless,
            "kept": kept,
            "stored_bytes_before": before,
            "stored_bytes_after": after,
            "worst_psnr_db": (reduced > 0 && worst.is_finite()).then_some(worst),
            "min_psnr_db": args.min_psnr,
            "written": write,
            "seconds": secs,
        });
        for (k, x) in extra.as_object().into_iter().flatten() {
            v[k] = x.clone();
        }
        crate::print_json(&v)?;
    } else {
        let mib = |b: u64| b as f64 / f64::from(1u32 << 20);
        println!("{} entries match '{}'", idx.len(), args.only);
        println!(
            "Reduced:  {reduced} to 8 bits ({lossless} lossless), worst {}",
            if reduced > 0 {
                fmt_psnr(worst)
            } else {
                "-".into()
            }
        );
        println!(
            "Stored:   {:.1} -> {:.1} MiB (saves {:.1} MiB)",
            mib(before),
            mib(after),
            mib(before - after)
        );
        let kept: Vec<String> = kept
            .iter()
            .map(|(k, n)| format!("{n} {}", k.replace('_', " ")))
            .collect();
        if !kept.is_empty() {
            println!("Kept:     {}", kept.join(", "));
        }
        if let Some(r) = &report {
            crate::write_summary(&r.merge.repack, output, secs);
        } else if reduced > 0 {
            println!("Dry run:  nothing written");
        } else {
            println!("Nothing to reduce, nothing written");
        }
    }
    Ok(0)
}
