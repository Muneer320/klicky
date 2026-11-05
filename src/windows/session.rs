//! Fail closed until this process's desktop session is verified as unlocked.
use ::windows::core::PWSTR;
use ::windows::Win32::Foundation::HWND;
use ::windows::Win32::System::RemoteDesktop::*;
use ::windows::Win32::UI::WindowsAndMessaging::{
    PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMECRITICAL, PBT_APMRESUMESUSPEND, PBT_APMSUSPEND,
    WTS_CONSOLE_CONNECT, WTS_CONSOLE_DISCONNECT, WTS_REMOTE_CONNECT, WTS_REMOTE_DISCONNECT,
    WTS_SESSION_LOCK, WTS_SESSION_LOGOFF, WTS_SESSION_LOGON, WTS_SESSION_UNLOCK,
};
use anyhow::{bail, Result};

pub(super) struct Session {
    hwnd: HWND,
    id: u32,
    verified: bool,
    suspended: bool,
    ending: bool,
}
impl Session {
    pub fn register(hwnd: HWND, id: u32) -> Result<Self> {
        unsafe { WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION)? };
        let mut session = Self {
            hwnd,
            id,
            verified: false,
            suspended: false,
            ending: false,
        };
        session.refresh();
        Ok(session)
    }
    pub fn active(&self) -> bool {
        self.verified && !self.suspended && !self.ending
    }
    pub fn change(&mut self, event: u32, id: u32) -> bool {
        if id != self.id {
            return false;
        }
        match event {
            WTS_SESSION_LOCK
            | WTS_SESSION_LOGOFF
            | WTS_CONSOLE_DISCONNECT
            | WTS_REMOTE_DISCONNECT => self.verified = false,
            WTS_SESSION_UNLOCK | WTS_SESSION_LOGON | WTS_CONSOLE_CONNECT | WTS_REMOTE_CONNECT => {
                self.refresh()
            }
            _ => return false,
        }
        true
    }
    pub fn power(&mut self, event: u32) -> bool {
        match event {
            PBT_APMSUSPEND => self.suspended = true,
            PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND | PBT_APMRESUMECRITICAL => {
                self.suspended = false;
                self.refresh();
            }
            _ => return false,
        }
        true
    }
    pub fn ending(&mut self, ending: bool) {
        self.ending = ending;
        if !ending {
            self.refresh();
        }
    }
    fn refresh(&mut self) {
        self.verified = unlocked(self.id).unwrap_or(false);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = WTSUnRegisterSessionNotification(self.hwnd);
        }
    }
}

fn unlocked(id: u32) -> Result<bool> {
    let mut buffer = PWSTR::null();
    let mut bytes = 0;
    unsafe { WTSQuerySessionInformationW(None, id, WTSSessionInfoEx, &mut buffer, &mut bytes)? };
    struct Buffer(PWSTR);
    impl Drop for Buffer {
        fn drop(&mut self) {
            unsafe { WTSFreeMemory(self.0.as_ptr().cast()) };
        }
    }
    let buffer = Buffer(buffer);
    if buffer.0.is_null() || (bytes as usize) < size_of::<WTSINFOEXW>() {
        bail!("Windows returned incomplete session information");
    }
    let info = unsafe { buffer.0.as_ptr().cast::<WTSINFOEXW>().read_unaligned() };
    if info.Level != 1 {
        bail!("Windows returned an unknown session information level");
    }
    let info = unsafe { info.Data.WTSInfoExLevel1 };
    Ok(info.SessionId == id
        && info.SessionState == WTSActive
        && info.SessionFlags == WTS_SESSIONSTATE_UNLOCK as i32)
}
