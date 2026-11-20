//! Saving an archive with its pending changes in any container (GRF/GPF, Thor, RGZ), with the
//! choices of GRF Editor's "Save advanced"; new empty archives; hashes of an entry; comparing two
//! archives and writing their differences as a patch.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rgrf_core::edit::{self, Edit};
use rgrf_core::encode::Method;
use rgrf_core::hash::Digest;
use rgrf_core::progress::Control;
use rgrf_core::repack::RepackOptions;
use rgrf_core::thor::{self, ThorFile, ThorOptions};
use rgrf_core::writer::{Format, Writer};
use rgrf_core::{Archive, DecodeStatus, GrfPath, rgz};
use serde::{Deserialize, Serialize};

use crate::backend::{ArchiveInfo, EditPlan, EditSummary, Session};

/// How an archive is written ("Save as", "Save advanced"). Every choice left out keeps what the
/// archive has.
#[derive(Deserialize, Debug, Clone, Default)]
pub struct SaveRequest {
    pub dst: PathBuf,
    /// Encode every file again with `method` instead of copying the stored streams.
    #[serde(default)]
    pub recompress: bool,
    /// `zlib`, `zopfli` or `lzma`; used with `recompress` and for added files.
    #[serde(default)]
    pub method: Option<String>,
    /// `0x200`, `0x300`, `0x103` or `0x102`; none keeps the version.
    #[serde(default)]
    pub format: Option<String>,
    /// `keep` (as opened), `none` (write it plain) or `password` (encrypt with `password`).
    #[serde(default)]
    pub encryption: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

/// The container a file name asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Grf,
    Thor,
    Rgz,
}

pub fn container_of(path: &Path) -> Container {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("thor") => Container::Thor,
        Some("rgz") => Container::Rgz,
        _ => Container::Grf,
    }
}

fn method_of(name: Option<&str>) -> Result<Method, String> {
    Ok(match name.unwrap_or("zlib") {
        "zlib" => Method::DEFAULT,
        "zopfli" => Method::Zopfli { iterations: 10 },
        "lzma" => Method::Lzma { preset: 9 },
        other => return Err(format!("unknown method '{other}'")),
    })
}

/// The key the result is encrypted with.
fn key_of(plan: &EditPlan, req: &SaveRequest) -> Result<Option<rgrf_core::grfcrypt::Key>, String> {
    match req.encryption.as_deref().unwrap_or("keep") {
        "keep" => Ok(plan.key),
        "none" => Ok(None),
        "password" => {
            let p = req.password.as_deref().unwrap_or("");
            if p.is_empty() {
                return Err(err!("err-password-empty"));
            }
            rgrf_core::grfcrypt::key_from_password(p)
                .map(Some)
                .map_err(|e| e.to_string())
        }
        other => Err(format!("unknown encryption choice '{other}'")),
    }
}

/// Writes the archive of `plan` with its changes to `req.dst`.
pub fn save_as(
    plan: &EditPlan,
    req: &SaveRequest,
    overwrite: bool,
    ctl: &Control,
) -> Result<EditSummary, String> {
    let src = plan.open()?;
    let edits = plan.edits()?;
    let key = key_of(plan, req)?;
    let method = method_of(req.method.as_deref())?;
    match container_of(&req.dst) {
        Container::Grf => {
            let format = match req.format.as_deref() {
                None | Some("") | Some("keep") => None,
                Some(f) => Some(Format::parse(f).ok_or_else(|| format!("unknown format '{f}'"))?),
            };
            if key.is_some() && format.is_some_and(|f| f != Format::V200) {
                return Err(err!("err-encrypt-format"));
            }
            let opts = RepackOptions {
                overwrite,
                method,
                format,
                copy_streams: !req.recompress,
                recompress_all: req.recompress,
                encrypt: key,
                ..RepackOptions::default()
            };
            let r = edit::apply(&src, &plan.src, &req.dst, &edits, &opts, ctl)
                .map_err(|e| e.to_string())?;
            Ok(EditSummary {
                added: r.added,
                replaced: r.replaced,
                removed: r.removed,
                files: r.merge.repack.files,
                file_bytes: r.merge.repack.output_file_len,
                verified: r.merge.repack.verified,
            })
        }
        kind => {
            if key.is_some() {
                return Err(err!("err-encrypt-format"));
            }
            let (files, counts) = final_files(&src, &edits, ctl)?;
            if files.is_empty() {
                return Err(err!("err-patch-empty"));
            }
            if kind == Container::Thor {
                let list: Vec<ThorFile> = files
                    .iter()
                    .map(|(n, d)| ThorFile {
                        name: n.clone(),
                        data: Some(d.clone()),
                    })
                    .collect();
                let opts = ThorOptions {
                    method,
                    overwrite,
                    ..ThorOptions::default()
                };
                thor::write(&req.dst, &list, &opts).map_err(|e| e.to_string())?;
            } else {
                rgz::write(&req.dst, &files, 9, overwrite).map_err(|e| e.to_string())?;
            }
            let verified = same_content(&req.dst, &files)?;
            Ok(EditSummary {
                added: counts.0,
                replaced: counts.1,
                removed: counts.2,
                files: files.len(),
                file_bytes: std::fs::metadata(&req.dst).map(|m| m.len()).unwrap_or(0),
                verified,
            })
        }
    }
}

#[derive(Serialize, Debug)]
pub struct SavedInPlace {
    pub summary: EditSummary,
    pub info: ArchiveInfo,
}

/// "Save": writes the archive of the session in `slot` with its changes (and the choices of
/// `req`; its `dst` is ignored) next to it, checks it, puts it in place of the archive and opens
/// it again. The old file is only replaced by a complete and verified new one; the session is
/// closed for the moment of the swap (Windows cannot replace an open file).
pub fn save_in_place(
    slot: &std::sync::Mutex<Option<Session>>,
    mut req: SaveRequest,
    ctl: &Control,
) -> Result<SavedInPlace, String> {
    let plan = slot
        .lock()
        .map_err(|_| "internal error")?
        .as_ref()
        .ok_or_else(|| err!("err-no-archive"))?
        .plan();
    let path = plan.src.clone();
    // the temporary file keeps the extension, so it is written in the archive's own container
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().into_owned())
        .unwrap_or_else(|| "grf".into());
    let tmp = path.with_file_name(format!(".{name}.rgrf-saving.{ext}"));
    req.dst = tmp.clone();
    let new_key = key_of(&plan, &req)?;
    let summary = match save_as(&plan, &req, true, ctl) {
        Ok(s) if s.verified => s,
        Ok(_) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(err!("err-save-unverified"));
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let mut slot = slot.lock().map_err(|_| "internal error")?;
    let drafts = slot.take().and_then(|mut old| {
        // the changes are in the file now: the draft goes with them
        old.discard_pending();
        old.draft_root()
    });
    let renamed = std::fs::rename(&tmp, &path);
    // after a failed rename the old file is still there, with the old key
    let key = if renamed.is_ok() { new_key } else { plan.key };
    let mut s = Session::open_with(&path, key.as_ref())?;
    s.set_draft_root(drafts);
    let info = s.info();
    *slot = Some(s);
    renamed.map_err(|e| {
        err!("err-save-replace", "path": path.display().to_string(), "tmp": tmp.display().to_string(), "error": e.to_string())
    })?;
    Ok(SavedInPlace { summary, info })
}

/// Every file of `src` after the edits, in archive order (new files at the end), with the numbers
/// of added, replaced and removed files.
#[allow(clippy::type_complexity)]
fn final_files(
    src: &Archive,
    edits: &[Edit],
    ctl: &Control,
) -> Result<(Vec<(GrfPath, Vec<u8>)>, (usize, usize, usize)), String> {
    // the last edit of a path wins
    let mut last: HashMap<Vec<u8>, &Edit> = HashMap::new();
    let mut order: Vec<Vec<u8>> = Vec::new();
    for e in edits {
        let name = match e {
            Edit::Put { name, .. } | Edit::Remove { name } => name,
        };
        if last.insert(name.key(), e).is_none() {
            order.push(name.key());
        }
    }
    let (mut added, mut replaced, mut removed) = (0, 0, 0);
    let mut out = Vec::new();
    let mut seen: HashMap<Vec<u8>, usize> = HashMap::new();
    let total = src.len() as u64;
    for (i, entry) in src.entries().iter().enumerate() {
        ctl.check().map_err(|e| e.to_string())?;
        ctl.report(i as u64, total);
        // empty markers and delete directives carry no file
        if entry.is_marker() {
            continue;
        }
        let key = entry.name.key();
        let item = match last.get(&key) {
            Some(Edit::Remove { .. }) => {
                removed += 1;
                continue;
            }
            Some(Edit::Put { name, data }) => {
                replaced += 1;
                (name.clone(), data.clone())
            }
            None => {
                let d = src.read(i).map_err(|e| e.to_string())?;
                if d.status != DecodeStatus::Ok {
                    return Err(format!(
                        "{}: cannot be read ({:?})",
                        entry.name.display(rgrf_core::NameMode::Cp949),
                        d.status
                    ));
                }
                (entry.name.clone(), d.data)
            }
        };
        // an archive that has a path twice keeps the later entry, as the client reads it
        match seen.get(&key) {
            Some(&at) => out[at] = item,
            None => {
                seen.insert(key, out.len());
                out.push(item);
            }
        }
    }
    for key in order {
        if seen.contains_key(&key) {
            continue;
        }
        if let Some(Edit::Put { name, data }) = last.get(&key) {
            added += 1;
            out.push((name.clone(), data.clone()));
        }
    }
    Ok((out, (added, replaced, removed)))
}

/// Opens the written patch and compares every file with what should be in it.
fn same_content(dst: &Path, files: &[(GrfPath, Vec<u8>)]) -> Result<bool, String> {
    let a = Archive::open(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    let extra = usize::from(a.find(&GrfPath::new(*b"data.integrity")).is_some());
    if a.entries().iter().filter(|e| !e.is_marker()).count() != files.len() + extra {
        return Ok(false);
    }
    for (name, data) in files {
        let Some(i) = a.find(name) else {
            return Ok(false);
        };
        let d = a.read(i).map_err(|e| e.to_string())?;
        if d.status != DecodeStatus::Ok || d.data != *data {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The file name of a new archive of `kind`; the page knows such a file as unsaved by this name.
pub fn untitled_name(kind: &str, stamp: u128) -> Result<String, String> {
    match kind {
        "grf" | "gpf" | "rgz" | "thor" => Ok(format!("untitled-{stamp}.{kind}")),
        other => Err(format!("unknown archive type '{other}'")),
    }
}

/// Writes an empty GRF (0x200) to open as a new archive.
pub fn create_empty(dst: &Path) -> Result<(), String> {
    let f = std::fs::File::create(dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    let w = Writer::new(std::io::BufWriter::new(f), Format::V200, true, None)
        .map_err(|e| e.to_string())?;
    w.finish(&[]).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize, Debug)]
pub struct Hashes {
    pub size: usize,
    pub crc32: String,
    pub md5: String,
    pub sha256: String,
}

/// CRC32 and MD5 (as GRF Editor's hash viewer shows them) and SHA-256 of content.
pub fn hashes(data: &[u8]) -> Hashes {
    use sha2::Digest as _;
    let d = Digest::of(data);
    let sha = sha2::Sha256::digest(data);
    Hashes {
        size: data.len(),
        crc32: d.crc32_hex(),
        md5: d.md5_hex(),
        sha256: sha.iter().map(|b| format!("{b:02x}")).collect(),
    }
}

#[derive(Serialize, Debug)]
pub struct Comparison {
    /// The other archive.
    pub other: String,
    /// Paths only in the open archive (new compared with the other).
    pub only_here: Vec<String>,
    /// Paths only in the other archive (gone in the open one).
    pub only_there: Vec<String>,
    pub changed: Vec<String>,
    pub same: usize,
    /// The full counts (the lists stop at a few thousand paths).
    pub only_here_total: usize,
    pub only_there_total: usize,
    pub changed_total: usize,
}

/// Most paths of each list sent to the page.
const LIST_MAX: usize = 2000;

/// Compares the archive of `plan` (as saved, without its pending changes) with `other`.
pub fn compare_plan(
    plan: &EditPlan,
    other: &Path,
    mode: rgrf_core::NameMode,
    ctl: &Control,
) -> Result<Comparison, String> {
    compare(&plan.src, plan.key.as_ref(), other, mode, ctl)
}

/// Writes what changed from `old` to the archive of `plan` as a patch (see [`make_patch`]).
pub fn patch_plan(
    plan: &EditPlan,
    old: &Path,
    dst: &Path,
    method: Option<&str>,
    ctl: &Control,
) -> Result<PatchSummary, String> {
    make_patch(&plan.src, plan.key.as_ref(), old, dst, method, ctl)
}

/// Compares the open archive (`here`) with `other` by path and decoded content.
pub fn compare(
    here: &Path,
    key: Option<&rgrf_core::grfcrypt::Key>,
    other: &Path,
    mode: rgrf_core::NameMode,
    ctl: &Control,
) -> Result<Comparison, String> {
    let a = open(here, key)?;
    let b = Archive::open(other).map_err(|e| format!("{}: {e}", other.display()))?;
    let r = rgrf_core::merge::diff(&a, &b, ctl).map_err(|e| e.to_string())?;
    let names = |v: &[GrfPath]| -> Vec<String> {
        v.iter().take(LIST_MAX).map(|n| n.display(mode)).collect()
    };
    Ok(Comparison {
        other: other.display().to_string(),
        only_here: names(&r.only_a),
        only_there: names(&r.only_b),
        changed: names(&r.changed),
        same: r.same,
        only_here_total: r.only_a.len(),
        only_there_total: r.only_b.len(),
        changed_total: r.changed.len(),
    })
}

fn open(path: &Path, key: Option<&rgrf_core::grfcrypt::Key>) -> Result<Archive, String> {
    match key {
        Some(k) => Archive::open_with_key(path, k),
        None => Archive::open(path),
    }
    .map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Serialize, Debug)]
pub struct PatchSummary {
    pub added: usize,
    pub changed: usize,
    /// Paths the old archive has and the new one not: a Thor patch deletes them, a GRF cannot.
    pub removed: usize,
    pub file_bytes: u64,
}

/// Writes what changed from `old` to the open archive (`new`) as a patch: Thor (with delete
/// directives for removed paths) or GRF, by the extension of `dst`.
pub fn make_patch(
    new: &Path,
    key: Option<&rgrf_core::grfcrypt::Key>,
    old: &Path,
    dst: &Path,
    method: Option<&str>,
    ctl: &Control,
) -> Result<PatchSummary, String> {
    let n = open(new, key)?;
    let o = Archive::open(old).map_err(|e| format!("{}: {e}", old.display()))?;
    let method = method_of(method)?;
    if container_of(dst) == Container::Thor {
        let r = rgrf_core::merge::diff(&o, &n, ctl).map_err(|e| e.to_string())?;
        let mut files = Vec::new();
        for name in r.only_b.iter().chain(&r.changed) {
            let i = n.find(name).ok_or("internal error: path vanished")?;
            let d = n.read(i).map_err(|e| e.to_string())?;
            files.push(ThorFile {
                name: name.clone(),
                data: Some(d.data),
            });
        }
        for name in &r.only_a {
            files.push(ThorFile {
                name: name.clone(),
                data: None,
            });
        }
        if files.is_empty() {
            return Err(err!("err-patch-empty"));
        }
        let opts = ThorOptions {
            method,
            overwrite: true,
            integrity: true,
            ..ThorOptions::default()
        };
        thor::write(dst, &files, &opts).map_err(|e| e.to_string())?;
        return Ok(PatchSummary {
            added: r.only_b.len(),
            changed: r.changed.len(),
            removed: r.only_a.len(),
            file_bytes: std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0),
        });
    }
    let opts = RepackOptions {
        method,
        overwrite: true,
        ..RepackOptions::default()
    };
    let r = rgrf_core::merge::patch(&o, &n, dst, &opts, ctl).map_err(|e| e.to_string())?;
    Ok(PatchSummary {
        added: r.added,
        changed: r.changed,
        removed: r.removed,
        file_bytes: r.repack.output_file_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::Session;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rgrf-saving-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_new_archive_gets_files_and_is_saved_as_grf_thor_and_rgz() {
        let d = dir("new");
        let empty = d.join("new.grf");
        create_empty(&empty).unwrap();
        let mut s = Session::open(&empty).unwrap();
        assert_eq!(s.info().entries, 0);
        let f = d.join("a.txt");
        std::fs::write(&f, b"hello").unwrap();
        s.stage_add(r"data\a.txt", &f).unwrap();
        let plan = s.edit_plan().unwrap();
        for ext in ["grf", "gpf", "thor", "rgz"] {
            let req = SaveRequest {
                dst: d.join(format!("out.{ext}")),
                ..SaveRequest::default()
            };
            let r = save_as(&plan, &req, false, &Control::default()).unwrap();
            assert_eq!((r.added, r.files), (1, 1), "{ext}");
            assert!(r.verified, "{ext}");
            let a = Archive::open(&req.dst).unwrap();
            let i = a
                .find(&GrfPath::from_display(r"data\a.txt", rgrf_core::NameMode::Cp949).unwrap())
                .unwrap();
            assert_eq!(a.read(i).unwrap().data, b"hello", "{ext}");
        }
        assert_eq!(container_of(Path::new("x.THOR")), Container::Thor);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn saving_recompressed_and_encrypted_and_back() {
        let d = dir("enc");
        let empty = d.join("e.grf");
        create_empty(&empty).unwrap();
        let mut s = Session::open(&empty).unwrap();
        let f = d.join("b.bin");
        std::fs::write(&f, vec![7u8; 5000]).unwrap();
        s.stage_add(r"data\b.bin", &f).unwrap();
        let plan = s.edit_plan().unwrap();
        let enc = SaveRequest {
            dst: d.join("enc.grf"),
            recompress: true,
            method: Some("lzma".into()),
            encryption: Some("password".into()),
            password: Some("secret".into()),
            ..SaveRequest::default()
        };
        assert!(
            save_as(&plan, &enc, false, &Control::default())
                .unwrap()
                .verified
        );
        let key = rgrf_core::grfcrypt::key_from_password("secret").unwrap();
        let a = Archive::open_with_key(&enc.dst, &key).unwrap();
        assert_eq!(a.read(0).unwrap().data, vec![7u8; 5000]);
        // an empty password and an encrypted Thor are refused
        let bad = SaveRequest {
            password: Some(String::new()),
            ..enc.clone()
        };
        assert!(save_as(&plan, &bad, true, &Control::default()).is_err());
        let thor = SaveRequest {
            dst: d.join("x.thor"),
            ..enc.clone()
        };
        assert!(save_as(&plan, &thor, true, &Control::default()).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn differences_become_a_thor_patch_with_deletes() {
        let d = dir("patch");
        let (old, new) = (d.join("old.grf"), d.join("new.grf"));
        create_empty(&old).unwrap();
        let mut s = Session::open(&old).unwrap();
        let (fa, fb) = (d.join("a"), d.join("b"));
        std::fs::write(&fa, b"one").unwrap();
        std::fs::write(&fb, b"two").unwrap();
        s.stage_add(r"data\a.txt", &fa).unwrap();
        s.stage_add(r"data\gone.txt", &fb).unwrap();
        let req = SaveRequest {
            dst: d.join("old2.grf"),
            ..SaveRequest::default()
        };
        save_as(&s.edit_plan().unwrap(), &req, false, &Control::default()).unwrap();
        std::fs::rename(&req.dst, &old).unwrap();
        // new: a.txt changed, gone.txt removed, c.txt added
        let mut s = Session::open(&old).unwrap();
        std::fs::write(&fa, b"ONE").unwrap();
        s.stage_add(r"data\a.txt", &fa).unwrap();
        s.stage_add(r"data\c.txt", &fb).unwrap();
        let gone = (0..s.info().entries)
            .find(|&i| s.details(i).is_ok_and(|d| d.name.ends_with("gone.txt")))
            .unwrap();
        s.stage_remove(&[gone]).unwrap();
        let req = SaveRequest {
            dst: new.clone(),
            ..SaveRequest::default()
        };
        save_as(&s.edit_plan().unwrap(), &req, false, &Control::default()).unwrap();
        let c = compare(
            &new,
            None,
            &old,
            rgrf_core::NameMode::Cp949,
            &Control::default(),
        )
        .unwrap();
        assert_eq!(
            (c.only_here.len(), c.only_there.len(), c.changed.len()),
            (1, 1, 1)
        );
        let patch = d.join("p.thor");
        let r = make_patch(&new, None, &old, &patch, None, &Control::default()).unwrap();
        assert_eq!((r.added, r.changed, r.removed), (1, 1, 1));
        // a.txt and c.txt, the hash list, and a delete directive for gone.txt
        let t = Archive::open(&patch).unwrap();
        assert_eq!(t.entries().iter().filter(|e| !e.is_marker()).count(), 3);
        assert_eq!(t.entries().iter().filter(|e| e.is_removal()).count(), 1);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn saving_in_place_replaces_the_archive_and_reopens_it() {
        let d = dir("inplace");
        let grf = d.join("a.grf");
        create_empty(&grf).unwrap();
        let f = d.join("x.txt");
        std::fs::write(&f, b"x").unwrap();
        let mut s = Session::open(&grf).unwrap();
        s.stage_add(r"data\x.txt", &f).unwrap();
        let slot = std::sync::Mutex::new(Some(s));
        let r = save_in_place(&slot, SaveRequest::default(), &Control::default()).unwrap();
        assert_eq!((r.summary.added, r.info.files), (1, 1));
        let g = slot.lock().unwrap();
        let s = g.as_ref().unwrap();
        assert!(s.pending().is_empty());
        assert_eq!(s.info().files, 1);
        // no temporary file is left next to it
        let left: Vec<_> = std::fs::read_dir(&d)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("rgrf-saving"))
            .collect();
        assert!(left.is_empty());
        drop(g);
        // a Thor is saved as a Thor
        let thor = d.join("p.thor");
        let req = SaveRequest {
            dst: thor.clone(),
            ..SaveRequest::default()
        };
        save_as(
            &slot.lock().unwrap().as_ref().unwrap().plan(),
            &req,
            false,
            &Control::default(),
        )
        .unwrap();
        let mut t = Session::open(&thor).unwrap();
        std::fs::write(&f, b"y").unwrap();
        t.stage_add(r"data\y.txt", &f).unwrap();
        let slot = std::sync::Mutex::new(Some(t));
        let r = save_in_place(&slot, SaveRequest::default(), &Control::default()).unwrap();
        assert_eq!(r.info.files, 2);
        assert!(
            r.info.format.to_lowercase().contains("thor"),
            "{}",
            r.info.format
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_text_of_files_is_searched() {
        use crate::backend::ContentQuery;
        let d = dir("search");
        let grf = d.join("s.grf");
        create_empty(&grf).unwrap();
        let mut s = Session::open(&grf).unwrap();
        let files = [
            (r"data\a.lua", "local Prontera = 1\nprint(prontera)\n"),
            (r"data\b.txt", "nothing here\nPRONTERA_FIELD\n"),
            (r"data\c.bin", "pron\0\0\0\0tera prontera"),
        ];
        for (name, text) in files {
            let f = d.join(name.rsplit('\\').next().unwrap());
            std::fs::write(&f, text).unwrap();
            s.stage_add(name, &f).unwrap();
        }
        let q = |query: &str, case, word, files: &str| ContentQuery {
            query: query.into(),
            case,
            whole_word: word,
            files: files.into(),
            ..ContentQuery::default()
        };
        let ctl = Control::default();
        let slot = std::sync::Mutex::new(Some(s));
        save_in_place(&slot, SaveRequest::default(), &ctl).unwrap();
        let s = slot.into_inner().unwrap().unwrap();
        // the binary file is skipped
        let r = s
            .search_contents(&q("prontera", false, false, ""), &ctl)
            .unwrap();
        assert_eq!(
            (r.files_searched, r.files_with_hits, r.hits.len()),
            (3, 2, 3)
        );
        assert_eq!((r.hits[0].line, r.hits[1].line), (1, 2));
        let r = s
            .search_contents(&q("prontera", true, true, "*.lua"), &ctl)
            .unwrap();
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].text, "print(prontera)");
        let regex = ContentQuery {
            regex: true,
            ..q(r"^PRONTERA_\w+$", true, false, "*.txt")
        };
        assert_eq!(s.search_contents(&regex, &ctl).unwrap().hits.len(), 1);
        let bad = ContentQuery {
            regex: true,
            ..q("(", false, false, "")
        };
        assert!(s.search_contents(&bad, &ctl).is_err());
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn hashes_of_known_bytes() {
        let h = hashes(b"abc");
        assert_eq!(h.crc32, "352441c2");
        assert_eq!(h.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert!(h.sha256.starts_with("ba7816bf"));
    }
}
