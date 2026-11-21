//! `rgrf client slim`: a smaller copy of what is in a client folder besides the archives: the
//! music (MP3 at a lower bit rate), the skin pictures (24-bit BMP as 8-bit), PNG and JPEG without
//! a change of the picture, and without what a client does not read (file manager files, backups
//! of edited files, screenshots). The folder itself is never changed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use rayon::prelude::*;
use rgrf_core::encode::{Method, encode};
use rgrf_formats::bmp;
use rgrf_formats::lossy;
use serde_json::json;

pub struct SlimArgs<'a> {
    /// Re-encode MP3s above this bit rate (0.75 times it below 32 kHz); `None` leaves them.
    pub bgm_kbps: Option<u32>,
    /// Store 24-bit BMPs as 8 bits where the picture keeps this PSNR; `None` leaves them.
    pub min_psnr: Option<f64>,
    /// Zopfli iterations of the lossless PNG pass; `None` leaves PNGs.
    pub png: Option<u64>,
    pub jpegtran: Option<&'a Path>,
    /// Also copy the archives in the folder (default: they are made separately).
    pub keep_grf: bool,
    pub overwrite: bool,
    pub report: Option<&'a Path>,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    files: u64,
    before: u64,
    after: u64,
}

enum Dealt {
    Copied(u64),
    Left(&'static str),
    Skipped(&'static str, u64),
    Changed {
        kind: &'static str,
        before: u64,
        after: u64,
        note: String,
    },
}

fn is_junk(name: &str) -> Option<&'static str> {
    let l = name.to_ascii_lowercase();
    if l == ".ds_store" || l == "thumbs.db" || l == "desktop.ini" {
        return Some("file manager file");
    }
    let ext = l.rsplit('.').next().unwrap_or("");
    if ext == "bak" || (ext.starts_with("bak") && ext[3..].chars().all(|c| c.is_ascii_digit())) {
        return Some("backup of an edited file");
    }
    None
}

fn walk(dir: &Path, rel: &Path, files: &mut Vec<PathBuf>, dirs: &mut Vec<PathBuf>) -> Result<()> {
    for e in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let e = e?;
        let r = rel.join(e.file_name());
        if e.file_type()?.is_dir() {
            dirs.push(r.clone());
            walk(&e.path(), &r, files, dirs)?;
        } else if e.file_type()?.is_file() {
            files.push(r);
        }
    }
    Ok(())
}

/// MP3 at a lower rate; `Ok(None)` when the file is not worth it.
fn reencode_mp3(data: &[u8], kbps: u32) -> Result<Option<(Vec<u8>, String)>> {
    let d = lossy::decode(data, "mp3").map_err(|e| anyhow::anyhow!("{e}"))?;
    let frames = d.channels.first().map_or(0, Vec::len);
    if frames == 0 {
        return Ok(None);
    }
    let seconds = frames as f64 / f64::from(d.rate);
    let now = lossy::average_kbps(data.len(), seconds).unwrap_or(0);
    let target = if d.rate >= 32000 { kbps } else { kbps * 3 / 4 };
    if f64::from(now) < f64::from(target) * 1.2 {
        return Ok(None);
    }
    let out =
        lossy::encode_mp3(d.rate, &d.channels, Some(target)).map_err(|e| anyhow::anyhow!("{e}"))?;
    // the music must be as long as before and clearly smaller
    let check = lossy::decode(&out, "mp3").map_err(|e| anyhow::anyhow!("{e}"))?;
    let seconds2 = check.channels.first().map_or(0, Vec::len) as f64 / f64::from(check.rate);
    if (seconds2 - seconds).abs() > 0.25 || out.len() as f64 > data.len() as f64 * 0.9 {
        return Ok(None);
    }
    Ok(Some((
        out,
        format!(
            "{} Hz, {} ch, {now} -> {target} kbit/s",
            d.rate,
            d.channels.len()
        ),
    )))
}

pub fn slim(folder: &Path, output: &Path, args: &SlimArgs, json: bool) -> Result<i32> {
    let started = Instant::now();
    let src = folder
        .canonicalize()
        .with_context(|| format!("{} is not a folder", folder.display()))?;
    if !src.is_dir() {
        bail!("{} is not a folder", folder.display());
    }
    let wanted = std::env::current_dir()?.join(output);
    let mut tail = Vec::new();
    let mut base = wanted.as_path();
    while !base.exists() {
        tail.push(base.file_name().context("bad output path")?.to_owned());
        base = base.parent().context("bad output path")?;
    }
    let mut out_abs = base.canonicalize()?;
    out_abs.extend(tail.iter().rev());
    if out_abs.starts_with(&src) || src.starts_with(&out_abs) {
        bail!("the output and the client folder must not contain each other");
    }
    if output.exists() && !args.overwrite && fs::read_dir(output)?.next().is_some() {
        bail!("{} is not empty (use --overwrite)", output.display());
    }
    let (mut files, mut dirs) = (Vec::new(), Vec::new());
    walk(&src, Path::new(""), &mut files, &mut dirs)?;
    fs::create_dir_all(output)?;
    // screenshots and replays are the user's, not the client's: the folders stay, empty
    let user_dirs = ["screenshot", "replay"];
    let top = |p: &Path| {
        p.components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    };
    for d in &dirs {
        if top(d).starts_with("_tmp") {
            continue;
        }
        fs::create_dir_all(output.join(d))?;
    }
    let bgm = args.bgm_kbps;
    let results: Vec<(String, Result<Dealt>)> = files
        .par_iter()
        .map(|rel| {
            let shown = rel.to_string_lossy().into_owned();
            let from = src.join(rel);
            let to = output.join(rel);
            let work = || -> Result<Dealt> {
                let name = rel
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let size = fs::metadata(&from)?.len();
                if top(rel).starts_with("_tmp") {
                    return Ok(Dealt::Skipped("temporary folder", size));
                }
                if let Some(why) = is_junk(&name) {
                    return Ok(Dealt::Skipped(why, size));
                }
                let l = name.to_ascii_lowercase();
                if rel.components().count() > 1 && user_dirs.contains(&top(rel).as_str()) {
                    return Ok(Dealt::Skipped("the user's own files", size));
                }
                if rel.components().count() == 1 && l.ends_with(".grf") && !args.keep_grf {
                    return Ok(Dealt::Skipped("archive (built separately)", size));
                }
                let copy = |kept: &'static str| -> Result<Dealt> {
                    fs::copy(&from, &to).with_context(|| format!("cannot copy {shown}"))?;
                    Ok(if kept.is_empty() {
                        Dealt::Copied(size)
                    } else {
                        Dealt::Left(kept)
                    })
                };
                if let (Some(kbps), true) = (bgm, l.ends_with(".mp3")) {
                    let data = fs::read(&from)?;
                    return match reencode_mp3(&data, kbps) {
                        Ok(Some((out, note))) => {
                            fs::write(&to, &out)?;
                            Ok(Dealt::Changed {
                                kind: "music",
                                before: size,
                                after: out.len() as u64,
                                note,
                            })
                        }
                        Ok(None) => copy("music kept (already low enough or no gain)"),
                        Err(_) => copy("music kept (not readable)"),
                    };
                }
                // the guild emblems must stay the 24-bit pictures the client asks for
                if let (Some(psnr), true) =
                    (args.min_psnr, l.ends_with(".bmp") && top(rel) != "emblem")
                {
                    let data = fs::read(&from)?;
                    return Ok(match bmp::reduce(&data, psnr) {
                        bmp::Reduced::Done { bytes, psnr, .. } if (bytes.len() as u64) < size => {
                            fs::write(&to, &bytes)?;
                            Dealt::Changed {
                                kind: "BMP",
                                before: size,
                                after: bytes.len() as u64,
                                note: format!("{psnr:.1} dB"),
                            }
                        }
                        _ => copy("")?,
                    });
                }
                if let (Some(iterations), true) = (args.png, l.ends_with(".png")) {
                    let data = fs::read(&from)?;
                    let deflate = |d: &[u8]| encode(Method::Zopfli { iterations }, d);
                    if let rgrf_formats::png_opt::Outcome::Smaller(s) =
                        rgrf_formats::png_opt::optimize(&data, &deflate)
                    {
                        fs::write(&to, &s)?;
                        return Ok(Dealt::Changed {
                            kind: "PNG",
                            before: size,
                            after: s.len() as u64,
                            note: "same picture".into(),
                        });
                    }
                    return copy("");
                }
                if let (Some(tool), true) =
                    (args.jpegtran, l.ends_with(".jpg") || l.ends_with(".jpeg"))
                {
                    let data = fs::read(&from)?;
                    if let Some(s) = crate::image_cmd::jpegtran_optimize(tool, &data)? {
                        fs::write(&to, &s)?;
                        return Ok(Dealt::Changed {
                            kind: "JPEG",
                            before: size,
                            after: s.len() as u64,
                            note: "same picture".into(),
                        });
                    }
                    return copy("");
                }
                copy("")
            };
            let done = work();
            (shown, done)
        })
        .collect();

    let mut by_kind: std::collections::BTreeMap<&str, Tally> = Default::default();
    let mut skipped: std::collections::BTreeMap<&str, Tally> = Default::default();
    let mut left: std::collections::BTreeMap<&str, u64> = Default::default();
    let (mut copied, mut copied_bytes) = (0u64, 0u64);
    let mut csv = String::from("kind,path,before,after,note\n");
    for (shown, r) in results {
        match r? {
            Dealt::Copied(b) => {
                copied += 1;
                copied_bytes += b;
            }
            Dealt::Left(why) => *left.entry(why).or_default() += 1,
            Dealt::Skipped(why, b) => {
                let t = skipped.entry(why).or_default();
                t.files += 1;
                t.before += b;
            }
            Dealt::Changed {
                kind,
                before,
                after,
                note,
            } => {
                let t = by_kind.entry(kind).or_default();
                t.files += 1;
                t.before += before;
                t.after += after;
                csv.push_str(&format!(
                    "{kind},\"{}\",{before},{after},\"{note}\"\n",
                    shown.replace('"', "\"\"")
                ));
            }
        }
    }
    if let Some(p) = args.report {
        fs::write(p, csv).with_context(|| format!("cannot write {}", p.display()))?;
    }
    let secs = started.elapsed().as_secs_f64();
    let mib = |b: u64| b as f64 / f64::from(1u32 << 20);
    if json {
        let kinds: Vec<_> = by_kind
            .iter()
            .map(|(k, t)| json!({"kind": k, "files": t.files, "bytes_before": t.before, "bytes_after": t.after}))
            .collect();
        let sk: Vec<_> = skipped
            .iter()
            .map(|(k, t)| json!({"reason": k, "files": t.files, "bytes": t.before}))
            .collect();
        crate::print_json(&json!({
            "schema_version": crate::SCHEMA_VERSION,
            "copied": copied, "copied_bytes": copied_bytes,
            "changed": kinds, "skipped": sk, "seconds": secs,
        }))?;
    } else {
        println!("{} files -> {}", files.len(), output.display());
        println!(
            "Copied unchanged: {copied} files, {:.1} MiB",
            mib(copied_bytes)
        );
        for (k, t) in &by_kind {
            println!(
                "{k:<6} {} files made smaller: {:.1} -> {:.1} MiB (saves {:.1} MiB)",
                t.files,
                mib(t.before),
                mib(t.after),
                mib(t.before - t.after)
            );
        }
        for (k, n) in &left {
            println!("Kept: {n} x {k}");
        }
        for (k, t) in &skipped {
            println!("Left out: {} files, {:.1} MiB: {k}", t.files, mib(t.before));
        }
        println!("Took {secs:.1} s");
    }
    Ok(0)
}
