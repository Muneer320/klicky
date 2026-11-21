//! Shared audio preferences and helper-facing state. Device IDs are backend-local,
//! opaque values, never display names or transient enumeration indexes.
use anyhow::{bail, Result};
use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Backend {
    Wasapi,
    CoreAudio,
    PipeWire,
    PulseAudio,
    Alsa,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputPreference {
    #[default]
    SystemDefault,
    Device {
        backend: Backend,
        id: String,
    },
}

impl OutputPreference {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::SystemDefault)
    }

    pub fn validate(&self) -> Result<()> {
        if let Self::Device { id, .. } = self {
            if id.is_empty() || id.len() > 1024 || id.chars().any(char::is_control) {
                bail!("audio device ID must contain 1-1024 bytes without control characters");
            }
        }
        Ok(())
    }

    /// Adapters opt in only once they can honor explicit selection. Never play
    /// through a different device merely because the requested one is missing.
    pub fn require_system_default(&self) -> Result<()> {
        self.validate()?;
        if !self.is_default() {
            bail!("this audio adapter does not support explicit output selection yet; use `klicky audio follow-default` to restore system-default playback");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OutputDevice {
    pub backend: Backend,
    pub id: String,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable,
    Recovering,
    Inactive,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AudioStatus {
    pub schema_version: u32,
    pub requested: OutputPreference,
    /// None when inactive, unavailable, or the backend cannot identify its route.
    pub active: Option<OutputDevice>,
    pub availability: Availability,
    pub manual_paused: bool,
    pub can_select: bool,
    pub can_enumerate: bool,
    pub can_retry: bool,
}

impl AudioStatus {
    pub fn validate(&self) -> Result<()> {
        self.requested.validate()?;
        if self.schema_version != 1 {
            bail!("unsupported audio status schema");
        }
        if let Some(device) = &self.active {
            OutputPreference::Device {
                backend: device.backend,
                id: device.id.clone(),
            }
            .validate()?;
            if let OutputPreference::Device { backend, id } = &self.requested {
                if *backend != device.backend || *id != device.id {
                    bail!("active output does not match the requested device");
                }
            }
            if self.availability != Availability::Available
                || device
                    .name
                    .as_ref()
                    .is_some_and(|name| name.is_empty() || name.len() > 2048)
            {
                bail!("invalid active audio output");
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Subcommand, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum AudioCommand {
    /// Print confirmed audio state as JSON; requires a running engine
    Status,
    /// List selectable outputs (requires a platform selection adapter)
    Outputs,
    /// Select an opaque backend device ID (requires a platform selection adapter)
    Select {
        #[arg(long, value_enum)]
        backend: Backend,
        #[arg(long, allow_hyphen_values = true)]
        id: String,
    },
    /// Restore system-default policy without changing pause, volume or pack
    FollowDefault,
    /// Request recovery without changing manual pause
    Retry,
}

impl AudioCommand {
    pub fn validate(&self) -> Result<()> {
        if let Self::Select { backend, id } = self {
            OutputPreference::Device {
                backend: *backend,
                id: id.clone(),
            }
            .validate()?;
        }
        Ok(())
    }
}

pub fn unsupported_selection() -> anyhow::Error {
    anyhow::anyhow!("output enumeration/selection is not supported by this platform adapter yet; existing playback and preferences were preserved")
}
