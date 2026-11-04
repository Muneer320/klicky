use std::sync::atomic::Ordering;
use std::sync::Arc;

use ::windows::core::{implement, PCWSTR};
use ::windows::Win32::Foundation::PROPERTYKEY;
use ::windows::Win32::Media::Audio::*;
use ::windows::Win32::System::Com::*;
use anyhow::{Context, Result};

use super::Shared;

#[implement(IMMNotificationClient)]
struct Callback {
    shared: Arc<Shared>,
}

impl Callback {
    fn dirty(&self) {
        self.shared.route_dirty.store(true, Ordering::Release);
        self.shared.wake();
    }
}
#[allow(non_snake_case)]
impl IMMNotificationClient_Impl for Callback_Impl {
    fn OnDeviceStateChanged(&self, _: &PCWSTR, _: DEVICE_STATE) -> ::windows::core::Result<()> {
        self.dirty();
        Ok(())
    }
    fn OnDeviceAdded(&self, _: &PCWSTR) -> ::windows::core::Result<()> {
        self.dirty();
        Ok(())
    }
    fn OnDeviceRemoved(&self, _: &PCWSTR) -> ::windows::core::Result<()> {
        self.dirty();
        Ok(())
    }
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _: &PCWSTR,
    ) -> ::windows::core::Result<()> {
        if (flow == eRender || flow == eAll) && role == eMultimedia {
            self.dirty();
        }
        Ok(())
    }
    fn OnPropertyValueChanged(
        &self,
        id: &PCWSTR,
        key: &PROPERTYKEY,
    ) -> ::windows::core::Result<()> {
        if *key == PKEY_AudioEngine_DeviceFormat {
            if let Ok(current) = self.shared.device_id.try_lock() {
                // Native callback ID is NUL-terminated. Compare only the stored
                // ID length and terminator, without allocation or text logging.
                if !id.is_null()
                    && !current.is_empty()
                    && current
                        .iter()
                        .enumerate()
                        .all(|(index, value)| unsafe { *id.0.add(index) == *value })
                    && unsafe { *id.0.add(current.len()) == 0 }
                {
                    self.shared.retry.store(true, Ordering::Release);
                    self.shared.wake();
                }
            } else {
                // Notification callbacks may not wait on synchronization. A
                // concurrent ID update gets a conservative retry, not a lost change.
                self.shared.retry.store(true, Ordering::Release);
                self.shared.wake();
            }
        }
        Ok(())
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

pub(super) struct Notifications {
    enumerator: IMMDeviceEnumerator,
    callback: IMMNotificationClient,
    registered: bool,
    _apartment: Apartment,
}
impl Notifications {
    pub fn register(shared: Arc<Shared>) -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
            let apartment = Apartment;
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let callback: IMMNotificationClient = Callback { shared }.into();
            enumerator.RegisterEndpointNotificationCallback(&callback)?;
            Ok(Self {
                enumerator,
                callback,
                registered: true,
                _apartment: apartment,
            })
        }
    }
    pub fn default_id(&self) -> Result<Vec<u16>> {
        unsafe {
            let device = self
                .enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)?;
            let id = device.GetId()?;
            let value = id.as_wide().to_vec();
            CoTaskMemFree(Some(id.0.cast()));
            Ok(value)
        }
    }
    pub fn close(&mut self) -> Result<()> {
        if self.registered {
            unsafe {
                self.enumerator
                    .UnregisterEndpointNotificationCallback(&self.callback)
            }
            .context("unregister audio endpoint notifications")?;
            self.registered = false;
        }
        Ok(())
    }
}
impl Drop for Notifications {
    fn drop(&mut self) {
        if self.close().is_err() {
            // Registration does not AddRef. Unknown unregister failure cannot
            // justify freeing a pointer the system may still call. Retain one
            // reference until process exit; explicit close reports the failure.
            std::mem::forget(self.callback.clone());
        }
    }
}
