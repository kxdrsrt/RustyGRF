//! `edit`: write a changed copy of an archive (replace, add and remove files) without touching
//! the original.
//!
//! The new and replacement files go into a small temporary archive that is merged over the
//! source; removed paths are left out. Unchanged entries keep their original streams, so editing
//! a few files of a large archive does not recompress the rest.

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::Path;

use crate::archive::Archive;
use crate::container::flags;
use crate::encode::encode;
use crate::error::{Error, Result};
use crate::merge::{MergeReport, merge_excluding};
use crate::name::GrfPath;
use crate::pipeline;
use crate::progress::Control;
use crate::repack::RepackOptions;
use crate::writer::{Format, OutEntry, Writer};

#[derive(Debug, Clone)]
pub enum Edit {
    /// Replaces the file at `name`, or adds it when the archive has no such path.
    Put { name: GrfPath, data: Vec<u8> },
    /// Leaves the path out of the result.
    Remove { name: GrfPath },
}

#[derive(Debug, Clone)]
pub struct EditReport {
    pub merge: MergeReport,
    pub added: usize,
    pub replaced: usize,
    /// Removed paths the archive really had.
    pub removed: usize,
}

/// The archive path a user typed: `/` and `\\` both work, a leading separator is dropped, and the
/// text is turned into bytes with `mode`. Empty paths and `..` parts are refused.
pub fn archive_path(text: &str, mode: crate::name::NameMode) -> Result<GrfPath> {
    let norm = text.replace('/', "\\");
    let norm = norm.trim_start_matches('\\');
    if norm.is_empty() || norm.split('\\').any(|p| p.is_empty() || p == "..") {
        return Err(Error::InvalidInput(format!(
            "'{text}' is not a usable archive path"
        )));
    }
    GrfPath::from_display(norm, mode).ok_or_else(|| {
        Error::InvalidInput(format!(
            "'{text}' cannot be written in the name mode {mode:?}"
        ))
    })
}

/// Splits `ARCHIVE_PATH=FILE` at the first `=`.
pub fn split_put(spec: &str) -> Result<(&str, &str)> {
    spec.split_once('=')
        .filter(|(a, f)| !a.is_empty() && !f.is_empty())
        .ok_or_else(|| Error::InvalidInput(format!("'{spec}' is not ARCHIVE_PATH=FILE")))
}

/// Writes `dst` as `src` with the edits applied. For one path the last edit wins.
pub fn apply(
    src: &Archive,
    src_path: &Path,
    dst: &Path,
    edits: &[Edit],
    opts: &RepackOptions,
    ctl: &Control,
) -> Result<EditReport> {
    if let (Ok(a), Ok(b)) = (src_path.canonicalize(), dst.canonicalize())
        && a == b
    {
        return Err(Error::InvalidInput(
            "the output must not be the archive that is edited".into(),
        ));
    }
    let mut puts: HashMap<Vec<u8>, (GrfPath, &[u8])> = HashMap::new();
    let mut removes: HashSet<Vec<u8>> = HashSet::new();
    for e in edits {
        match e {
            Edit::Put { name, data } => {
                removes.remove(&name.key());
                puts.insert(name.key(), (name.clone(), data));
            }
            Edit::Remove { name } => {
                puts.remove(&name.key());
                removes.insert(name.key());
            }
        }
    }
    let in_src: HashSet<Vec<u8>> = src
        .entries()
        .iter()
        .filter(|e| !e.is_marker())
        .map(|e| e.name.key())
        .collect();
    let replaced = puts.keys().filter(|k| in_src.contains(*k)).count();
    let added = puts.len() - replaced;
    let removed = removes.iter().filter(|k| in_src.contains(*k)).count();

    if !opts.dry_run && dst.exists() && !opts.overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }
    let report = if puts.is_empty() {
        merge_excluding(&[src], dst, opts, false, &removes, ctl)?
    } else {
        let tmp = pipeline::temp_path(dst).with_extension("edits");
        let result = write_puts(&puts, &tmp, opts).and_then(|()| {
            let patch = Archive::open(&tmp)?;
            merge_excluding(&[&patch, src], dst, opts, false, &removes, ctl)
        });
        let _ = std::fs::remove_file(&tmp);
        result?
    };
    Ok(EditReport {
        merge: report,
        added,
        replaced,
        removed,
    })
}

/// A file compressed for [`write_packed`].
#[derive(Debug, Clone)]
pub struct Packed {
    pub name: GrfPath,
    pub size: u32,
    pub stream: Vec<u8>,
}

/// Compresses one file with `method` (can run in parallel; keeps only the compressed bytes).
pub fn pack(name: GrfPath, data: &[u8], method: crate::encode::Method) -> Result<Packed> {
    let size = u32::try_from(data.len()).map_err(|_| {
        Error::TooLarge(format!(
            "{} is larger than 4 GiB",
            name.display(Default::default())
        ))
    })?;
    Ok(Packed {
        name,
        size,
        stream: encode(method, data),
    })
}

/// Writes a new 0x200 archive that holds only `files` (a patch archive for the client to read
/// before the others), sorted by name; for one path the last file wins. Written to a temporary
/// file first, so a failure leaves no half archive. Returns the number of entries.
pub fn write_packed(files: &[Packed], dst: &Path, overwrite: bool) -> Result<usize> {
    if dst.exists() && !overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }
    let mut last: HashMap<Vec<u8>, &Packed> = HashMap::new();
    for f in files {
        last.insert(f.name.key(), f);
    }
    let mut sorted: Vec<&Packed> = last.into_values().collect();
    sorted.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
    let count = sorted.len();
    let tmp = pipeline::temp_path(dst);
    let written = (|| {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)?;
        let mut w = Writer::new(BufWriter::new(file), Format::V200, true, None)?;
        let mut entries = Vec::with_capacity(sorted.len());
        for f in sorted {
            entries.push(OutEntry {
                name: f.name.clone(),
                placed: w.put_stream(&f.stream)?,
                size: f.size,
                flags: flags::FILE,
            });
        }
        let (_, mut out) = w.finish(&entries)?;
        std::io::Write::flush(&mut out)?;
        drop(out);
        std::fs::rename(&tmp, dst)?;
        Ok(count)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

fn write_puts(
    puts: &HashMap<Vec<u8>, (GrfPath, &[u8])>,
    tmp: &Path,
    opts: &RepackOptions,
) -> Result<()> {
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(tmp)?;
    let mut w = Writer::new(BufWriter::new(file), Format::V200, true, None)?;
    let mut entries = Vec::with_capacity(puts.len());
    let mut sorted: Vec<_> = puts.values().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    for (name, data) in sorted {
        let size = u32::try_from(data.len()).map_err(|_| {
            Error::TooLarge(format!(
                "{} is larger than 4 GiB",
                name.display(Default::default())
            ))
        })?;
        let placed = w.put_stream(&encode(opts.method, data))?;
        entries.push(OutEntry {
            name: name.clone(),
            placed,
            size,
            flags: flags::FILE,
        });
    }
    let (_, mut out) = w.finish(&entries)?;
    std::io::Write::flush(&mut out)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::{BuildOptions, build};
    use std::fs;

    fn names(a: &Archive) -> Vec<String> {
        let mut v: Vec<String> = a
            .entries()
            .iter()
            .map(|e| e.name.display(Default::default()))
            .collect();
        v.sort();
        v
    }

    #[test]
    fn replaces_adds_and_removes_without_changing_the_source() {
        let dir = std::env::temp_dir().join(format!("rgrf-edit-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let root = dir.join("src/data");
        fs::create_dir_all(&root).unwrap();
        for (n, c) in [("a.txt", "old a"), ("b.txt", "b"), ("c.txt", "c")] {
            fs::write(root.join(n), c).unwrap();
        }
        let grf = dir.join("in.grf");
        build(
            &dir.join("src"),
            &grf,
            &BuildOptions::default(),
            &Control::default(),
        )
        .unwrap();
        let before = fs::read(&grf).unwrap();
        let arc = Archive::open(&grf).unwrap();
        let p = |s: &str| GrfPath::new(s.as_bytes().to_vec());
        let edits = [
            Edit::Put {
                name: p("DATA\\A.TXT"),
                data: b"new a".to_vec(),
            },
            Edit::Put {
                name: p("data\\d.txt"),
                data: b"d".to_vec(),
            },
            Edit::Remove {
                name: p("data\\b.txt"),
            },
            Edit::Remove {
                name: p("data\\missing.txt"),
            },
            // the last edit for a path wins
            Edit::Remove {
                name: p("data\\e.txt"),
            },
            Edit::Put {
                name: p("data\\e.txt"),
                data: b"e".to_vec(),
            },
        ];
        let out = dir.join("out.grf");
        let r = apply(
            &arc,
            &grf,
            &out,
            &edits,
            &RepackOptions::default(),
            &Control::default(),
        )
        .unwrap();
        assert_eq!((r.added, r.replaced, r.removed), (2, 1, 1));
        let res = Archive::open(&out).unwrap();
        assert_eq!(
            names(&res),
            ["data\\a.txt", "data\\c.txt", "data\\d.txt", "data\\e.txt"]
        );
        let read = |n: &str| {
            let i = res.find(&p(n)).unwrap();
            res.read(i).unwrap().data.to_vec()
        };
        assert_eq!(read("data\\a.txt"), b"new a");
        assert_eq!(read("data\\c.txt"), b"c");
        assert_eq!(read("data\\d.txt"), b"d");
        assert_eq!(fs::read(&grf).unwrap(), before, "the source is unchanged");
        assert!(!dir.join(".out.grf.rgrf-tmp.edits").exists());

        // only removals, and refusing to overwrite the source or an existing file
        let only = dir.join("only.grf");
        apply(
            &arc,
            &grf,
            &only,
            &[Edit::Remove {
                name: p("data\\a.txt"),
            }],
            &RepackOptions::default(),
            &Control::default(),
        )
        .unwrap();
        assert_eq!(names(&Archive::open(&only).unwrap()).len(), 2);
        assert!(
            apply(
                &arc,
                &grf,
                &grf,
                &edits,
                &RepackOptions {
                    overwrite: true,
                    ..Default::default()
                },
                &Control::default()
            )
            .is_err()
        );
        assert!(
            apply(
                &arc,
                &grf,
                &out,
                &edits,
                &RepackOptions::default(),
                &Control::default()
            )
            .is_err()
        );
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;
    use crate::name::NameMode;

    #[test]
    fn typed_archive_paths_are_normalised_and_checked() {
        let p = archive_path("/data/sprite/a.spr", NameMode::Cp1252).unwrap();
        assert_eq!(p.as_bytes(), b"data\\sprite\\a.spr");
        assert!(archive_path("data\\x.txt", NameMode::Cp1252).is_ok());
        for bad in ["", "/", "data//x", "data/../x", "data\\"] {
            assert!(archive_path(bad, NameMode::Cp1252).is_err(), "{bad}");
        }
        assert_eq!(
            split_put("data/a.txt=/tmp/a=b.txt").unwrap(),
            ("data/a.txt", "/tmp/a=b.txt")
        );
        assert!(
            split_put("nofile=").is_err() && split_put("=x").is_err() && split_put("x").is_err()
        );
    }
}
