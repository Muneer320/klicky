use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{files, process, protocol::InstanceIdentity, Paths};

/// Exclusive per-user/session ownership, not an input/audio readiness signal.
/// Hold through component teardown. Drop/crash releases the handle; keep the file.
/// Releasing this lease is not confirmation that the process has exited.
pub struct InstanceOwner {
    directory: PathBuf,
    _lease: File,
    identity: InstanceIdentity,
}

impl InstanceOwner {
    /// Try once. Busy is `None`; permission, reparse and other path errors propagate.
    /// This does not identify, signal or terminate another process.
    pub fn try_acquire(paths: &Paths) -> Result<Option<Self>> {
        let directory = paths.ensure_runtime_dir()?;
        let Some(lease) = files::try_lock(&directory.join("klicky.lock"))? else {
            return Ok(None);
        };
        Ok(Some(Self {
            directory,
            _lease: lease,
            identity: process::current_identity()?,
        }))
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Stable for this lease; a fresh lease has a fresh nonce, even in one process.
    pub fn identity(&self) -> &InstanceIdentity {
        &self.identity
    }
}
