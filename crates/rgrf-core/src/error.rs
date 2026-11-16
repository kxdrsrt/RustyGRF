use std::io;

/// Errors that stop an operation. Damaged entries are not errors: they are reported through
/// [`crate::codec::DecodeStatus`] so that tolerant reading (like the client) stays possible.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("not a GRF archive: {0}")]
    NotGrf(String),
    #[error("unsupported GRF version 0x{0:03x} (supported: 0x200, 0x300)")]
    UnsupportedVersion(u32),
    #[error("corrupt file table: {0}")]
    CorruptTable(String),
    #[error("entry index {0} out of range")]
    NoSuchEntry(usize),
    #[error(
        "entry {index} lies outside the archive (offset {offset}, size {size}, file size {file_size})"
    )]
    EntryOutOfRange {
        index: usize,
        offset: u64,
        size: u64,
        file_size: u64,
    },
    #[error("operation cancelled")]
    Cancelled,
    #[error("unsafe or invalid path in archive: {0}")]
    UnsafePath(String),
    #[error("target already exists: {0}")]
    TargetExists(String),
    #[error("{0}")]
    TooLarge(String),
    #[error("cannot repack: {0}")]
    Unsupported(String),
    #[error("verification of the written archive failed: {0}")]
    VerifyFailed(String),
}

pub type Result<T> = std::result::Result<T, Error>;
