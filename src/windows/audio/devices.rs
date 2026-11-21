//! Blocking endpoint queries. Call from a worker or the standalone CLI, never
//! from the input, render, or tray callbacks.
use anyhow::{bail, Context, Result};
use cpal_native::traits::{DeviceTrait, HostTrait};
use cpal_native::{Device, DeviceDescription, DeviceType, InterfaceType};

use crate::audio::{Backend, OutputDevice, OutputList, OutputPreference};

pub fn validate_preference(preference: &OutputPreference) -> Result<()> {
    preference.validate()?;
    if matches!(preference, OutputPreference::Device { backend, .. } if *backend != Backend::Wasapi)
    {
        bail!("Windows audio selection requires a WASAPI device ID");
    }
    Ok(())
}

pub fn matches(preference: &OutputPreference, id: &str) -> bool {
    match preference {
        OutputPreference::SystemDefault => true,
        OutputPreference::Device {
            backend: Backend::Wasapi,
            id: requested,
        } => requested == id,
        _ => false,
    }
}

fn description(device: &Device) -> Option<DeviceDescription> {
    // CPAL's WASAPI metadata path can panic when a property store disappears
    // during hotplug. Optional labels must not terminate stream ownership.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| device.description()))
        .ok()
        .and_then(Result::ok)
}

pub fn name(device: &Device) -> Option<String> {
    description(device)
        .map(|description| description.name().to_owned())
        .filter(|name| !name.is_empty() && name.len() <= 2048)
}

fn endpoint_id(device: &Device) -> Result<String> {
    let cpal_native::platform::DeviceInner::Wasapi(native) = device.as_inner();
    let native = native.immdevice().context("audio endpoint disappeared")?;
    unsafe {
        let id = native.GetId()?;
        let text = id.to_string();
        ::windows::Win32::System::Com::CoTaskMemFree(Some(id.0.cast()));
        Ok(text?)
    }
}

pub fn find_output(id: &str) -> Result<Device> {
    for device in cpal_native::default_host().output_devices()? {
        if endpoint_id(&device).ok().as_deref() == Some(id) {
            return Ok(device);
        }
    }
    bail!("selected audio output is not available; previous preference was kept")
}

pub fn validate_selection(preference: &OutputPreference) -> Result<()> {
    validate_preference(preference)?;
    if let OutputPreference::Device { id, .. } = preference {
        let device = find_output(id)?;
        device
            .default_output_config()
            .context("selected output has no usable default PCM format")?;
    }
    Ok(())
}

pub fn enumerate() -> Result<OutputList> {
    let mut devices = Vec::new();
    let mut speakers = Vec::new();
    for device in cpal_native::default_host().output_devices()? {
        let Ok(id) = endpoint_id(&device) else {
            continue;
        };
        let metadata = description(&device);
        if metadata.as_ref().is_some_and(|info| {
            info.device_type() == DeviceType::Speaker
                && info.interface_type() == InterfaceType::BuiltIn
        }) {
            speakers.push(id.clone());
        }
        devices.push(OutputDevice {
            backend: Backend::Wasapi,
            id,
            name: metadata
                .map(|info| info.name().to_owned())
                .filter(|name| !name.is_empty() && name.len() <= 2048),
        });
        if devices.len() > 64 {
            bail!("too many audio outputs to return in one control response");
        }
    }
    devices.sort_by(|a, b| (&a.name, &a.id).cmp(&(&b.name, &b.id)));
    let default_id = super::notifications::default_endpoint_id()
        .ok()
        .and_then(|id| String::from_utf16(&id).ok())
        .filter(|id| devices.iter().any(|device| device.id == *id));
    let list = OutputList {
        schema_version: 1,
        devices,
        default_id,
        suggested_speakers_id: (speakers.len() == 1).then(|| speakers.remove(0)),
    };
    list.validate()?;
    Ok(list)
}
