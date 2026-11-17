//! Which paths the server may read and write.
//!
//! Reading works anywhere unless `--allow-read` roots are given. Writing is only possible below
//! the `--allow-write` roots; without any, the writing tools do not exist. Paths with `..` are
//! refused, symbolic links are resolved before the check, and existing files are never replaced
//! unless the caller says so.

use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Policy {
    read_roots: Vec<PathBuf>,
    write_roots: Vec<PathBuf>,
}

impl Policy {
    /// All roots must exist; they are stored in their canonical form.
    pub fn new(read_roots: &[PathBuf], write_roots: &[PathBuf]) -> Result<Self, String> {
        let canon = |p: &PathBuf| -> Result<PathBuf, String> {
            fs::canonicalize(p).map_err(|e| format!("{}: {e}", p.display()))
        };
        Ok(Self {
            read_roots: read_roots.iter().map(canon).collect::<Result<_, _>>()?,
            write_roots: write_roots.iter().map(canon).collect::<Result<_, _>>()?,
        })
    }

    pub fn write_enabled(&self) -> bool {
        !self.write_roots.is_empty()
    }

    pub fn write_roots(&self) -> &[PathBuf] {
        &self.write_roots
    }

    pub fn read_restricted(&self) -> bool {
        !self.read_roots.is_empty()
    }

    fn may_read(&self, p: &Path) -> bool {
        self.read_roots.is_empty()
            || self
                .read_roots
                .iter()
                .chain(&self.write_roots)
                .any(|r| p.starts_with(r))
    }

    /// An existing file or folder that may be read.
    pub fn read(&self, path: &str) -> Result<PathBuf, String> {
        let p = absolute(path)?;
        let c = fs::canonicalize(&p).map_err(|e| format!("{path}: {e}"))?;
        if !self.may_read(&c) {
            return Err(format!(
                "{path}: reading is not allowed here (see --allow-read)"
            ));
        }
        Ok(c)
    }

    /// A path below a write root that is not an existing symlink; for archives to create.
    /// Fails if it exists and `overwrite` is false (folders are never replaced).
    pub fn write_file(&self, path: &str, overwrite: bool) -> Result<PathBuf, String> {
        let full = self.write_path(path)?;
        match fs::symlink_metadata(&full) {
            Ok(m) if m.file_type().is_symlink() => {
                Err(format!("{path}: is a symbolic link, refusing to write"))
            }
            Ok(m) if m.is_dir() => Err(format!("{path}: is a folder")),
            Ok(_) if !overwrite => Err(format!(
                "{path}: already exists (pass overwrite=true to replace it)"
            )),
            _ => Ok(full),
        }
    }

    /// A path below a write root for extracted files (may exist as a folder).
    pub fn write_dir(&self, path: &str) -> Result<PathBuf, String> {
        let full = self.write_path(path)?;
        match fs::symlink_metadata(&full) {
            Ok(m) if m.file_type().is_symlink() => {
                Err(format!("{path}: is a symbolic link, refusing to write"))
            }
            Ok(m) if !m.is_dir() => Err(format!("{path}: exists and is not a folder")),
            _ => Ok(full),
        }
    }

    fn write_path(&self, path: &str) -> Result<PathBuf, String> {
        if !self.write_enabled() {
            return Err(
                "writing is disabled (start the server with --allow-write <folder>)".into(),
            );
        }
        let p = absolute(path)?;
        // the longest existing ancestor is resolved (symlinks!), the rest must be plain names
        let mut base = p.as_path();
        let mut rest: Vec<&std::ffi::OsStr> = Vec::new();
        while !base.exists() {
            let (Some(parent), Some(name)) = (base.parent(), base.file_name()) else {
                return Err(format!("{path}: no existing parent folder"));
            };
            rest.push(name);
            base = parent;
        }
        let mut full = fs::canonicalize(base).map_err(|e| format!("{path}: {e}"))?;
        full.extend(rest.iter().rev());
        if !self.write_roots.iter().any(|r| full.starts_with(r)) {
            let roots: Vec<String> = self
                .write_roots
                .iter()
                .map(|r| r.display().to_string())
                .collect();
            return Err(format!(
                "{path}: outside the folders allowed for writing ({})",
                roots.join(", ")
            ));
        }
        if full.file_name().is_none() {
            return Err(format!("{path}: not a file or folder path"));
        }
        Ok(full)
    }
}

/// Absolute form without touching the file system; `..` is refused instead of resolved.
fn absolute(path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    let p = Path::new(path);
    if p.components().any(|c| c == Component::ParentDir) {
        return Err(format!("{path}: '..' is not allowed in paths"));
    }
    if p.is_absolute() {
        Ok(p.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|d| d.join(p))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("rgrf-mcp-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (w, o) = (base.join("write"), base.join("other"));
        fs::create_dir_all(&w).unwrap();
        fs::create_dir_all(&o).unwrap();
        (base, w, o)
    }

    #[test]
    fn writing_is_off_without_a_root_and_reading_is_open() {
        let (base, _, o) = dirs("off");
        let p = Policy::new(&[], &[]).unwrap();
        assert!(!p.write_enabled());
        assert!(
            p.write_file(o.join("a.grf").to_str().unwrap(), false)
                .is_err()
        );
        assert!(p.read(o.to_str().unwrap()).is_ok());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn writes_stay_inside_the_roots() {
        let (base, w, o) = dirs("roots");
        let p = Policy::new(&[], std::slice::from_ref(&w)).unwrap();
        let canon_w = fs::canonicalize(&w).unwrap();
        // new files and new sub folders below the root are fine
        let ok = p
            .write_file(w.join("sub/new.grf").to_str().unwrap(), false)
            .unwrap();
        assert_eq!(ok, canon_w.join("sub/new.grf"));
        // outside, with `..`, and empty paths are not
        let outside = p
            .write_file(o.join("x.grf").to_str().unwrap(), false)
            .unwrap_err();
        assert!(outside.contains("outside"), "{outside}");
        let dots = format!("{}/../other/x.grf", w.display());
        assert!(p.write_file(&dots, false).unwrap_err().contains("'..'"));
        assert!(p.write_file("", false).is_err());
        // existing files need overwrite=true, folders never
        fs::write(w.join("have.grf"), b"x").unwrap();
        let have = w.join("have.grf");
        assert!(
            p.write_file(have.to_str().unwrap(), false)
                .unwrap_err()
                .contains("already exists")
        );
        assert!(p.write_file(have.to_str().unwrap(), true).is_ok());
        assert!(
            p.write_file(w.to_str().unwrap(), true)
                .unwrap_err()
                .contains("folder")
        );
        // extraction folders may exist
        assert!(p.write_dir(w.to_str().unwrap()).is_ok());
        assert!(p.write_dir(have.to_str().unwrap()).is_err());
        fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_cannot_lead_out_of_the_write_root() {
        let (base, w, o) = dirs("link");
        let p = Policy::new(&[], std::slice::from_ref(&w)).unwrap();
        std::os::unix::fs::symlink(&o, w.join("escape")).unwrap();
        let via_dir = w.join("escape/x.grf");
        assert!(
            p.write_file(via_dir.to_str().unwrap(), false)
                .unwrap_err()
                .contains("outside")
        );
        fs::write(o.join("t.grf"), b"x").unwrap();
        std::os::unix::fs::symlink(o.join("t.grf"), w.join("link.grf")).unwrap();
        let link = w.join("link.grf");
        let err = p.write_file(link.to_str().unwrap(), true).unwrap_err();
        assert!(err.contains("outside") || err.contains("symbolic"), "{err}");
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn read_roots_limit_reading_but_include_the_write_roots() {
        let (base, w, o) = dirs("read");
        let p = Policy::new(std::slice::from_ref(&o), std::slice::from_ref(&w)).unwrap();
        assert!(p.read(o.to_str().unwrap()).is_ok());
        assert!(p.read(w.to_str().unwrap()).is_ok());
        let elsewhere = std::env::temp_dir();
        assert!(
            p.read(elsewhere.to_str().unwrap())
                .unwrap_err()
                .contains("not allowed")
        );
        assert!(p.read(o.join("missing").to_str().unwrap()).is_err());
        fs::remove_dir_all(base).unwrap();
    }
}
