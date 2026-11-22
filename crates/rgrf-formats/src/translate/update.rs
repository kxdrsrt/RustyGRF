//! Updates `core.grf` and `cdata.grf` to the newest translation and keeps everything that is yours.
//!
//! Policy (decided per file, see [`super::mods`] and [`super::personal`] for the details):
//! - your file is an unmodified OLDER version of a project file: replaced by the current version,
//! - you edited a project file (three-way merge, your lines win): merged,
//! - `msgstringtable.txt` / `skillinfolist.lub` / `skilldescript.lub`: entry-wise merge,
//! - your file is not from the project at all (own textures, other Lua): left exactly as it is,
//! - a file that is new in the translation and not in your GRF: added (core only; cdata only
//!   replaces).
//!
//! Result: `<out>/core.grf`, `<out>/cdata.grf` (and `<out>/report.md`); nothing in `client-dev` is
//! touched. Needs the `rgrf` program, git, the output of `assemble` and a FULL clone of ROenglishRE.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use unicode_normalization::UnicodeNormalization;

use super::{GrfTool, IoContext, Result, make_dirs, mods, msg, personal, walk, write};

#[derive(Debug, Clone)]
pub struct Options {
    /// A FULL clone of ROenglishRE.
    pub repo: PathBuf,
    /// The output of `assemble`.
    pub asm: PathBuf,
    pub client_dev: PathBuf,
    /// The output folder; it must not exist.
    pub out: PathBuf,
    pub built_before: String,
    /// Data paths never touched (server files etc.).
    pub exclude: Vec<String>,
    /// Skills whose `SkillName` always stays yours.
    pub keep_skill: Vec<String>,
    /// A folder (archive root, contains `data/`) with your own files that go into `core.grf` with
    /// the highest priority, e.g. a skill icon.
    pub extra_core: Option<PathBuf>,
}

fn copy_tree(src: &Path, dst: &Path) -> Result<()> {
    make_dirs(dst)?;
    for e in fs::read_dir(src)
        .ctx(|| format!("cannot read {}", src.display()))?
        .flatten()
    {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if from.is_dir() {
            copy_tree(&from, &to)?;
        } else {
            super::copy_file(&from, &to)?;
        }
    }
    Ok(())
}

fn count_files(root: &Path) -> usize {
    walk(root).iter().map(|(_, f)| f.len()).sum()
}

/// `rgrf build` of a folder, or `None` when there is nothing in it.
fn build(tool: &GrfTool, folder: &Path, grf: &Path) -> Result<Option<PathBuf>> {
    let empty = fs::read_dir(folder).map_or(true, |mut r| r.next().is_none());
    if !folder.is_dir() || empty {
        return Ok(None);
    }
    tool.build(folder, grf)?;
    Ok(Some(grf.to_path_buf()))
}

/// Runs the update; returns the report text (also written to `<out>/report.md`).
pub fn run(tool: &GrfTool, o: &Options) -> Result<String> {
    if o.out.exists() {
        return msg(format!("{} exists", o.out.display()));
    }
    make_dirs(&o.out)?;
    let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let (repo, asm, client_dev, out) = (abs(&o.repo), abs(&o.asm), abs(&o.client_dev), abs(&o.out));

    let mods_dir = out.join("mods");
    let mut m = mods::Options::new(
        repo.clone(),
        client_dev.clone(),
        asm.clone(),
        mods_dir.clone(),
    );
    m.built_before.clone_from(&o.built_before);
    m.exclude.clone_from(&o.exclude);
    let (report, _) = mods::run(&m)?;
    let personal_dir = out.join("personal");
    let mut p = personal::Options::new(repo, client_dev.clone(), asm.clone(), personal_dir.clone());
    p.built_before.clone_from(&o.built_before);
    p.keep_skill.clone_from(&o.keep_skill);
    personal::run(&p)?;

    // files that must NOT come from the plain translation: everything mods looked at (recognised:
    // its own output, unknown: keep yours) and everything personal handled
    let mut skip: HashSet<String> = report.iter().map(|e| e.path().to_lowercase()).collect();
    for (dir, files) in walk(&personal_dir) {
        for f in files {
            if f != "report.md" {
                let rel = dir.join(&f);
                let rel = rel
                    .strip_prefix(&personal_dir)
                    .unwrap_or(&rel)
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/");
                // <archive>/data/... -> data/...
                if let Some((_, rest)) = rel.split_once('/') {
                    skip.insert(rest.to_lowercase());
                }
            }
        }
    }
    skip.extend(
        o.exclude
            .iter()
            .map(|x| x.to_lowercase().replace('\\', "/")),
    );
    let skip_nfc: HashSet<String> = skip.iter().map(|x| x.nfc().collect()).collect();

    let plain = out.join("translation-plain");
    copy_tree(&asm.join("grf"), &plain)?;
    let mut removed = 0;
    for (dir, files) in walk(&plain) {
        for f in files {
            let path = dir.join(&f);
            let rel = path
                .strip_prefix(&plain)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
            let norm: String = rel.nfc().collect();
            if skip_nfc.contains(&norm.to_lowercase()) {
                fs::remove_file(&path).ctx(|| format!("cannot remove {}", path.display()))?;
                removed += 1;
            }
        }
    }
    let translation = build(tool, &plain, &out.join("translation-plain.grf"))?;
    let Some(translation) = translation else {
        return msg("the translation has no files to build");
    };
    let mut summary = vec![
        "# Update report\n".to_string(),
        format!(
            "Plain translation files: {} (left out because yours are kept or merged: {removed})\n",
            count_files(&plain)
        ),
    ];
    for (name, replace_only) in [("core", false), ("cdata", true)] {
        let mut layers: Vec<PathBuf> = Vec::new();
        if name == "core"
            && let Some(extra) = &o.extra_core
            && let Some(g) = build(tool, &abs(extra), &out.join("extra-core.grf"))?
        {
            layers.push(g);
        }
        for (label, root) in [("personal", &personal_dir), ("mods", &mods_dir)] {
            if let Some(g) = build(
                tool,
                &root.join(name),
                &out.join(format!("{label}-{name}.grf")),
            )? {
                layers.push(g);
            }
        }
        let old = client_dev.join("full").join(format!("{name}.grf"));
        let new = out.join(format!("{name}.grf"));
        layers.push(translation.clone());
        layers.push(old.clone());
        tool.merge(&layers, &new, replace_only)?;
        let d = tool.last_line(&["diff".as_ref(), old.as_os_str(), new.as_os_str()])?;
        let v = tool.last_line(&["verify".as_ref(), new.as_os_str()])?;
        summary.push(format!("## {name}.grf\n- old vs new: {d}\n- verify: {v}\n"));
    }
    summary.push(
        "Details: mods/report.json (per file), personal/report.md (your edits that were carried over).\n"
            .to_string(),
    );
    let text = summary.join("\n");
    write(&out.join("report.md"), text.as_bytes())?;
    Ok(text)
}
