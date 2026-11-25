use anyhow::{bail, Result};
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Playback {
    pub schema_version: u32,
    pub running: bool,
    pub process_exists: bool,
    pub paused: bool,
    pub sound_pack: String,
    pub volume: f64,
    pub packs: Vec<String>,
    pub autostart: bool,
}

impl Playback {
    pub fn parse(text: &str) -> Result<Self> {
        let mut value: Self = serde_json::from_str(text)?;
        if value.schema_version != 1
            || !value.volume.is_finite()
            || !(0.0..=1.0).contains(&value.volume)
            || value.running && !value.process_exists
            || value.paused && !value.running
        {
            bail!("Unsupported or invalid CLI status; update Klicky and refresh");
        }
        // Pack directory names are display metadata, not playback health.
        // Hide unusable choices without disabling Pause/Resume/Stop, including
        // when the active name itself cannot be offered safely in the menu.
        value.packs.retain(|pack| valid_text(pack, 256));
        let mut seen = HashSet::new();
        for pack in &value.packs {
            if !seen.insert(pack) {
                bail!("Invalid sound-pack list");
            }
        }
        value.packs.truncate(256);
        Ok(value)
    }

    pub fn title(&self) -> &'static str {
        if self.running {
            if self.paused {
                "Paused"
            } else {
                "Running"
            }
        } else if self.process_exists {
            "Starting or unresponsive"
        } else {
            "Stopped"
        }
    }

    pub fn can_configure(&self) -> bool {
        self.running || !self.process_exists
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Preference {
    SystemDefault,
    Device { backend: String, id: String },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Device {
    pub backend: String,
    pub id: String,
    pub name: Option<String>,
}

impl Device {
    fn valid(&self) -> bool {
        matches!(
            self.backend.as_str(),
            "wasapi" | "core_audio" | "pipe_wire" | "pulse_audio" | "alsa"
        ) && valid_text(&self.id, 1024)
            && self.name.as_ref().is_none_or(|s| valid_text(s, 2048))
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Audio {
    pub schema_version: u32,
    pub requested: Option<Preference>,
    pub availability: String,
    pub can_select: bool,
    pub can_enumerate: bool,
    pub can_retry: bool,
}

impl Audio {
    pub fn parse(text: &str) -> Result<Self> {
        let value: Self = serde_json::from_str(text)?;
        if value.schema_version != 1
            || !matches!(
                value.availability.as_str(),
                "initializing" | "available" | "unavailable" | "recovering" | "inactive"
            )
        {
            bail!("Unsupported audio status");
        }
        if let Some(Preference::Device { backend, id }) = &value.requested {
            if !(Device {
                backend: backend.clone(),
                id: id.clone(),
                name: None,
            })
            .valid()
            {
                bail!("Invalid requested output");
            }
        }
        Ok(value)
    }

    pub fn supported(&self) -> bool {
        self.can_select || self.can_enumerate || self.can_retry
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Outputs {
    pub schema_version: u32,
    pub devices: Vec<Device>,
}

impl Outputs {
    pub fn parse(text: &str) -> Result<Self> {
        let value: Self = serde_json::from_str(text)?;
        if value.schema_version != 1 || value.devices.len() > 64 {
            bail!("Unsupported or oversized audio-output list");
        }
        let mut seen = HashSet::new();
        for device in &value.devices {
            if !device.valid() || !seen.insert((&device.backend, &device.id)) {
                bail!("Invalid or duplicate audio-output identity");
            }
        }
        Ok(value)
    }
}

fn valid_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

pub fn label(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect::<String>()
        .replace('_', "__")
}
