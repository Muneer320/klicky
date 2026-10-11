use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

static SAVE_NONCE: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub sound_pack: String,
    pub volume: f32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            sound_pack: "eg-oreo".to_string(),
            volume: 0.8,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&config_path())
    }

    fn load_from(path: &Path) -> Result<Self> {
        if path.exists() {
            let contents = fs::read_to_string(path)?;
            let config: Self = toml::from_str(&contents)?;
            config.validate()?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save_to(path)?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&config_path())
    }

    fn save_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let contents = toml::to_string_pretty(self)?;
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("config.toml");
        let (temporary, mut file) = loop {
            let temporary = parent.join(format!(
                ".{file_name}.{}.{}.tmp",
                std::process::id(),
                SAVE_NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
            {
                Ok(file) => break (temporary, file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        };

        let result = (|| {
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    pub fn validate(&self) -> Result<()> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            bail!("configured volume must be between 0.0 and 1.0");
        }
        Ok(())
    }
}

fn app_config_dir() -> PathBuf {
    dirs::config_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("klicky")
}

pub fn config_path() -> PathBuf {
    app_config_dir().join("config.toml")
}

pub fn user_sounds_dir() -> PathBuf {
    app_config_dir().join("sounds")
}

pub fn sound_roots() -> Vec<PathBuf> {
    let mut roots = vec![user_sounds_dir()];
    if let Some(path) = std::env::var_os("KLICKY_SYSTEM_SOUNDS_DIR") {
        roots.push(PathBuf::from(path));
    } else if cfg!(target_os = "linux") {
        roots.push(PathBuf::from("/usr/share/klicky/sounds"));
    }
    roots
}

pub fn resolve_sound_pack(name: &str, roots: &[PathBuf]) -> Option<PathBuf> {
    roots
        .iter()
        .map(|root| root.join(name))
        .find(|path| path.is_dir())
}

pub fn available_sound_packs(roots: &[PathBuf]) -> Result<Vec<String>> {
    let mut names = BTreeSet::new();
    for root in roots.iter().filter(|root| root.is_dir()) {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if entry.path().is_dir() {
                names.insert(entry.file_name().to_string_lossy().into_owned());
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn system_runtime_dir() -> Option<PathBuf> {
    if let Some(path) = dirs::runtime_dir() {
        return Some(path);
    }

    #[cfg(target_os = "linux")]
    {
        // geteuid has no preconditions and only reads process identity.
        let path = PathBuf::from("/run/user").join(unsafe { libc::geteuid() }.to_string());
        if path.is_dir() {
            return Some(path);
        }
    }

    None
}

pub fn runtime_dir() -> PathBuf {
    system_runtime_dir()
        .map(|path| path.join("klicky"))
        .unwrap_or_else(|| app_config_dir().join("run"))
}

pub fn ensure_runtime_dir() -> Result<PathBuf> {
    let path = runtime_dir();
    fs::create_dir_all(&path)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
    Ok(path)
}

pub fn socket_path() -> PathBuf {
    runtime_dir().join("klicky.sock")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("klicky-{name}-{}-{nonce}", std::process::id()))
    }

    #[test]
    fn user_sound_pack_overrides_system_pack() {
        let root = test_root("precedence");
        let user = root.join("user");
        let system = root.join("system");
        fs::create_dir_all(user.join("shared")).unwrap();
        fs::create_dir_all(system.join("shared")).unwrap();

        let resolved = resolve_sound_pack("shared", &[user.clone(), system]);

        assert_eq!(resolved, Some(user.join("shared")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn available_sound_packs_are_unique_sorted_directories() {
        let root = test_root("listing");
        let user = root.join("user");
        let system = root.join("system");
        fs::create_dir_all(user.join("zebra")).unwrap();
        fs::create_dir_all(user.join("shared")).unwrap();
        fs::create_dir_all(system.join("alpha")).unwrap();
        fs::create_dir_all(system.join("shared")).unwrap();
        fs::write(system.join("not-a-pack"), b"ignored").unwrap();

        let packs = available_sound_packs(&[user, system]).unwrap();

        assert_eq!(packs, vec!["alpha", "shared", "zebra"]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_save_atomically_replaces_file_with_private_permissions() {
        let root = test_root("config-save");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        fs::write(&path, "old incomplete config").unwrap();
        let config = Config {
            sound_pack: "cherrymx-blue-abs".to_string(),
            volume: 0.4,
        };

        config.save_to(&path).unwrap();

        assert_eq!(
            Config::load_from(&path).unwrap().sound_pack,
            config.sound_pack
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_save_skips_stale_temporary_file_names() {
        let root = test_root("config-save-collision");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        let nonce = SAVE_NONCE.load(Ordering::Relaxed);
        let stale = root.join(format!(".config.toml.{}.{}.tmp", std::process::id(), nonce));
        fs::write(&stale, "stale temp from a previous process").unwrap();

        Config::default().save_to(&path).unwrap();

        assert!(Config::load_from(&path).is_ok());
        assert_eq!(
            fs::read_to_string(stale).unwrap(),
            "stale temp from a previous process"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_config_save_preserves_existing_file_and_cleans_temporary_file() {
        let root = test_root("config-save-failure");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        fs::create_dir(&path).unwrap();
        fs::write(path.join("sentinel"), "preserve").unwrap();
        let config = Config::default();

        assert!(config.save_to(&path).is_err());

        assert_eq!(
            fs::read_to_string(path.join("sentinel")).unwrap(),
            "preserve"
        );
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn config_load_rejects_non_finite_and_out_of_range_volume() {
        let root = test_root("config-volume-validation");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");

        for volume in ["-0.1", "1.1", "nan", "inf"] {
            fs::write(
                &path,
                format!("sound_pack = \"eg-oreo\"\nvolume = {volume}\n"),
            )
            .unwrap();
            assert!(Config::load_from(&path).is_err(), "accepted {volume}");
        }

        for volume in [0.0, 0.5, 1.0] {
            let config = Config {
                sound_pack: "eg-oreo".to_string(),
                volume,
            };
            config.save_to(&path).unwrap();
            assert_eq!(Config::load_from(&path).unwrap().volume, volume);
        }
        fs::remove_dir_all(root).unwrap();
    }
}
