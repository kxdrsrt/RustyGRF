//! Installs the result of `update` into `client-dev`: backup first, a dry run unless `apply`.
//!
//! 1. backs up `full/core.grf`, `full/cdata.grf` and every file of `grf/core/data` and
//!    `grf/cdata/data` that changes;
//! 2. updates your unpacked source folders `grf/core/data` and `grf/cdata/data` from the new archives
//!    (otherwise the next `client-dev/build-grfs.ps1` run would build the old content again):
//!    changed files are overwritten, new files are added, files that are identical stay untouched,
//!    names keep their spelling (case-insensitive);
//! 3. copies the new `core.grf` and `cdata.grf` to `full/`;
//! 4. copies the loose folder `SystemEN` (output of `assemble`, `loose/SystemEN`) to
//!    `full/SystemEN`. Other loose files of the package (cps.dll, DATA.ini) are NOT copied: they
//!    must not replace yours.
//!
//! Without `apply` nothing is written and the plan is printed.

use std::fs;
use std::path::{Path, PathBuf};

use super::{
    GrfTool, IoContext, Result, bump, copy_file, dict_repr, fold_name, fold_parts, make_dirs, msg,
    walk,
};

#[derive(Debug, Clone)]
pub struct Options {
    /// The output folder of `update` (`core.grf`, `cdata.grf`).
    pub update: PathBuf,
    /// The output of `assemble` (for `loose/SystemEN`).
    pub asm: PathBuf,
    /// The `client-dev` folder (contains `full/` and `grf/`).
    pub client_dev: PathBuf,
    /// The backup folder; it must not exist when applying.
    pub backup: PathBuf,
    /// Really write; the default is a dry run.
    pub apply: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    New,
    Changed,
    Same,
}

impl State {
    fn name(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Changed => "changed",
            Self::Same => "same",
        }
    }
}

/// `[(source file, target file, state)]`.
fn plan(src_data: &Path, dst_data: &Path) -> Result<Vec<(PathBuf, PathBuf, State)>> {
    let mut out = Vec::new();
    for (dir, files) in walk(src_data) {
        let rel = dir
            .strip_prefix(src_data)
            .unwrap_or(&dir)
            .to_string_lossy()
            .into_owned();
        let mut target_dir = dst_data.to_path_buf();
        if !rel.is_empty() {
            for part in fold_parts(dst_data, &rel) {
                target_dir.push(part);
            }
        }
        let mut files = files;
        files.sort();
        for f in files {
            let target = target_dir.join(fold_name(&target_dir, &f));
            let src = dir.join(&f);
            let state = if !target.is_file() {
                State::New
            } else if super::read(&src)? == super::read(&target)? {
                State::Same
            } else {
                State::Changed
            };
            out.push((src, target, state));
        }
    }
    Ok(out)
}

/// `shutil.copy2`: content and modification time.
fn copy2(src: &Path, dst: &Path) -> Result<()> {
    copy_file(src, dst)?;
    if let Ok(t) = fs::metadata(src).and_then(|m| m.modified())
        && let Ok(f) = fs::OpenOptions::new().write(true).open(dst)
    {
        let _ = f.set_modified(t);
    }
    Ok(())
}

fn copy_tree(src: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    make_dirs(dst)?;
    for e in fs::read_dir(src)
        .ctx(|| format!("cannot read {}", src.display()))?
        .flatten()
    {
        let (from, to) = (e.path(), dst.join(e.file_name()));
        if from.is_dir() {
            copy_tree(&from, &to, overwrite)?;
        } else if overwrite || !to.exists() {
            copy2(&from, &to)?;
        }
    }
    Ok(())
}

/// Runs the installation; `log` gets the lines the script printed.
pub fn run(tool: &GrfTool, o: &Options, log: &mut dyn FnMut(&str)) -> Result<()> {
    let names = ["core", "cdata"];
    for n in names {
        for p in [
            o.client_dev.join("full").join(format!("{n}.grf")),
            o.update.join(format!("{n}.grf")),
            o.client_dev.join("grf").join(n).join("data"),
        ] {
            if !p.exists() {
                return msg(format!("missing: {}", p.display()));
            }
        }
    }
    let sys_en = o.asm.join("loose").join("SystemEN");
    if !sys_en.is_dir() {
        return msg(format!("missing: {}", sys_en.display()));
    }
    if o.apply && o.backup.exists() {
        return msg(format!(
            "{} exists; refusing to write into it",
            o.backup.display()
        ));
    }
    let tmp = o.update.join(".install-work");
    make_dirs(&tmp)?;
    let result = (|| -> Result<()> {
        let mut plans = Vec::new();
        for n in names {
            let out = tmp.join(n);
            tool.extract(&o.update.join(format!("{n}.grf")), &out)?;
            let base = o.client_dev.join("grf").join(n).join("data");
            let p = plan(&out.join("data"), &base)?;
            let mut counts: Vec<(String, usize)> = Vec::new();
            for (_, _, st) in &p {
                bump(&mut counts, st.name());
            }
            log(&format!("grf/{n}/data: {}", dict_repr(&counts)));
            for (_, t, st) in &p {
                if *st == State::Changed {
                    log(&format!(
                        "   changed: {}",
                        t.strip_prefix(&base).unwrap_or(t).display()
                    ));
                }
            }
            plans.push((n, p));
        }
        log(&format!(
            "full/SystemEN: {} files",
            walk(&sys_en).iter().map(|(_, f)| f.len()).sum::<usize>()
        ));
        if !o.apply {
            log("dry run: nothing written (use --apply)");
            return Ok(());
        }
        make_dirs(&o.backup)?;
        for (n, p) in &plans {
            let base = o.client_dev.join("grf").join(n).join("data");
            let full = o.client_dev.join("full").join(format!("{n}.grf"));
            copy2(&full, &o.backup.join(format!("{n}.grf")))?;
            for (_, t, st) in p {
                if *st == State::Changed {
                    let b = o
                        .backup
                        .join("grf")
                        .join(n)
                        .join("data")
                        .join(t.strip_prefix(&base).unwrap_or(t));
                    if let Some(d) = b.parent() {
                        make_dirs(d)?;
                    }
                    copy2(t, &b)?;
                }
            }
            for (s, t, st) in p {
                if matches!(st, State::New | State::Changed) {
                    if let Some(d) = t.parent() {
                        make_dirs(d)?;
                    }
                    copy_file(s, t)?;
                }
            }
            let mut staged = full.clone().into_os_string();
            staged.push(".new");
            let staged = PathBuf::from(staged);
            copy_file(&o.update.join(format!("{n}.grf")), &staged)?;
            fs::rename(&staged, &full).ctx(|| format!("cannot replace {}", full.display()))?;
        }
        let full_sys_en = o.client_dev.join("full").join("SystemEN");
        if full_sys_en.is_dir() {
            copy_tree(&full_sys_en, &o.backup.join("SystemEN"), true)?;
        }
        copy_tree(&sys_en, &full_sys_en, true)?;
        log(&format!("done; backup in {}", o.backup.display()));
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp);
    result
}
