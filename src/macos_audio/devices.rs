use anyhow::{bail, Context, Result};
use cpal_native::traits::{DeviceTrait, HostTrait};
use cpal_native::Device;

use crate::audio::{Backend, OutputDevice, OutputList, OutputPreference};

pub(super) fn validate(preference: &OutputPreference) -> Result<()> {
    preference.validate()?;
    if matches!(preference, OutputPreference::Device { backend, .. } if *backend != Backend::CoreAudio)
    {
        bail!("macOS requires a core-audio output ID");
    }
    Ok(())
}

pub(super) fn describe(device: &Device) -> Result<OutputDevice> {
    let id = device.id()?.id().to_owned();
    let name = device
        .description()
        .ok()
        .map(|description| description.name().to_owned())
        .filter(|name| !name.is_empty() && name.len() <= 2048);
    let output = OutputDevice {
        backend: Backend::CoreAudio,
        id,
        name,
    };
    OutputPreference::Device {
        backend: output.backend,
        id: output.id.clone(),
    }
    .validate()?;
    Ok(output)
}

pub(super) fn default_id() -> Result<String> {
    Ok(cpal_native::default_host()
        .default_output_device()
        .context("no default macOS output")?
        .id()?
        .id()
        .to_owned())
}

pub(crate) fn outputs() -> Result<OutputList> {
    let devices = cpal_native::default_host()
        .output_devices()?
        .take(65)
        .map(|device| describe(&device))
        .collect::<Result<Vec<_>>>()?;
    let default_id = default_id()
        .ok()
        .filter(|id| devices.iter().any(|device| device.id == *id));
    let list = OutputList {
        schema_version: 1,
        devices,
        default_id,
        suggested_speakers_id: None,
    };
    list.validate()?;
    Ok(list)
}

pub(super) fn resolve(preference: &OutputPreference) -> Result<(Device, OutputDevice)> {
    validate(preference)?;
    let id = match preference {
        OutputPreference::SystemDefault => default_id()?,
        OutputPreference::Device { id, .. } => id.clone(),
    };
    // Enumerated devices use CPAL's pinned HAL output path, including when the
    // selected endpoint happens to be the current system default.
    let device = cpal_native::default_host()
        .output_devices()?
        .find(|device| device.id().is_ok_and(|value| value.id() == id))
        .context("selected macOS output is unavailable")?;
    let output = describe(&device)?;
    device.default_output_config()?;
    Ok((device, output))
}
