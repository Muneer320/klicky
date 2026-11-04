use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use unix::{
    available_sound_packs, ensure_runtime_dir, resolve_sound_pack, runtime_dir, socket_path,
    sound_roots,
};

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
    pub fn validate(&self) -> Result<()> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            bail!("configured volume must be between 0.0 and 1.0");
        }
        Ok(())
    }
}
