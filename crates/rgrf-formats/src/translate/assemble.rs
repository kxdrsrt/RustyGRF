//! Assembles a translated client folder from a checkout of `llchrisll/ROenglishRE`.
//!
//! Re-implements the copy order of the project's `Tools/ClientGenerator.bat` so it runs on macOS
//! and Linux too. Later copies overwrite earlier ones (`xcopy /Y`), so the priority from lowest to
//! highest is: `Translation/Renewal` (project base, 2015-10-29), `Translation/Pre-Renewal` (only
//! for the pre-renewal mode: wins over Renewal), `Translation/Compatibility/<date>/[<mode>/]` for
//! every menu date from 2017-06-14 up to the client date, then the deletions the generator does
//! for newer clients.
//!
//! The overlay is case-insensitive on purpose (as on Windows): `luafiles514` and `LuaFiles514` are
//! one folder. The `Additions` folder and CLS are not part of this.

use std::fs;
use std::path::{Path, PathBuf};

use super::{IoContext, Result, copy_file, fold_name, fold_parts, make_dirs, msg, walk};

/// The menu of `ClientGenerator.bat`, number 1 is the first.
pub const MENU: [&str; 31] = [
    "2012-04-10",
    "2015-05-13",
    "2015-10-29",
    "2017-06-14",
    "2017-12-13",
    "2018-06-20",
    "2019-06-05",
    "2020-09-02",
    "2021-10-28",
    "2022-03-30",
    "2022-04-06",
    "2022-06-02",
    "2022-08-31",
    "2022-09-28",
    "2022-12-07",
    "2023-01-18",
    "2023-08-02",
    "2023-09-20",
    "2024-03-11",
    "2024-04-03",
    "2024-05-02",
    "2024-08-07",
    "2024-10-16",
    "2025-01-22",
    "2025-06-18",
    "2025-09-02",
    "2025-12-17",
    "2026-04-01",
    "2026-08-05",
    "2026-08-19",
    "2026-10-21",
];

#[derive(Debug, Clone)]
pub struct Options {
    /// Pre-renewal (`true`) or renewal server.
    pub pre_renewal: bool,
    /// A client date of [`MENU`].
    pub client: String,
    /// The output folder; it must not exist.
    pub out: PathBuf,
    /// Files (path relative to the archive root, `/` separators, case-insensitive, `*` `?` `[..]`)
    /// that are dropped: files of your own server that the translation must not overwrite.
    pub exclude: Vec<String>,
}

/// Python's `fnmatch.fnmatchcase` on two lower-cased strings.
pub(crate) fn fnmatch(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some('*') => (0..=n.len()).any(|i| go(&p[1..], &n[i..])),
            Some('?') => !n.is_empty() && go(&p[1..], &n[1..]),
            Some('[') => {
                // the closing bracket: a `]` right after `[` or `[!` belongs to the set
                let mut j = 1;
                if p.get(j) == Some(&'!') {
                    j += 1;
                }
                if p.get(j) == Some(&']') {
                    j += 1;
                }
                while j < p.len() && p[j] != ']' {
                    j += 1;
                }
                if j >= p.len() {
                    return !n.is_empty() && n[0] == '[' && go(&p[1..], &n[1..]);
                }
                let Some(&c) = n.first() else { return false };
                let mut set = &p[1..j];
                let negate = set.first() == Some(&'!');
                if negate {
                    set = &set[1..];
                }
                let mut hit = false;
                let mut k = 0;
                while k < set.len() {
                    if k + 2 < set.len() && set[k + 1] == '-' {
                        hit |= set[k] <= c && c <= set[k + 2];
                        k += 3;
                    } else {
                        hit |= set[k] == c;
                        k += 1;
                    }
                }
                hit != negate && go(&p[j + 1..], &n[1..])
            }
            Some(&c) => n.first() == Some(&c) && go(&p[1..], &n[1..]),
        }
    }
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    go(&p, &n)
}

/// `xcopy /E /Y`: copies the tree, overwriting files; names match case-insensitively. Returns
/// the number of files.
fn overlay(src: &Path, dst: &Path) -> Result<usize> {
    let mut count = 0;
    for (dir, files) in walk(src) {
        let rel = dir.strip_prefix(src).unwrap_or(&dir);
        let rel = rel.to_string_lossy().into_owned();
        let target = if rel.is_empty() {
            dst.to_path_buf()
        } else {
            let mut t = dst.to_path_buf();
            for part in fold_parts(dst, &rel) {
                t.push(part);
            }
            t
        };
        make_dirs(&target)?;
        for f in files {
            let name = fold_name(&target, &f);
            copy_file(&dir.join(&f), &target.join(name))?;
            count += 1;
        }
    }
    Ok(count)
}

fn compat_dir(repo: &Path, date: &str, mode_dir: &str) -> PathBuf {
    let base = repo.join("Translation").join("Compatibility").join(date);
    let sub = base.join(mode_dir);
    if sub.is_dir() { sub } else { base }
}

/// Runs the assembly; `log` gets the lines the script printed.
pub fn assemble(repo: &Path, opts: &Options, log: &mut dyn FnMut(&str)) -> Result<()> {
    if opts.out.exists() {
        return msg(format!(
            "{} exists; refusing to write into it",
            opts.out.display()
        ));
    }
    let Some(index) = MENU.iter().position(|d| *d == opts.client) else {
        return msg(format!(
            "unknown client date {} (one of {})",
            opts.client,
            MENU.join(", ")
        ));
    };
    let n = index + 1;
    let mode_dir = if opts.pre_renewal {
        "Pre-Renewal"
    } else {
        "Renewal"
    };
    log(&format!(
        "mode {mode_dir}, client {} (menu #{n})",
        opts.client
    ));
    let final_dir = opts.out.clone();
    let all = final_dir.join("all");
    make_dirs(&all)?;
    let mut copy = |src: PathBuf| -> Result<()> {
        let count = overlay(&src, &all)?;
        log(&format!("  {count:6} files from {}", src.display()));
        Ok(())
    };
    copy(repo.join("Translation").join("Renewal"))?;
    if opts.pre_renewal {
        copy(repo.join("Translation").join("Pre-Renewal"))?;
    }
    if n == 1 || n == 2 {
        copy(compat_dir(repo, MENU[n - 1], mode_dir))?;
    } else if n >= 4 {
        // the batch loop copies menu items 4..n-1, then the client's own folder
        for i in 4..=n {
            copy(compat_dir(repo, MENU[i - 1], mode_dir))?;
        }
    }
    let at = |rel: &str| -> PathBuf {
        let mut p = all.clone();
        for part in fold_parts(&all, rel) {
            p.push(part);
        }
        p
    };
    let sep = std::path::MAIN_SEPARATOR;
    if n >= 19 {
        let rel = ["data", "luafiles514", "lua files", "hateffectinfo"].join(&sep.to_string());
        let _ = fs::remove_dir_all(at(&rel));
    }
    if n >= 25 {
        let rel = ["data", "luafiles514", "lua files", "selectpackage"].join(&sep.to_string());
        let p = at(&rel);
        let f = p.join(fold_name(&p, "selectpackageitem.lub"));
        if f.is_file() {
            fs::remove_file(&f).ctx(|| format!("cannot remove {}", f.display()))?;
        }
    }
    for pattern in &opts.exclude {
        let pat = pattern.to_lowercase().replace('\\', "/");
        let mut removed = 0;
        for (dir, files) in walk(&all) {
            for f in files {
                let path = dir.join(&f);
                let rel = path
                    .strip_prefix(&all)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace(sep, "/");
                if fnmatch(&pat, &rel.to_lowercase()) {
                    fs::remove_file(&path).ctx(|| format!("cannot remove {}", path.display()))?;
                    removed += 1;
                }
            }
        }
        log(&format!("  excluded {removed} file(s) for {pattern}"));
    }
    // what belongs in a GRF (data/) and what stays loose next to the client
    let (grf, loose) = (final_dir.join("grf"), final_dir.join("loose"));
    make_dirs(&grf)?;
    make_dirs(&loose)?;
    let mut names: Vec<String> = fs::read_dir(&all)
        .ctx(|| format!("cannot read {}", all.display()))?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for e in names {
        let dest = if e.to_lowercase() == "data" {
            &grf
        } else {
            &loose
        };
        fs::rename(all.join(&e), dest.join(&e))
            .ctx(|| format!("cannot move {}", all.join(&e).display()))?;
    }
    fs::remove_dir(&all).ctx(|| format!("cannot remove {}", all.display()))?;
    log(&format!(
        "done: {} (for the GRF) and {} (loose files)",
        grf.display(),
        loose.display()
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_like_fnmatch() {
        assert!(fnmatch("data/sclientinfo.xml", "data/sclientinfo.xml"));
        assert!(fnmatch("data/*/petinfo.lub", "data/a/b/petinfo.lub"));
        assert!(fnmatch("data/?.bmp", "data/a.bmp"));
        assert!(fnmatch("data/[ab].bmp", "data/b.bmp"));
        assert!(!fnmatch("data/[!ab].bmp", "data/b.bmp"));
        assert!(!fnmatch("data/a.bmp", "data/a.bmp.bak"));
    }
}
