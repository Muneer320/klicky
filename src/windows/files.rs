use std::fs::{self, File, Metadata, OpenOptions};
use std::io;
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use ::windows::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT,
};
use anyhow::{bail, Context, Result};

pub(super) const RETRY_INTERVAL: Duration = Duration::from_millis(10);

pub(super) fn check_metadata(metadata: &Metadata, path: &Path, directory: bool) -> Result<()> {
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
        bail!("managed path is a reparse point: {}", path.display());
    }
    if metadata.is_dir() != directory || (!directory && !metadata.is_file()) {
        bail!("managed path has the wrong file type: {}", path.display());
    }
    Ok(())
}

pub(super) fn ensure_directory(path: &Path) -> Result<()> {
    match fs::create_dir(path) {
        Ok(()) => (),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error).with_context(|| format!("create {}", path.display())),
    }
    check_metadata(&fs::symlink_metadata(path)?, path, true)
}

pub(super) fn check_file_if_present(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => check_metadata(&metadata, path, false),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn sharing_violation(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(32 | 33))
}

/// Persistent file, exclusive handle: closing/crashing releases ownership.
/// Never unlink a lock file to bypass another holder.
pub(super) fn lock(path: &Path, deadline: Instant) -> Result<File> {
    loop {
        match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)
        {
            Ok(file) => {
                check_metadata(&file.metadata()?, path, false)?;
                return Ok(file);
            }
            Err(error) if sharing_violation(&error) && Instant::now() < deadline => {
                thread::sleep(RETRY_INTERVAL);
            }
            Err(error) => {
                return Err(error).with_context(|| format!("acquire lock {}", path.display()));
            }
        }
    }
}
