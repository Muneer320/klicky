//! Shortcut operations have their own owned worker; pack discovery stays read-only.
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use ::windows::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForSingleObject,
};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Sender};

use super::super::startup::{self, Startup, Status};
use super::super::Paths;

pub(super) enum Request {
    Check,
    Set(bool),
    Settings,
    Stop,
}
pub(super) struct Update {
    pub status: Option<std::result::Result<Status, String>>,
    pub error: Option<String>,
}
struct Shared {
    event: OwnedHandle,
    result: Mutex<Option<Update>>,
    pending: AtomicBool,
    stop: AtomicBool,
}
pub(super) struct Worker {
    shared: Arc<Shared>,
    commands: Sender<Request>,
    thread: Option<JoinHandle<std::result::Result<(), String>>>,
}
impl Worker {
    pub fn spawn(paths: Paths) -> Result<Self> {
        let event = unsafe { CreateEventW(None, true, false, None)? };
        let shared = Arc::new(Shared {
            event: unsafe { OwnedHandle::from_raw_handle(event.0) },
            result: Mutex::new(None),
            pending: AtomicBool::new(false),
            stop: AtomicBool::new(false),
        });
        let (commands, requests) = bounded(1);
        let flags = Arc::clone(&shared);
        let thread = thread::Builder::new()
            .name("klicky-startup".into())
            .spawn(move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    while let Ok(request) = requests.recv() {
                        if flags.stop.load(Ordering::Acquire) {
                            break;
                        }
                        let update = match request {
                            Request::Check | Request::Set(_) => {
                                let result =
                                    Startup::current(paths.clone()).and_then(
                                        |startup| match request {
                                            Request::Set(enabled) => startup.set(enabled),
                                            _ => startup.status(),
                                        },
                                    );
                                Update {
                                    status: Some(result.map_err(|error| format!("{error:#}"))),
                                    error: None,
                                }
                            }
                            Request::Settings => Update {
                                status: None,
                                error: startup::open_settings()
                                    .err()
                                    .map(|error| format!("{error:#}")),
                            },
                            Request::Stop => break,
                        };
                        let mut slot = flags
                            .result
                            .lock()
                            .unwrap_or_else(|error| error.into_inner());
                        *slot = Some(update);
                        unsafe {
                            let _ = SetEvent(HANDLE(flags.event.as_raw_handle()));
                        }
                    }
                }))
                .map_err(|_| "Windows startup worker panicked".into())
            })?;
        let worker = Self {
            shared,
            commands,
            thread: Some(thread),
        };
        worker.submit(Request::Check)?;
        Ok(worker)
    }
    pub fn submit(&self, request: Request) -> Result<()> {
        if self.shared.stop.load(Ordering::Acquire) {
            bail!("Login startup work is stopping");
        }
        if self
            .shared
            .pending
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            bail!("Login startup work is busy");
        }
        if self.commands.try_send(request).is_err() {
            self.shared.pending.store(false, Ordering::Release);
            bail!("Login startup worker is unavailable");
        }
        Ok(())
    }
    pub fn busy(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }
    pub fn changed_handle(&self) -> HANDLE {
        HANDLE(self.shared.event.as_raw_handle())
    }
    // Only the UI owner joins this handle, outside callbacks and native waits.
    pub fn thread_handle(&self) -> Option<HANDLE> {
        self.thread
            .as_ref()
            .map(|worker| HANDLE(worker.as_raw_handle()))
    }
    pub fn take(&self) -> Result<Option<Update>> {
        let mut slot = self
            .shared
            .result
            .lock()
            .map_err(|_| anyhow::anyhow!("Startup result lock poisoned"))?;
        unsafe { ResetEvent(self.changed_handle())? };
        let result = slot.take();
        if result.is_some() {
            self.shared.pending.store(false, Ordering::Release);
        }
        Ok(result)
    }
    pub fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        let _ = self.commands.try_send(Request::Stop);
    }
    pub fn stopping(&self) -> bool {
        self.shared.stop.load(Ordering::Acquire)
    }
    pub fn shutdown_complete(&self) -> bool {
        self.thread.is_none()
    }
    pub fn try_join(&mut self) -> Result<bool> {
        let Some(handle) = self.thread_handle() else {
            return Ok(true);
        };
        match unsafe { WaitForSingleObject(handle, 0) } {
            WAIT_TIMEOUT => Ok(false),
            WAIT_OBJECT_0 => {
                self.thread
                    .take()
                    .context("Startup thread disappeared")?
                    .join()
                    .map_err(|_| anyhow::anyhow!("Startup worker panicked at exit"))?
                    .map_err(anyhow::Error::msg)?;
                Ok(true)
            }
            WAIT_FAILED => Err(::windows::core::Error::from_thread().into()),
            _ => bail!("Unexpected startup thread wait result"),
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.request_stop();
        // The ordinary UI loop joins asynchronously. Initialization/error unwinding
        // also must not abandon a writer. The process reader lease stays held.
        while !self.shutdown_complete() {
            let _ = self.try_join();
            if !self.shutdown_complete() {
                thread::sleep(std::time::Duration::from_millis(25));
            }
        }
    }
}
