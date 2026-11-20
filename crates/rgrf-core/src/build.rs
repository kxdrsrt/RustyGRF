//! `build`: create an archive from a directory tree.
//!
//! The given folder is the archive root: `<folder>/data/sprite/a.spr` becomes `data\sprite\a.spr`
//! (the same convention as GrfCL and the extracted folders of `rgrf extract`). Names on disk are
//! turned back into archive bytes with the chosen [`NameMode`] (default CP1252, the inverse of
//! `extract`), after Unicode normalization to NFC.

use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Read};
use std::path::{Path, PathBuf};
use unicode_normalization::UnicodeNormalization;

use crate::archive::Archive;
use crate::container::flags;
use crate::encode::{Method, encode};
use crate::error::{Error, Result};
use crate::name::{GrfPath, NameMode};
use crate::pipeline::{self, Expected, Kind, Prepared, sha};
use crate::progress::Control;
use crate::writer::{Format, OutEntry, Placed, Writer};

#[derive(Debug, Clone)]
pub struct BuildOptions {
    pub method: Method,
    pub format: Format,
    /// `None` uses the format default (see [`Format::aligns_by_default`]).
    pub align: Option<bool>,
    /// Let files with identical content share one data block.
    pub dedupe: bool,
    /// How file names on disk map to archive bytes.
    pub names: NameMode,
    /// Decode the written archive and compare every file with the one on disk.
    pub verify: bool,
    pub overwrite: bool,
    /// Only scan and plan, write nothing.
    pub dry_run: bool,
    /// Override of the format's size limit (tests).
    pub max_file_len: Option<u64>,
    /// Encrypt all files with GRF Editor's scheme (0x200 only) and mark the archive.
    pub encrypt: Option<crate::grfcrypt::Key>,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            method: Method::DEFAULT,
            format: Format::V200,
            align: None,
            dedupe: false,
            names: NameMode::default(),
            verify: true,
            overwrite: false,
            dry_run: false,
            max_file_len: None,
            encrypt: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Default)]
pub struct BuildReport {
    pub format: u32,
    pub files: usize,
    pub blocks: usize,
    pub shared_files: usize,
    pub unpacked_bytes: u64,
    pub output_stream_bytes: u64,
    pub output_file_len: u64,
    pub verified: bool,
    pub dry_run: bool,
    pub skipped: Vec<Skipped>,
}

struct Item {
    path: PathBuf,
    name: GrfPath,
    key: Vec<u8>,
    size: u64,
}

/// Files that are never part of an archive (file manager droppings).
fn junk(name: &str) -> bool {
    matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini") || name.starts_with("._")
}

fn scan(root: &Path, names: NameMode, skipped: &mut Vec<Skipped>) -> Result<Vec<Item>> {
    let mut items = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    let mut stack = vec![(root.to_path_buf(), Vec::<String>::new())];
    while let Some((dir, rel)) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(fs::DirEntry::file_name);
        for e in entries {
            let path = e.path();
            let file_name = e.file_name();
            let Some(name) = file_name.to_str() else {
                bad.push(format!("{} (not valid Unicode)", path.display()));
                continue;
            };
            let ft = e.file_type()?;
            if ft.is_symlink() {
                skipped.push(Skipped {
                    path,
                    reason: "symbolic link",
                });
                continue;
            }
            let component: String = name.nfc().collect();
            let mut rel_next = rel.clone();
            rel_next.push(component);
            if ft.is_dir() {
                stack.push((path, rel_next));
                continue;
            }
            if !ft.is_file() {
                skipped.push(Skipped {
                    path,
                    reason: "not a regular file",
                });
                continue;
            }
            if junk(name) {
                skipped.push(Skipped {
                    path,
                    reason: "file manager metadata",
                });
                continue;
            }
            let size = e.metadata()?.len();
            if size > u64::from(u32::MAX) {
                return Err(Error::InvalidInput(format!(
                    "{} is {size} bytes; a single file must be smaller than 4 GiB",
                    path.display()
                )));
            }
            let joined = rel_next.join("\\");
            match GrfPath::from_display(&joined, names) {
                Some(name) => items.push(Item {
                    key: name.key(),
                    name,
                    path,
                    size,
                }),
                None => bad.push(format!("{joined} (not representable in {names:?})")),
            }
        }
    }
    if !bad.is_empty() {
        bad.sort();
        return Err(Error::InvalidInput(format!(
            "{} file names cannot be turned into archive names, e.g. {} (try --names)",
            bad.len(),
            bad.iter().take(3).cloned().collect::<Vec<_>>().join("; ")
        )));
    }
    if items.is_empty() {
        return Err(Error::InvalidInput(format!(
            "no files found in {}",
            root.display()
        )));
    }
    items.sort_by(|a, b| a.key.cmp(&b.key).then_with(|| a.name.cmp(&b.name)));
    let clashes: Vec<String> = items
        .windows(2)
        .filter(|w| w[0].key == w[1].key)
        .map(|w| format!("{} / {}", w[0].path.display(), w[1].path.display()))
        .collect();
    if !clashes.is_empty() {
        return Err(Error::InvalidInput(format!(
            "{} files would get the same archive path (names are compared case-insensitively): {}",
            clashes.len(),
            clashes
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        )));
    }
    Ok(items)
}

/// Files of a folder with their archive paths, and what was left out.
pub type ScannedFolder = (Vec<(GrfPath, PathBuf)>, Vec<Skipped>);

/// The files below `root` with the archive path each would get (the folder is the archive root),
/// in path order; symbolic links and file manager droppings are listed in the second value.
pub fn scan_folder(root: &Path, names: NameMode) -> Result<ScannedFolder> {
    let mut skipped = Vec::new();
    let items = scan(root, names, &mut skipped)?;
    Ok((
        items.into_iter().map(|i| (i.name, i.path)).collect(),
        skipped,
    ))
}

fn hash_file(path: &Path) -> Result<(u64, [u8; 32])> {
    use sha2::{Digest as _, Sha256};
    let mut f = File::open(path)?;
    let (mut h, mut buf, mut n) = (Sha256::new(), vec![0u8; 1 << 20], 0u64);
    loop {
        let k = f.read(&mut buf)?;
        if k == 0 {
            break;
        }
        h.update(&buf[..k]);
        n += k as u64;
    }
    Ok((n, h.finalize().into()))
}

struct Unit {
    members: Vec<usize>,
    hash: Option<[u8; 32]>,
}

fn plan_units(items: &[Item], opts: &BuildOptions, ctl: &Control) -> Result<Vec<Unit>> {
    // 0x102/0x103 scramble data depending on the name, so blocks cannot be shared
    if !opts.dedupe || opts.format.is_legacy() {
        return Ok((0..items.len())
            .map(|i| Unit {
                members: vec![i],
                hash: None,
            })
            .collect());
    }
    let hashes: Vec<Result<(u64, [u8; 32])>> = items
        .par_iter()
        .map(|it| {
            ctl.check()?;
            hash_file(&it.path)
        })
        .collect();
    let mut units: Vec<Unit> = Vec::new();
    let mut seen: HashMap<([u8; 32], u64), usize> = HashMap::new();
    for (i, h) in hashes.into_iter().enumerate() {
        let (n, h) = h?;
        if n != items[i].size {
            return Err(Error::InvalidInput(format!(
                "{} changed while building",
                items[i].path.display()
            )));
        }
        match seen.get(&(h, n)) {
            Some(&u) => units[u].members.push(i),
            None => {
                seen.insert((h, n), units.len());
                units.push(Unit {
                    members: vec![i],
                    hash: Some(h),
                });
            }
        }
    }
    Ok(units)
}

fn prepare(items: &[Item], u: usize, unit: &Unit, opts: &BuildOptions) -> Result<Prepared> {
    let it = &items[unit.members[0]];
    let data = fs::read(&it.path)?;
    if data.len() as u64 != it.size {
        return Err(Error::InvalidInput(format!(
            "{} changed while building",
            it.path.display()
        )));
    }
    let expected = if opts.verify {
        Expected::Data(unit.hash.unwrap_or_else(|| sha(&data)), 0)
    } else {
        Expected::Nothing
    };
    Ok(Prepared {
        unit: u,
        kind: Kind::Encoded,
        stream: encode(opts.method, &data),
        raw: None,
        flags: flags::FILE,
        expected,
    })
}

pub fn build(
    src_dir: &Path,
    dst: &Path,
    opts: &BuildOptions,
    ctl: &Control,
) -> Result<BuildReport> {
    let root = src_dir.canonicalize()?;
    if !root.is_dir() {
        return Err(Error::InvalidInput(format!(
            "{} is not a folder",
            src_dir.display()
        )));
    }
    if let Some(parent) = dst.canonicalize().ok().or_else(|| {
        dst.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()
            .ok()
    }) && parent.starts_with(&root)
    {
        return Err(Error::InvalidInput(
            "the output must not lie inside the folder that is packed".into(),
        ));
    }
    if !opts.dry_run && dst.exists() && !opts.overwrite {
        return Err(Error::TargetExists(dst.display().to_string()));
    }

    let mut report = BuildReport {
        format: opts.format.version(),
        dry_run: opts.dry_run,
        ..BuildReport::default()
    };
    let items = scan(&root, opts.names, &mut report.skipped)?;
    let units = plan_units(&items, opts, ctl)?;
    report.files = items.len();
    report.blocks = units.len();
    report.shared_files = items.len() - units.len();
    report.unpacked_bytes = items.iter().map(|i| i.size).sum();
    if opts.dry_run {
        return Ok(report);
    }

    let tmp = pipeline::temp_path(dst);
    match write_all(&items, &units, &tmp, opts, ctl, &mut report) {
        Ok(()) => {
            pipeline::commit(&tmp, dst, opts.overwrite)?;
            Ok(report)
        }
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write_all(
    items: &[Item],
    units: &[Unit],
    tmp: &Path,
    opts: &BuildOptions,
    ctl: &Control,
    report: &mut BuildReport,
) -> Result<()> {
    if opts.encrypt.is_some() && opts.format != Format::V200 {
        return Err(Error::Unsupported(
            "the encryption works with the 0x200 format only (as in GRF Editor)".into(),
        ));
    }
    let align = opts
        .align
        .unwrap_or_else(|| opts.format.aligns_by_default())
        || opts.encrypt.is_some();
    let file = OpenOptions::new().write(true).create_new(true).open(tmp)?;
    let writer = Writer::new(
        BufWriter::with_capacity(1 << 20, file),
        opts.format,
        align,
        opts.max_file_len,
    )?;
    let mut stream_bytes = 0u64;
    let (mut writer, done) = pipeline::run(
        writer,
        pipeline::batches(units.iter().map(|u| items[u.members[0]].size)),
        units.len() as u64,
        ctl,
        &|u| prepare(items, u, &units[u], opts),
        &|u| {
            let it = &items[units[u].members[0]];
            (it.name.clone(), it.size as u32)
        },
        opts.encrypt
            .as_ref()
            .map(|key| pipeline::Encrypt { key, only: None }),
        &mut |batch| stream_bytes += batch.iter().map(|p| p.stream.len() as u64).sum::<u64>(),
    )?;
    let mut placed: Vec<Option<Placed>> = vec![None; units.len()];
    let mut expected: Vec<Expected> = vec![Expected::Nothing; units.len()];
    for (u, pl, _, exp) in done {
        placed[u] = Some(pl);
        expected[u] = exp;
    }
    let mut unit_of = vec![0usize; items.len()];
    for (u, unit) in units.iter().enumerate() {
        for &m in &unit.members {
            unit_of[m] = u;
        }
    }
    let mut entries: Vec<OutEntry> = items
        .iter()
        .enumerate()
        .map(|(i, it)| OutEntry {
            name: it.name.clone(),
            placed: placed[unit_of[i]].expect("every unit was written"),
            size: it.size as u32,
            flags: flags::FILE,
        })
        .collect();
    if let Some(key) = &opts.encrypt {
        entries.push(pipeline::encryption_marker(&mut writer, key)?);
    }
    let (len, mut out) = writer.finish(&entries)?;
    std::io::Write::flush(&mut out)?;
    drop(out);
    report.output_file_len = len;
    report.output_stream_bytes = stream_bytes;
    if opts.verify {
        verify_built(
            items,
            units,
            &unit_of,
            &expected,
            tmp,
            opts.encrypt.as_ref(),
            ctl,
        )?;
        report.verified = true;
    }
    Ok(())
}

fn verify_built(
    items: &[Item],
    units: &[Unit],
    unit_of: &[usize],
    expected: &[Expected],
    path: &Path,
    key: Option<&crate::grfcrypt::Key>,
    ctl: &Control,
) -> Result<()> {
    let fail = |m: String| Err(Error::VerifyFailed(m));
    let out = match key {
        Some(k) => Archive::open_with_key(path, k)?,
        None => Archive::open(path)?,
    };
    let extra = usize::from(key.is_some());
    if out.len() != items.len() + extra {
        return fail(format!(
            "{} entries written, {} expected",
            out.len(),
            items.len() + extra
        ));
    }
    for (i, (it, e)) in items.iter().zip(out.entries()).enumerate() {
        if it.name != e.name || u64::from(e.size) != it.size || !e.is_file() {
            return fail(format!(
                "entry {i} ({}) differs in name, size or kind",
                it.path.display()
            ));
        }
        let rep = units[unit_of[i]].members[0];
        if rep != i
            && (e.offset != out.entries()[rep].offset || e.zsize != out.entries()[rep].zsize)
        {
            return fail(format!("entry {i} should share the block of entry {rep}"));
        }
    }
    let problems: Vec<Result<Option<String>>> = (0..units.len())
        .into_par_iter()
        .map(|u| {
            ctl.check()?;
            let rep = units[u].members[0];
            Ok(match &expected[u] {
                Expected::Data(h, _) => {
                    let d = out.read(rep)?;
                    (!d.status.is_ok() || sha(&d.data) != *h).then(|| {
                        format!(
                            "{}: content differs after building ({:?})",
                            items[rep].path.display(),
                            d.status
                        )
                    })
                }
                _ => None,
            })
        })
        .collect();
    for p in problems {
        if let Some(msg) = p? {
            return fail(msg);
        }
    }
    Ok(())
}
