use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

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
        let path = config_path();
        if path.exists() {
            let contents = fs::read_to_string(&path)?;
            Ok(toml::from_str(&contents)?)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let contents = toml::to_string_pretty(self)?;
        fs::write(&path, contents)?;
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

pub fn pid_path() -> PathBuf {
    runtime_dir().join("klicky.pid")
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
}
