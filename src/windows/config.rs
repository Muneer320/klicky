use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ::windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
use anyhow::{bail, Context, Result};

use super::{files, paths::validate_pack_name, Paths};
use crate::Config;

const WRITE_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_CONFIG_BYTES: u64 = 64 * 1024;
static NONCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
pub struct ConfigStore {
    paths: Paths,
}

impl ConfigStore {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    /// Blocking filesystem operation; keep off input, audio and GUI callbacks.
    pub fn load(&self) -> Result<Config> {
        self.update_inner(None::<fn(&mut Config) -> Result<()>>)
    }

    /// Reread and apply a patch while holding the per-user writer lock.
    /// The returned configuration is persisted, not confirmation of active playback.
    pub fn update(&self, patch: impl FnOnce(&mut Config) -> Result<()>) -> Result<Config> {
        self.update_inner(Some(patch))
    }

    fn update_inner(
        &self,
        patch: Option<impl FnOnce(&mut Config) -> Result<()>>,
    ) -> Result<Config> {
        let deadline = Instant::now() + WRITE_TIMEOUT;
        self.paths.ensure_data_dir()?;
        let _writer = files::lock(&self.paths.writer_lock(), deadline)?;
        self.paths.ensure_data_dir()?;
        let path = self.paths.config_path();
        files::check_file_if_present(&path)?;
        let mut contents = String::new();
        let existing = match OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(&path)
        {
            Ok(file) => {
                files::check_metadata(&file.metadata()?, &path, false)?;
                file.take(MAX_CONFIG_BYTES + 1)
                    .read_to_string(&mut contents)?;
                if contents.len() as u64 > MAX_CONFIG_BYTES {
                    bail!("Windows configuration exceeds 64 KiB");
                }
                true
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => false,
            Err(error) => return Err(error).context("read Windows configuration"),
        };
        let mut config: Config = if existing {
            toml::from_str(&contents)?
        } else {
            Config::default()
        };
        config.validate()?;
        validate_pack_name(&config.sound_pack)?;
        let changed = patch.is_some();
        if let Some(patch) = patch {
            patch(&mut config)?;
            config.validate()?;
            validate_pack_name(&config.sound_pack)?;
        }
        if changed || !existing {
            replace(&path, toml::to_string_pretty(&config)?.as_bytes(), deadline)?;
        }
        Ok(config)
    }
}

struct Temporary {
    path: PathBuf,
    file: Option<File>,
}

impl Drop for Temporary {
    fn drop(&mut self) {
        // Close before removal on Windows; only ever remove our own create-new file.
        drop(self.file.take());
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!(
                    "Could not remove temporary configuration {}: {error}",
                    self.path.display()
                );
            }
        }
    }
}

fn replace(path: &Path, contents: &[u8], deadline: Instant) -> Result<()> {
    let parent = path.parent().context("configuration has no parent")?;
    let mut temporary = None;
    for _ in 0..64 {
        let candidate = parent.join(format!(
            ".config.toml.{}.{}.tmp",
            std::process::id(),
            NONCE.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .share_mode(0)
            .open(&candidate)
        {
            Ok(file) => {
                temporary = Some(Temporary {
                    path: candidate,
                    file: Some(file),
                });
                break;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(error) => return Err(error).context("create temporary configuration"),
        }
    }
    let Some(mut temporary) = temporary else {
        bail!("too many temporary configuration name collisions");
    };
    let mut file = temporary
        .file
        .take()
        .context("temporary configuration is closed")?;
    file.write_all(contents)?;
    file.sync_all()?;
    drop(file);
    loop {
        files::check_file_if_present(path)?;
        match fs::rename(&temporary.path, path) {
            Ok(()) => return Ok(()),
            Err(error) if files::sharing_violation(&error) && Instant::now() < deadline => {
                thread::sleep(files::RETRY_INTERVAL)
            }
            Err(error) => {
                return Err(error).context("replace Windows configuration; previous file retained")
            }
        }
    }
}
