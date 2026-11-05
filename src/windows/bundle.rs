//! A reader lease shared by desktop processes across sessions. Installers must
//! acquire the same marker exclusively before touching bundle contents.
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use ::windows::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
use anyhow::{bail, Context, Result};

use super::{files, Paths};

pub(super) const DESKTOP_EXE: &str = "klicky-desktop.exe";
const MARKER: &str = ".klicky.bundle.lock";
static PROCESS_LEASE: OnceLock<File> = OnceLock::new();

pub(super) fn hold_for_process(paths: &Paths) -> Result<()> {
    if PROCESS_LEASE.get().is_none() {
        let lease = acquire(paths.bundle_dir())?;
        // Statics are deliberately not dropped: retain the native reader even
        // through a final error dialog, until Windows ends this process.
        let _ = PROCESS_LEASE.set(lease);
    }
    Ok(())
}

#[derive(Debug)]
struct Busy(io::Error);
impl fmt::Display for Busy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Klicky bundle is locked by installation/removal: {}",
            self.0
        )
    }
}
impl std::error::Error for Busy {}
pub(super) fn is_busy(error: &anyhow::Error) -> bool {
    error.is::<Busy>()
}

pub(super) fn acquire(directory: &Path) -> Result<File> {
    let marker = directory.join(MARKER);
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(file) => drop(file),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
        Err(error) => return Err(error).context("create bundle ownership marker"),
    }
    let deadline = Instant::now() + Duration::from_millis(250);
    let lease = loop {
        match OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&marker)
        {
            Ok(file) => break file,
            Err(error) if files::sharing_violation(&error) && Instant::now() < deadline => {
                std::thread::sleep(files::RETRY_INTERVAL);
            }
            Err(error) if files::sharing_violation(&error) => return Err(Busy(error).into()),
            Err(error) => {
                return Err(error)
                    .context("bundle is unavailable or locked by installation/removal")
            }
        }
    };
    let metadata = lease.metadata()?;
    files::check_metadata(&metadata, &marker, false)?;
    if metadata.len() != 0 {
        bail!("bundle ownership marker must be empty");
    }
    Ok(lease)
}

pub(super) fn desktop_path(paths: &Paths) -> Result<std::path::PathBuf> {
    let executable = paths.bundle_dir().join(DESKTOP_EXE);
    files::check_metadata(&std::fs::symlink_metadata(&executable)?, &executable, false)?;
    Ok(executable)
}

pub(super) fn owns_image(paths: &Paths, image: &Path) -> Result<bool> {
    let image = image
        .canonicalize()
        .context("resolve verified server image")?;
    // Canonical paths preserve distinct case-sensitive Windows directories.
    Ok(image.parent() == Some(paths.bundle_dir())
        && image
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.eq_ignore_ascii_case(DESKTOP_EXE) || name.eq_ignore_ascii_case("klicky.exe")
            }))
}
