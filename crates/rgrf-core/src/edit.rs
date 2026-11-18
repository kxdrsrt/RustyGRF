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
