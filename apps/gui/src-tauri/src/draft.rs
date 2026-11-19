//! Drafts: the pending changes of an archive on disk, so that a crash or closing the window does
//! not lose them. One folder per archive below the app's data folder: `draft.json` (what to do,
//! in order) and one file per edited content (named by its MD5 and length, written once).
//!
//! The open archive is identified by its path; its size and modification time are stored so the
//! app can tell whether it changed since the draft was written.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MANIFEST: &str = "draft.json";
const VERSION: u32 = 1;

/// One pending change as the draft stores it (names are the archive's raw bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftChange {
    Put {
        name: Vec<u8>,
        file: PathBuf,
    },
    Bytes {
        name: Vec<u8>,
        data: Arc<Vec<u8>>,
        what: Vec<String>,
    },
    Remove {
        name: Vec<u8>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Stored {
    Put {
        name: String,
        file: PathBuf,
    },
    Bytes {
        name: String,
        blob: String,
        what: Vec<String>,
    },
    Remove {
        name: String,
    },
}

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    archive: PathBuf,
    archive_len: u64,
    archive_mtime: u64,
    saved_at: u64,
    changes: Vec<Stored>,
}

/// What the app tells about a draft found when an archive is opened.
#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
pub struct DraftInfo {
    pub changes: usize,
    /// Seconds since 1970 when the draft was last written.
    pub saved_at: u64,
    /// The archive's size or modification time differs from when the draft was written.
    pub archive_changed: bool,
    /// Replacement files from disk that no longer exist.
    pub missing_files: usize,
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn archive_stamp(archive: &Path) -> (u64, u64) {
    fs::metadata(archive).map_or((0, 0), |m| (m.len(), m.modified().map_or(0, secs)))
}

fn hex(b: &[u8]) -> String {
    rgrf_core::name::hex(b)
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// The folder of `archive`'s draft below `root` (from a hash of its full path).
pub fn folder(root: &Path, archive: &Path) -> PathBuf {
    let full = archive
        .canonicalize()
        .unwrap_or_else(|_| archive.to_path_buf());
    let d = rgrf_core::hash::Digest::of(full.to_string_lossy().as_bytes());
    root.join(&d.md5_hex()[..16])
}

/// Writes the draft of `archive` (or removes it when there are no changes). Files are written to
/// a temporary name first, so a crash during the write leaves the previous draft intact.
pub fn save(root: &Path, archive: &Path, changes: &[DraftChange]) -> std::io::Result<()> {
    let dir = folder(root, archive);
    if changes.is_empty() {
        return match fs::remove_dir_all(&dir) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    fs::create_dir_all(&dir)?;
    let mut keep = vec![MANIFEST.to_string()];
    let mut stored = Vec::with_capacity(changes.len());
    for c in changes {
        stored.push(match c {
            DraftChange::Put { name, file } => Stored::Put {
                name: hex(name),
                file: file.clone(),
            },
            DraftChange::Remove { name } => Stored::Remove { name: hex(name) },
            DraftChange::Bytes { name, data, what } => {
                let blob = format!(
                    "{}-{}.bin",
                    rgrf_core::hash::Digest::of(data).md5_hex(),
                    data.len()
                );
                let path = dir.join(&blob);
                if !path.exists() {
                    let tmp = dir.join(format!("{blob}.tmp"));
                    fs::write(&tmp, data.as_slice())?;
                    fs::rename(&tmp, &path)?;
                }
                keep.push(blob.clone());
                Stored::Bytes {
                    name: hex(name),
                    blob,
                    what: what.clone(),
                }
            }
        });
    }
    let (archive_len, archive_mtime) = archive_stamp(archive);
    let m = Manifest {
        version: VERSION,
        archive: archive.to_path_buf(),
        archive_len,
        archive_mtime,
        saved_at: secs(SystemTime::now()),
        changes: stored,
    };
    let json = serde_json::to_vec_pretty(&m).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!("{MANIFEST}.tmp"));
    fs::write(&tmp, json)?;
    fs::rename(&tmp, dir.join(MANIFEST))?;
    // contents no change refers to any more
    for f in fs::read_dir(&dir)?.flatten() {
        let n = f.file_name().to_string_lossy().into_owned();
        if !keep.contains(&n) {
            let _ = fs::remove_file(f.path());
        }
    }
    Ok(())
}

fn read_manifest(root: &Path, archive: &Path) -> Option<Manifest> {
    let bytes = fs::read(folder(root, archive).join(MANIFEST)).ok()?;
    let m: Manifest = serde_json::from_slice(&bytes).ok()?;
    (m.version == VERSION && !m.changes.is_empty()).then_some(m)
}

/// Whether `archive` has a draft, and what it holds.
pub fn info(root: &Path, archive: &Path) -> Option<DraftInfo> {
    let m = read_manifest(root, archive)?;
    Some(DraftInfo {
        changes: m.changes.len(),
        saved_at: m.saved_at,
        archive_changed: archive_stamp(archive) != (m.archive_len, m.archive_mtime),
        missing_files: m
            .changes
            .iter()
            .filter(|c| matches!(c, Stored::Put { file, .. } if !file.exists()))
            .count(),
    })
}

/// The changes of `archive`'s draft, in order. Changes whose file or stored content is gone are
/// left out; their number is returned too.
pub fn load(root: &Path, archive: &Path) -> Option<(Vec<DraftChange>, usize)> {
    let m = read_manifest(root, archive)?;
    let dir = folder(root, archive);
    let mut out = Vec::with_capacity(m.changes.len());
    let mut skipped = 0;
    for c in m.changes {
        let change = match c {
            Stored::Put { name, file } if file.exists() => {
                unhex(&name).map(|name| DraftChange::Put { name, file })
            }
            Stored::Put { .. } => None,
            Stored::Remove { name } => unhex(&name).map(|name| DraftChange::Remove { name }),
            Stored::Bytes { name, blob, what } => {
                let data = (!blob.contains(['/', '\\']))
                    .then(|| fs::read(dir.join(&blob)).ok())
                    .flatten();
                unhex(&name)
                    .zip(data)
                    .map(|(name, data)| DraftChange::Bytes {
                        name,
                        data: Arc::new(data),
                        what,
                    })
            }
        };
        match change {
            Some(c) => out.push(c),
            None => skipped += 1,
        }
    }
    Some((out, skipped))
}

/// Deletes `archive`'s draft.
pub fn discard(root: &Path, archive: &Path) {
    let _ = fs::remove_dir_all(folder(root, archive));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("rgrf-draft-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn a_draft_round_trips_and_shares_unchanged_contents() {
        let base = tmp("round");
        let (root, archive, file) = (
            base.join("drafts"),
            base.join("a.grf"),
            base.join("new.txt"),
        );
        fs::write(&archive, b"GRF").unwrap();
        fs::write(&file, b"replacement").unwrap();
        let bytes = Arc::new(b"edited sprite".to_vec());
        let changes = vec![
            DraftChange::Put {
                name: b"data\\x.txt".to_vec(),
                file: file.clone(),
            },
            DraftChange::Bytes {
                name: b"data\\\xc7\xd1.spr".to_vec(),
                data: Arc::clone(&bytes),
                what: vec!["pixels".into()],
            },
            DraftChange::Remove {
                name: b"data\\old.txt".to_vec(),
            },
        ];
        assert!(info(&root, &archive).is_none());
        save(&root, &archive, &changes).unwrap();
        let i = info(&root, &archive).unwrap();
        assert_eq!(
            (i.changes, i.archive_changed, i.missing_files),
            (3, false, 0)
        );
        assert_eq!(load(&root, &archive).unwrap(), (changes.clone(), 0));

        // writing again keeps one content file; a dropped change removes its content
        save(&root, &archive, &changes).unwrap();
        let blobs = |d: &Path| {
            fs::read_dir(d)
                .unwrap()
                .filter(|f| {
                    f.as_ref()
                        .unwrap()
                        .path()
                        .extension()
                        .is_some_and(|e| e == "bin")
                })
                .count()
        };
        let dir = folder(&root, &archive);
        assert_eq!(blobs(&dir), 1);
        save(&root, &archive, &changes[..1]).unwrap();
        assert_eq!(blobs(&dir), 0);

        // a vanished replacement file is reported and left out; a changed archive is noticed
        save(&root, &archive, &changes).unwrap();
        fs::remove_file(&file).unwrap();
        fs::write(&archive, b"GRF, now longer").unwrap();
        let i = info(&root, &archive).unwrap();
        assert_eq!((i.archive_changed, i.missing_files), (true, 1));
        let (got, skipped) = load(&root, &archive).unwrap();
        assert_eq!((got.len(), skipped), (2, 1));

        // no changes: the draft goes away
        save(&root, &archive, &[]).unwrap();
        assert!(!dir.exists() && info(&root, &archive).is_none());
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn damaged_or_foreign_drafts_are_ignored() {
        let base = tmp("bad");
        let (root, archive) = (base.join("drafts"), base.join("a.grf"));
        let dir = folder(&root, &archive);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(MANIFEST), b"{ not json").unwrap();
        assert!(info(&root, &archive).is_none() && load(&root, &archive).is_none());
        // a content name that tries to leave the draft folder is not read
        fs::write(
            dir.join(MANIFEST),
            br#"{"version":1,"archive":"a.grf","archive_len":0,"archive_mtime":0,"saved_at":0,
               "changes":[{"kind":"bytes","name":"61","blob":"../../x","what":[]}]}"#,
        )
        .unwrap();
        assert_eq!(load(&root, &archive).unwrap(), (Vec::new(), 1));
        fs::remove_dir_all(base).unwrap();
    }
}
