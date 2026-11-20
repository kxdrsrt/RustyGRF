//! The write pipeline shared by `repack` and `build`: blocks are prepared in parallel in batches
//! (bounded memory), a writer thread appends them in order while the next batch is prepared.

use rayon::prelude::*;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::BufWriter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::container::{flags, legacy_head_only};
use crate::des;
use crate::error::{Error, Result};
use crate::name::{GrfPath, NameMode};
use crate::progress::Control;
use crate::writer::{Placed, Writer};

pub(crate) type Sink = Writer<BufWriter<File>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expected {
    /// Decodes to data with this hash and status class.
    Data([u8; 32], u8),
    /// Encrypted: the stored bytes (with padding) must be identical.
    Raw([u8; 32]),
    /// Copied unchanged: the stream (without padding) must be identical, so it decodes as before.
    Stream([u8; 32]),
    Nothing,
}

/// How a block ends up in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Encoded,
    KeptOriginal,
    Encrypted,
    Damaged,
    Stored,
    /// A protected entry decoded and stored like any other file.
    Unprotected,
}

pub(crate) struct Prepared {
    pub(crate) unit: usize,
    pub(crate) kind: Kind,
    pub(crate) stream: Vec<u8>,
    /// `Some((zsize, aligned))` for blocks copied unchanged.
    pub(crate) raw: Option<(u32, u32)>,
    pub(crate) flags: u8,
    pub(crate) expected: Expected,
}

/// What the writer thread reports per written block: unit, position, flags, expectation.
pub(crate) type Written = (usize, Placed, u8, Expected);

pub(crate) fn sha(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

pub(crate) fn status_class(s: &crate::codec::DecodeStatus) -> u8 {
    use crate::codec::DecodeStatus as S;
    match s {
        S::Ok => 0,
        S::Truncated { .. } => 1,
        S::SizeMismatch { .. } => 2,
        S::Corrupt(_) => 3,
        S::Encrypted => 4,
    }
}

/// How much unpacked data one batch may hold in memory (all worker threads together).
pub(crate) const BATCH_BYTES: u64 = 256 << 20;
pub(crate) const BATCH_UNITS: usize = 4096;

/// Splits `0..sizes.len()` into batches of at most [`BATCH_BYTES`] / [`BATCH_UNITS`].
pub(crate) fn batches(sizes: impl Iterator<Item = u64>) -> Vec<Range<usize>> {
    let (mut out, mut start, mut bytes, mut n) = (Vec::new(), 0usize, 0u64, 0usize);
    for (i, size) in sizes.enumerate() {
        if i > start && (bytes + size > BATCH_BYTES || i - start >= BATCH_UNITS) {
            out.push(start..i);
            start = i;
            bytes = 0;
        }
        bytes += size;
        n = i + 1;
    }
    if start < n {
        out.push(start..n);
    }
    out
}

/// Scrambles a block for a 0x102/0x103 archive, as GRF Editor does: only the first 20 blocks for
/// `.gnd .gat .act .str` (and short names), else a cycle derived from the stored size. Blocks that
/// are already scrambled (copied DES entries) stay as they are. Returns the sizes to record.
fn legacy_scramble(name: &GrfPath, p: &mut Prepared) -> Result<Option<(u32, u32)>> {
    if p.flags & flags::ANY_CRYPT != 0 {
        if p.flags & (flags::GRF_EDITOR_CRYPT | flags::GRAVITY_CRYPT | flags::DETECTED_CRYPT) != 0 {
            return Err(Error::Unsupported(format!(
                "{} is encrypted with a key; the 0x102/0x103 format cannot hold it",
                name.display(NameMode::Cp1252)
            )));
        }
        return Ok(p.raw);
    }
    let zsize = p.raw.map_or(p.stream.len(), |r| r.0 as usize) as u32;
    let name = name.as_bytes();
    let cycle = if legacy_head_only(name) || name.len() <= 4 {
        des::Cycle::Head
    } else {
        des::Cycle::for_size(zsize)
    };
    p.stream.resize(p.stream.len().div_ceil(8) * 8, 0);
    des::encrypt_data(&mut p.stream, cycle);
    Ok(Some((zsize, p.stream.len() as u32)))
}

/// Encrypts a plain compressed stream with GRF Editor's scheme (aligned, then RC4 with the
/// decompressed size as the counter). Streams that cannot be told apart from plain ones afterwards
/// stay plain, like in GRF Editor.
fn encrypt_plain(key: &crate::grfcrypt::Key, size: u32, p: &mut Prepared) {
    if p.flags != flags::FILE || p.raw.is_some() {
        return;
    }
    let zsize = p.stream.len() as u32;
    let mut padded = p.stream.clone();
    padded.resize(padded.len().div_ceil(8) * 8, 0);
    if crate::grfcrypt::encrypt_stream(key, &mut padded, u64::from(size)) {
        p.raw = Some((zsize, padded.len() as u32));
        p.stream = padded;
    }
}

/// Writes the hidden entry that marks an archive as encrypted: the CRC32 of the key, as a stored
/// zlib block like GRF Editor writes it (so the archives are byte for byte the same).
pub(crate) fn encryption_marker(
    writer: &mut Sink,
    key: &crate::grfcrypt::Key,
) -> Result<crate::writer::OutEntry> {
    let crc = crate::grfcrypt::key_crc(key).to_le_bytes();
    let placed = writer.put_stream(&crate::encode::encode(
        crate::encode::Method::Zlib { level: 0 },
        &crc,
    ))?;
    Ok(crate::writer::OutEntry {
        name: GrfPath::new(crate::grfcrypt::MARKER_NAME.as_bytes()),
        placed,
        size: 4,
        flags: flags::FILE,
    })
}

/// What to encrypt: with `only`, just the entries whose name key (see [`GrfPath::key`]) is in the
/// set; the others are written as plain streams.
#[derive(Clone, Copy)]
pub(crate) struct Encrypt<'a> {
    pub key: &'a crate::grfcrypt::Key,
    pub only: Option<&'a std::collections::HashSet<Vec<u8>>>,
}

/// Prepares every unit of every batch with `prepare` (in parallel inside a batch) and writes the
/// results in order. `on_batch` sees each finished batch before it is written (for statistics).
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    mut writer: Sink,
    ranges: Vec<Range<usize>>,
    total: u64,
    ctl: &Control,
    prepare: &(dyn Fn(usize) -> Result<Prepared> + Sync),
    info_of: &(dyn Fn(usize) -> (GrfPath, u32) + Sync),
    encrypt: Option<Encrypt<'_>>,
    on_batch: &mut dyn FnMut(&[Prepared]),
) -> Result<(Sink, Vec<Written>)> {
    let (tx, rx) = mpsc::sync_channel::<Vec<Prepared>>(1);
    let mut produced: Result<()> = Ok(());
    let written: Result<(Sink, Vec<Written>)> = std::thread::scope(|scope| {
        let handle = scope.spawn(move || {
            let mut done = Vec::new();
            for batch in rx {
                for mut p in batch {
                    if writer.is_legacy() {
                        p.raw = legacy_scramble(&info_of(p.unit).0, &mut p)?;
                    } else if let Some(enc) = &encrypt {
                        let (name, size) = info_of(p.unit);
                        if enc.only.is_none_or(|set| set.contains(&name.key())) {
                            encrypt_plain(enc.key, size, &mut p);
                        }
                    }
                    let pl = match p.raw {
                        Some((zsize, aligned)) => writer.put_raw(&p.stream, zsize, aligned)?,
                        None => writer.put_stream(&p.stream)?,
                    };
                    // GRF Editor's encryption has no flag in the table
                    done.push((p.unit, pl, p.flags & !flags::DETECTED_CRYPT, p.expected));
                }
            }
            Ok((writer, done))
        });
        for range in ranges {
            if let Err(e) = ctl.check() {
                produced = Err(e);
                break;
            }
            // checking per unit lets a cancel stop a batch of slow encodes (Zopfli) at once
            let batch: Result<Vec<Prepared>> = range
                .clone()
                .into_par_iter()
                .map(|u| {
                    ctl.check()?;
                    prepare(u)
                })
                .collect();
            match batch {
                Ok(batch) => {
                    on_batch(&batch);
                    if tx.send(batch).is_err() {
                        break; // the writer stopped; its error is returned below
                    }
                    ctl.report(range.end as u64, total);
                }
                Err(e) => {
                    produced = Err(e);
                    break;
                }
            }
        }
        drop(tx);
        handle.join().expect("writer thread panicked")
    });
    match written {
        Ok(w) => produced.map(|()| w),
        Err(e) => Err(produced.err().unwrap_or(e)),
    }
}

/// Temporary file next to the final one; the result is renamed only after all checks passed.
pub(crate) fn temp_path(dst: &Path) -> PathBuf {
    let name = dst
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dst.with_file_name(format!(".{name}.rgrf-tmp"))
}

pub(crate) fn commit(tmp: &Path, dst: &Path, overwrite: bool) -> Result<()> {
    if overwrite && dst.exists() {
        std::fs::remove_file(dst)?;
    }
    std::fs::rename(tmp, dst)?;
    Ok(())
}
