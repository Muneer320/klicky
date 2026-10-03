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

pub fn sounds_dir() -> PathBuf {
    app_config_dir().join("sounds")
}

pub fn runtime_dir() -> PathBuf {
    dirs::runtime_dir()
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
