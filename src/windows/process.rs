use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::path::PathBuf;
use std::time::Instant;

use ::windows::Win32::Foundation::{FILETIME, HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG};
use ::windows::Win32::Security::{GetTokenInformation, TokenSessionId, TOKEN_QUERY};
use ::windows::Win32::System::Pipes::GetNamedPipeServerProcessId;
use ::windows::Win32::System::Threading::{
    CreateProcessW, GetCurrentProcess, GetCurrentProcessId, GetExitCodeProcess, GetProcessTimes,
    OpenProcess, OpenProcessToken, QueryFullProcessImageNameW, WaitForSingleObject,
    CREATE_NO_WINDOW, PROCESS_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_SYNCHRONIZE, STARTUPINFOW,
};
use anyhow::{bail, Context, Result};

use super::protocol::InstanceIdentity;

/// A background app must not retain the CLI caller's redirected pipes or other
/// inheritable handles. Rust 1.87 Command::spawn enables generic inheritance even
/// with all three standard streams set to null, so use explicit Win32 creation.
pub(super) struct LaunchedProcess(OwnedHandle);

pub(super) fn spawn_desktop(paths: &super::Paths, paused: bool) -> Result<LaunchedProcess> {
    use ::windows::core::{PCWSTR, PWSTR};
    let executable = super::bundle::desktop_path(paths)?;
    let executable: Vec<u16> = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let directory: Vec<u16> = paths
        .bundle_dir()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // Application name is an explicit full path; no shell or PATH search. Only
    // argv[0] and our fixed optional flag are passed in the mutable command line.
    let command = if paused {
        "klicky-desktop.exe --paused"
    } else {
        "klicky-desktop.exe"
    };
    let mut command: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    unsafe {
        CreateProcessW(
            PCWSTR(executable.as_ptr()),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NO_WINDOW,
            None,
            PCWSTR(directory.as_ptr()),
            &startup,
            &mut process,
        )
        .context("launch sibling Klicky desktop")?;
    }
    // Both successful-create handles transfer immediately into owning wrappers.
    let handle = unsafe { OwnedHandle::from_raw_handle(process.hProcess.0) };
    drop(unsafe { OwnedHandle::from_raw_handle(process.hThread.0) });
    Ok(LaunchedProcess(handle))
}

impl LaunchedProcess {
    pub(super) fn try_wait(&self) -> Result<Option<u32>> {
        let handle = HANDLE(self.0.as_raw_handle());
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut code = 0;
                unsafe { GetExitCodeProcess(handle, &mut code)? };
                Ok(Some(code))
            }
            WAIT_FAILED => Err(io::Error::last_os_error()).context("inspect launched desktop exit"),
            result => bail!("unexpected process wait result: {}", result.0),
        }
    }
}

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

    /// Query the retained object; never reopen a PID to decide which bundle owns it.
    pub fn image_path(&self) -> Result<PathBuf> {
        let mut path = vec![0u16; 32768];
        let mut length = path.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                HANDLE(self.handle.as_raw_handle()),
                PROCESS_NAME_WIN32,
                ::windows::core::PWSTR(path.as_mut_ptr()),
                &mut length,
            )?
        };
        if length == 0 || length as usize >= path.len() {
            bail!("Windows returned an invalid process image path");
        }
        Ok(std::ffi::OsString::from_wide(&path[..length as usize]).into())
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
