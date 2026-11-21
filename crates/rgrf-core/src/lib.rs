//! RustyGRF core library: read, verify, build, repack, merge GRF archives (0x102 to 0x300).

pub mod archive;
pub mod build;
pub mod clientcrypt;
pub mod codec;
pub mod container;
pub mod definitions;
pub mod des;
pub mod edit;
pub mod encode;
pub mod error;
pub mod extract;
pub mod gravity;
pub mod grfcrypt;
pub mod hash;
pub mod hashlist;
pub mod io;
pub mod merge;
pub mod name;
mod pipeline;
pub mod progress;
pub mod repack;
pub mod rgz;
pub mod thor;
pub mod thorpack;
pub mod verify;
pub mod writer;

#[cfg(test)]
mod synthetic_tests;

pub use archive::Archive;
pub use codec::{DecodeStatus, Decoded};
pub use container::{Entry, Header};
pub use error::{Error, Result};
pub use name::{GrfPath, NameMode};

/// Crate version, used by the CLI and the MCP server for `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
