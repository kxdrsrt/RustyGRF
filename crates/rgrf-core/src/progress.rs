//! Progress reporting and cancellation shared by CLI, MCP server and the later GUI.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(Error::Cancelled)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

/// Callback invoked from worker threads; keep it cheap.
pub type ProgressFn<'a> = &'a (dyn Fn(Progress) + Sync);

/// Optional progress callback and cancel token handed to long operations.
#[derive(Clone, Copy, Default)]
pub struct Control<'a> {
    pub cancel: Option<&'a CancelToken>,
    pub progress: Option<ProgressFn<'a>>,
}

impl Control<'_> {
    pub fn check(&self) -> Result<()> {
        self.cancel.map_or(Ok(()), CancelToken::check)
    }

    pub fn report(&self, done: u64, total: u64) {
        if let Some(f) = self.progress {
            f(Progress { done, total });
        }
    }
}
