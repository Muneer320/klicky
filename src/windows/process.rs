use std::io;
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::time::Instant;

use ::windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
use ::windows::Win32::Security::{GetTokenInformation, TokenSessionId, TOKEN_QUERY};
use ::windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use ::windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, GetProcessTimes, OpenProcess, OpenProcessToken,
    WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use anyhow::{bail, Context, Result};

use super::protocol::InstanceIdentity;

pub(super) fn current_identity() -> Result<InstanceIdentity> {
    let mut nonce = [0; 16];
    // A per-lease OS-generated nonce, not a predictable PID/timestamp credential.
    unsafe { BCryptGenRandom(None, &mut nonce, BCRYPT_USE_SYSTEM_PREFERRED_RNG).ok()? };
    let identity = InstanceIdentity {
        pid: unsafe { GetCurrentProcessId() },
        creation_time: creation_time(unsafe { GetCurrentProcess() })?,
        nonce,
    };
    identity.validate()?;
    Ok(identity)
}

/// A retained kernel process object, never reopened by PID during an exit wait.
/// This does not grant termination rights or establish pipe/logon authorization.
pub struct ServerProcess {
    handle: OwnedHandle,
    identity: InstanceIdentity,
}

impl ServerProcess {
    /// Bind an authenticated pipe reply to its actual server and current session.
    /// The transport must separately enforce the current-logon SID and local-only
    /// pipe policy. Obtain this handle before sending Stop; keep it through exit.
    /// The nonce comes from that reply; match later replies with Response::validate_for.
    pub fn verify(pipe: BorrowedHandle<'_>, identity: &InstanceIdentity) -> Result<Self> {
        identity.validate()?;
        let pipe = HANDLE(pipe.as_raw_handle());
        let mut pid = 0;
        unsafe { GetNamedPipeServerProcessId(pipe, &mut pid)? };
        if pid != identity.pid {
            bail!("pipe server PID does not match its instance reply");
        }
        // Ownership transfers immediately; every later error closes the handle.
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )?
        };
        let handle = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        if creation_time(HANDLE(handle.as_raw_handle()))? != identity.creation_time {
            bail!("pipe server creation time does not match its instance reply");
        }
        // Query the retained object, not a potentially reused PID after opening it.
        let server_session = session_id(HANDLE(handle.as_raw_handle()))?;
        let current_session = session_id(unsafe { GetCurrentProcess() })?;
        if server_session != current_session {
            bail!("pipe server belongs to another Windows session");
        }
        if super::pipe::process_logon_sid(HANDLE(handle.as_raw_handle()))?
            != super::pipe::process_logon_sid(unsafe { GetCurrentProcess() })?
        {
            bail!("pipe server belongs to another Windows logon");
        }
        Ok(Self {
            handle,
            identity: identity.clone(),
        })
    }

    pub fn identity(&self) -> &InstanceIdentity {
        &self.identity
    }

    pub fn has_exited(&self) -> Result<bool> {
        self.wait(0)
    }

    /// Blocking operation for a worker, never an input/audio/UI callback.
    /// False means the deadline expired, not that shutdown or absence is proved.
    pub fn wait_until(&self, deadline: Instant) -> Result<bool> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            // Finite slices recheck the caller's deadline, including after resume.
            let milliseconds = remaining.as_millis().min(50) as u32;
            let milliseconds = if remaining.is_zero() {
                0
            } else {
                milliseconds.max(1)
            };
            if self.wait(milliseconds)? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                return Ok(false);
            }
        }
    }

    fn wait(&self, milliseconds: u32) -> Result<bool> {
        // &self keeps the handle alive for the entire wait.
        match unsafe { WaitForSingleObject(HANDLE(self.handle.as_raw_handle()), milliseconds) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            WAIT_FAILED => Err(io::Error::last_os_error()).context("wait for Windows server exit"),
            result => bail!("unexpected process wait result: {}", result.0),
        }
    }
}

fn session_id(process: HANDLE) -> Result<u32> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token)? };
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let mut session = 0u32;
    let mut returned = 0;
    unsafe {
        GetTokenInformation(
            HANDLE(token.as_raw_handle()),
            TokenSessionId,
            Some((&mut session as *mut u32).cast()),
            std::mem::size_of::<u32>() as u32,
            &mut returned,
        )?;
    }
    if returned != std::mem::size_of::<u32>() as u32 {
        bail!("unexpected Windows token session size");
    }
    Ok(session)
}

fn creation_time(handle: HANDLE) -> Result<u64> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe { GetProcessTimes(handle, &mut creation, &mut exit, &mut kernel, &mut user)? };
    Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
}
