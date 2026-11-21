//! GrfCL's `thorPack`, `thorUnpack`, `hash`, `hashComp` and `addFakeClientinfo`: Thor Patcher
//! executables with packed files, hash lists of a folder, and a definitions file.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rgrf_core::definitions;
use rgrf_core::hashlist::{self, HashList, HashMethod};
use rgrf_core::progress::Control;
use rgrf_core::thorpack;
use serde_json::json;

fn check_free(path: &Path, overwrite: bool) -> Result<()> {
    if path.exists() && !overwrite {
        bail!("{} exists (use --overwrite)", path.display());
    }
    Ok(())
}

fn files_below(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .with_context(|| format!("cannot read {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files_below(&p, out)?;
        } else {
            out.push(p);
        }
    }
    Ok(())
}

pub struct PackArgs<'a> {
    pub program: &'a Path,
    pub output: &'a Path,
    pub config_dir: &'a Path,
    pub folders: &'a [String],
    pub overwrite: bool,
}

/// `thor pack`: `config.ini` (stored as `config`) and `LanguageMap.ini` of the config folder, and
/// the files of the given subfolders (stored with the folder name in front, `/` between parts
/// except under `Languages`), packed behind the Thor Patcher program.
pub fn thor_pack(a: &PackArgs, json: bool) -> Result<i32> {
    check_free(a.output, a.overwrite)?;
    let program =
        std::fs::read(a.program).with_context(|| format!("cannot read {}", a.program.display()))?;
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (stored, file) in [
        ("config", "config.ini"),
        ("LanguageMap.ini", "LanguageMap.ini"),
    ] {
        let p = a.config_dir.join(file);
        let data = std::fs::read(&p).with_context(|| format!("cannot read {}", p.display()))?;
        files.push((stored.to_string(), data));
    }
    for folder in a.folders {
        let root = a.config_dir.join(folder);
        if !root.is_dir() {
            bail!("{} is not a folder", root.display());
        }
        let parent = root.parent().unwrap_or(a.config_dir).to_path_buf();
        let mut list = Vec::new();
        files_below(&root, &mut list)?;
        for f in list {
            let rel = f
                .strip_prefix(&parent)
                .unwrap_or(&f)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            let sep = if rel.first().is_some_and(|f| f == "Languages") {
                "\\"
            } else {
                "/"
            };
            files.push((
                rel.join(sep),
                std::fs::read(&f).with_context(|| format!("cannot read {}", f.display()))?,
            ));
        }
    }
    let packed = thorpack::pack(&program, &files)?;
    std::fs::write(a.output, &packed)
        .with_context(|| format!("cannot write {}", a.output.display()))?;
    if json {
        println!(
            "{}",
            json!({"schema_version": 1, "output": a.output.display().to_string(), "files": files.len(), "bytes": packed.len()})
        );
    } else {
        println!(
            "Wrote {}: {} files packed behind the program",
            a.output.display(),
            files.len()
        );
    }
    Ok(0)
}

/// `thor unpack`: writes the packed files of a Thor Patcher into `dest` (`config` becomes `config.ini`).
pub fn thor_unpack(packed: &Path, dest: &Path, overwrite: bool, json: bool) -> Result<i32> {
    let exe = std::fs::read(packed).with_context(|| format!("cannot read {}", packed.display()))?;
    let (program_len, files) = thorpack::unpack(&exe)?;
    let mut written = Vec::new();
    for (name, data) in &files {
        let name = if name == "config" {
            "config.ini"
        } else {
            name.as_str()
        };
        let mut target = dest.to_path_buf();
        for part in name.split(['/', '\\']) {
            if part.is_empty() || part == ".." || part.contains(':') {
                bail!("the packed name '{name}' is not a safe path");
            }
            target.push(part);
        }
        check_free(&target, overwrite)?;
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&target, data)?;
        written.push(name.to_string());
    }
    if json {
        println!(
            "{}",
            json!({"schema_version": 1, "program_bytes": program_len, "files": written})
        );
    } else {
        println!(
            "{} files written to {} (the program in front of them is {program_len} bytes)",
            written.len(),
            dest.display()
        );
    }
    Ok(0)
}

fn method_of(name: &str) -> HashMethod {
    match name {
        "crc32" => HashMethod::Crc32,
        "md5" => HashMethod::Md5,
        "size" => HashMethod::FileSize,
        _ => HashMethod::QuickMd5,
    }
}

/// `hashlist create`: the hashes of a folder (and of the entries of the archives in it).
pub fn hashlist_create(
    folder: &Path,
    output: &Path,
    pattern: &str,
    method: &str,
    overwrite: bool,
    json: bool,
) -> Result<i32> {
    check_free(output, overwrite)?;
    let list = hashlist::hash_folder(folder, pattern, method_of(method), &Control::default())?;
    list.write(output)?;
    if json {
        println!(
            "{}",
            json!({"schema_version": 1, "output": output.display().to_string(), "files": list.hashes.len(), "method": list.method.name()})
        );
    } else {
        println!(
            "{} files hashed ({}), written to {}",
            list.hashes.len(),
            list.method.name(),
            output.display()
        );
    }
    Ok(0)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// `hashlist show`: the method and every path with its hash.
pub fn hashlist_show(file: &Path, json: bool) -> Result<i32> {
    let l = HashList::read(file)?;
    if json {
        let files: Vec<_> = l
            .hashes
            .iter()
            .map(|(k, v)| json!({"path": k, "hash": hex(v)}))
            .collect();
        println!(
            "{}",
            json!({"schema_version": 1, "method": l.method.name(), "head_dir": l.head_dir, "partial": l.partial, "files": files})
        );
    } else {
        println!(
            "{} ({} files), folder {}",
            l.method.name(),
            l.hashes.len(),
            l.head_dir
        );
        for (k, v) in &l.hashes {
            println!("{}  {k}", hex(v));
        }
    }
    Ok(0)
}

/// `hashlist compare`: what the user's list lacks, changes or adds against the master's (exit 0
/// nothing, 2 differences).
pub fn hashlist_compare(master: &Path, user: &Path, exact: bool, json: bool) -> Result<i32> {
    let (m, u) = (HashList::read(master)?, HashList::read(user)?);
    // GRF Editor stores quick MD5 hashes as plain MD5, so only the length tells a list apart
    if m.method.len() != u.method.len() {
        bail!(
            "the lists use different hash methods ({} and {})",
            m.method.name(),
            u.method.name()
        );
    }
    let diffs = hashlist::compare(&m, &u, exact);
    if json {
        let items: Vec<_> = diffs
            .iter()
            .map(|d| json!({"issue": d.issue.name(), "path": d.path, "user_path": d.user_path}))
            .collect();
        println!(
            "{}",
            json!({"schema_version": 1, "issues": items.len(), "differences": items})
        );
    } else {
        for d in &diffs {
            match &d.user_path {
                Some(p) => println!("{:<13} {}  ->  {p}", d.issue.name(), d.path),
                None => println!("{:<13} {}", d.issue.name(), d.path),
            }
        }
        println!("{} issues", diffs.len());
    }
    Ok(if diffs.is_empty() { 0 } else { 2 })
}

/// `client definitions`: the `__definitions` file of a definitions text (archives are looked
/// for next to the text unless `base` says otherwise).
pub fn definitions_cmd(
    text: &Path,
    output: &Path,
    base: Option<&Path>,
    overwrite: bool,
    json: bool,
) -> Result<i32> {
    check_free(output, overwrite)?;
    let source =
        std::fs::read_to_string(text).with_context(|| format!("cannot read {}", text.display()))?;
    let base = base
        .map(Path::to_path_buf)
        .or_else(|| text.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let open = |p: &str| {
        let path = base.join(p.replace('\\', "/"));
        rgrf_core::Archive::open(&path).map_err(|e| format!("{}: {e}", path.display()))
    };
    let (bytes, report) = definitions::generate(&source, &open).map_err(anyhow::Error::msg)?;
    std::fs::write(output, &bytes).with_context(|| format!("cannot write {}", output.display()))?;
    if json {
        let items: Vec<_> = report
            .iter()
            .map(|(n, c)| json!({"archive": n, "files": c}))
            .collect();
        println!(
            "{}",
            json!({"schema_version": 1, "output": output.display().to_string(), "archives": items})
        );
    } else {
        for (n, c) in &report {
            println!("{n}: {c} files hashed");
        }
        println!("Wrote {}", output.display());
    }
    Ok(0)
}
