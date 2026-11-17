//! RustyGRF core library (milestone M1: read and verify GRF 0x200).

pub mod archive;
pub mod build;
pub mod codec;
pub mod container;
pub mod encode;
pub mod error;
pub mod extract;
pub mod hash;
pub mod io;
pub mod name;
mod pipeline;
pub mod progress;
pub mod repack;
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
