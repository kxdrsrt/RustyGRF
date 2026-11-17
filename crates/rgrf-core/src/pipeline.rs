//! The write pipeline shared by `repack` and `build`: blocks are prepared in parallel in batches
//! (bounded memory), a writer thread appends them in order while the next batch is prepared.

use rayon::prelude::*;
use sha2::{Digest as _, Sha256};
use std::fs::File;
use std::io::BufWriter;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::error::Result;
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

/// Prepares every unit of every batch with `prepare` (in parallel inside a batch) and writes the
/// results in order. `on_batch` sees each finished batch before it is written (for statistics).
pub(crate) fn run(
    mut writer: Sink,
    ranges: Vec<Range<usize>>,
    total: u64,
    ctl: &Control,
    prepare: &(dyn Fn(usize) -> Result<Prepared> + Sync),
    on_batch: &mut dyn FnMut(&[Prepared]),
) -> Result<(Sink, Vec<Written>)> {
    let (tx, rx) = mpsc::sync_channel::<Vec<Prepared>>(1);
    let mut produced: Result<()> = Ok(());
    let written: Result<(Sink, Vec<Written>)> = std::thread::scope(|scope| {
        let handle = scope.spawn(move || {
            let mut done = Vec::new();
            for batch in rx {
                for p in batch {
                    let pl = match p.raw {
                        Some((zsize, aligned)) => writer.put_raw(&p.stream, zsize, aligned)?,
                        None => writer.put_stream(&p.stream)?,
                    };
                    done.push((p.unit, pl, p.flags, p.expected));
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
