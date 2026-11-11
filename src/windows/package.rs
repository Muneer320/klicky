//! Package identity comes from Windows, never from a marker or an environment flag.
use std::time::{Duration, Instant};

use ::windows::ApplicationModel::{StartupTask, StartupTaskState};
use ::windows::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
use ::windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
use ::windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use anyhow::{bail, Context, Result};
use windows_core::{RuntimeType, HSTRING};
use windows_future::{AsyncStatus, IAsyncOperation};

use super::startup::Status;

pub(super) fn current() -> Result<bool> {
    let mut length = 0;
    match unsafe { GetCurrentPackageFullName(&mut length, None) } {
        APPMODEL_ERROR_NO_PACKAGE => Ok(false),
        ERROR_INSUFFICIENT_BUFFER => Ok(true),
        error => Err(std::io::Error::from_raw_os_error(error.0 as i32))
            .context("query Windows package identity"),
    }
}

struct Apartment;
impl Apartment {
    fn new() -> Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)? };
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

fn complete<T: RuntimeType>(operation: IAsyncOperation<T>, deadline: Instant) -> Result<T> {
    while operation.Status()? == AsyncStatus::Started {
        if Instant::now() >= deadline {
            let _ = operation.Cancel();
            bail!("Windows startup task timed out; check Windows Startup settings");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Ok(operation.GetResults()?)
}

pub(super) fn startup(enabled: Option<bool>) -> Result<Status> {
    let _apartment = Apartment::new()?;
    let deadline = Instant::now() + Duration::from_secs(2);
    let task = complete(
        StartupTask::GetAsync(&HSTRING::from("KlickyStartup"))?,
        deadline,
    )?;
    let state = task.State()?;
    if state == StartupTaskState::DisabledByUser {
        if enabled == Some(true) {
            bail!("Windows disabled Klicky startup. Enable it in Settings > Apps > Startup; Klicky will not override your choice");
        }
        return Ok(Status::DisabledByUser);
    }
    if state == StartupTaskState::DisabledByPolicy {
        if enabled == Some(true) {
            bail!("Klicky startup is disabled by Windows policy");
        }
        return Ok(Status::DisabledByPolicy);
    }
    if state == StartupTaskState::EnabledByPolicy {
        if enabled == Some(false) {
            bail!("Klicky startup is enabled by Windows policy and cannot be disabled here");
        }
        return Ok(Status::EnabledByPolicy);
    }
    match enabled {
        Some(true) if state == StartupTaskState::Disabled => {
            let state = complete(task.RequestEnableAsync()?, deadline)?;
            if state != StartupTaskState::Enabled {
                bail!("Windows did not enable Klicky startup; check Windows Startup settings");
            }
        }
        Some(false) => task.Disable()?,
        _ => (),
    }
    match task.State()? {
        StartupTaskState::Enabled => Ok(Status::Registered),
        StartupTaskState::Disabled => Ok(Status::Absent),
        StartupTaskState::DisabledByUser => Ok(Status::DisabledByUser),
        StartupTaskState::DisabledByPolicy => Ok(Status::DisabledByPolicy),
        StartupTaskState::EnabledByPolicy => Ok(Status::EnabledByPolicy),
        _ => bail!("Unrecognized Windows startup task state"),
    }
}
