//! `rgrf thor create` and `rgrf rgz create`: patches from a folder or from another archive.

use std::path::Path;

use anyhow::{Context, Result, bail};
use rgrf_core::build::scan_folder;
use rgrf_core::encode::Method;
use rgrf_core::rgz;
use rgrf_core::thor::{self, ThorFile, ThorOptions};
use rgrf_core::{DecodeStatus, GrfPath, NameMode};
use serde_json::json;

/// The files of a folder (the folder is the patch root) or of an archive, decoded.
fn collect(source: &Path, names: NameMode) -> Result<Vec<(GrfPath, Vec<u8>)>> {
    if source.is_dir() {
        let (files, _) = scan_folder(source, names)?;
        return files
            .into_iter()
            .map(|(name, path)| {
                let data = std::fs::read(&path)
                    .with_context(|| format!("cannot read {}", path.display()))?;
                Ok((name, data))
            })
            .collect();
    }
    let a = crate::crypt_cmd::open_archive(source)
        .with_context(|| format!("cannot open {}", source.display()))?;
    let mut out = Vec::new();
    for (i, e) in a.entries().iter().enumerate() {
        if e.is_marker() || e.is_removal() {
            continue;
        }
        let d = a.read(i)?;
        match d.status {
            DecodeStatus::Ok => out.push((e.name.clone(), d.data)),
            DecodeStatus::Encrypted => {
                bail!("{} is encrypted and cannot be read", e.name.display(names))
            }
            s => bail!(
                "{} is damaged ({s:?}); run `rgrf verify`",
                e.name.display(names)
            ),
        }
    }
    if out.is_empty() {
        bail!("no files in {}", source.display());
    }
    Ok(out)
}

pub struct ThorArgs<'a> {
    pub source: &'a Path,
    pub output: &'a Path,
    pub merge_into_grf: bool,
    pub target: Option<&'a str>,
    pub remove: &'a [String],
    pub integrity: bool,
    pub method: Method,
    pub overwrite: bool,
}

pub fn thor_create(a: &ThorArgs, names: NameMode, json: bool) -> Result<i32> {
    let mut files: Vec<ThorFile> = collect(a.source, names)?
        .into_iter()
        .map(|(name, data)| ThorFile {
            name,
            data: Some(data),
        })
        .collect();
    for r in a.remove {
        let name = GrfPath::from_display(r, names)
            .with_context(|| format!("'{r}' cannot be written in the name mode {names:?}"))?;
        files.push(ThorFile { name, data: None });
    }
    let target = match a.target {
        Some(t) => GrfPath::from_display(t, names)
            .with_context(|| format!("'{t}' cannot be written in the name mode {names:?}"))?
            .as_bytes()
            .to_vec(),
        None => Vec::new(),
    };
    if a.target.is_some() && !a.merge_into_grf {
        bail!("--target needs --merge-into-grf");
    }
    let opts = ThorOptions {
        merge_into_grf: a.merge_into_grf,
        target_grf: target,
        method: a.method,
        integrity: a.integrity,
        overwrite: a.overwrite,
    };
    let r = thor::write(a.output, &files, &opts)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "output": a.output.display().to_string(),
                "mode": format!("0x{:02x}", r.mode),
                "files": r.files,
                "removals": r.removals,
                "shared_streams": r.shared,
                "unpacked_bytes": r.bytes,
            }))?
        );
    } else {
        println!(
            "Wrote {} (Thor mode 0x{:02x}): {} files, {} delete directives, {} shared streams",
            a.output.display(),
            r.mode,
            r.files,
            r.removals,
            r.shared
        );
    }
    Ok(0)
}

pub fn rgz_create(
    source: &Path,
    output: &Path,
    level: u32,
    overwrite: bool,
    names: NameMode,
    json: bool,
) -> Result<i32> {
    let files = collect(source, names)?;
    rgz::write(output, &files, level, overwrite)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "output": output.display().to_string(),
                "files": files.len(),
            }))?
        );
    } else {
        println!("Wrote {} (RGZ): {} files", output.display(), files.len());
    }
    Ok(0)
}
