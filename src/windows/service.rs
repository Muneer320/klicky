//! Per-user desktop launch and verified bundle-specific lifecycle commands.
use std::os::windows::process::CommandExt;
use std::process::{Child, Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use super::bundle;
use super::control::{Client, ControlError, COMMAND_TIMEOUT};
use super::protocol::{Command, EngineState, Outcome, Snapshot};
use super::startup::{Startup, Status};
use super::{InstanceOwner, Paths};

pub(super) fn start(paths: &Paths) -> Result<Snapshot> {
    let client = Client::current()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut child: Option<Child> = None;
    let mut attempts = 0;
    loop {
        if Instant::now() >= deadline {
            bail!("Klicky launch was not confirmed within 15 seconds; no process was terminated");
        }
        match client.command(Command::Status {}, short_deadline(deadline)) {
            Ok(reply) => {
                if !bundle::owns_image(paths, &reply.process.image_path()?)? {
                    bail!("Klicky is running from another bundle; it was left unchanged");
                }
                let snapshot = match reply.response.outcome {
                    Outcome::Success { snapshot } => snapshot,
                    Outcome::Failure { message, .. } => bail!("{message}"),
                };
                match snapshot.state {
                    EngineState::Failed => bail!("Klicky started but initialization failed; use the tray to inspect and recover"),
                    EngineState::Stopping => bail!("Klicky is stopping; wait for exit before starting again"),
                    EngineState::Starting => (),
                    // These are explicit states, not an assertion that audio is ready.
                    _ => return Ok(snapshot),
                }
            }
            Err(ControlError::Absent) => {
                if let Some(process) = child.as_mut() {
                    if let Some(status) = process.try_wait()? {
                        child = None;
                        if attempts == 2 {
                            bail!("Klicky desktop exited before readiness ({status}); run klicky start for diagnostics");
                        }
                    }
                }
                if child.is_none() {
                    if let Some(owner) = InstanceOwner::try_acquire(paths)? {
                        drop(owner);
                        if Instant::now() >= deadline {
                            bail!(
                                "Readiness deadline expired before launch; no process was started"
                            );
                        }
                        let executable = bundle::desktop_path(paths)?;
                        child = Some(
                            Process::new(executable)
                                .current_dir(paths.bundle_dir())
                                .stdin(Stdio::null())
                                .stdout(Stdio::null())
                                .stderr(Stdio::null())
                                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW; never a shell command
                                .spawn()
                                .context("launch sibling Klicky desktop")?,
                        );
                        attempts += 1;
                    }
                }
            }
            // An existing owner may be initializing or saturated. Do not launch
            // another copy, write offline settings or terminate it on a timeout.
            Err(ControlError::Busy | ControlError::Timeout) => (),
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            bail!("Klicky launch was not confirmed within 15 seconds; no process was terminated");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Only stop a verified process from this bundle. A different portable copy is
/// deliberately left running; its own bundle reader lease remains independent.
pub(super) fn stop_for_update(paths: &Paths) -> Result<bool> {
    let client = Client::current()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    match client.command(Command::Status {}, deadline) {
        Ok(reply) => {
            if !bundle::owns_image(paths, &reply.process.image_path()?)? {
                return Ok(false);
            }
            client.stop_for(&reply.process, deadline)?;
            Ok(true)
        }
        Err(ControlError::Absent) => {
            let _owner = InstanceOwner::try_acquire(paths)?.context(
                "Session ownership exists without a verified endpoint; update must wait",
            )?;
            Ok(false)
        }
        Err(error) => Err(error.into()),
    }
}

pub(super) fn startup(paths: Paths, enabled: bool) -> Result<Status> {
    // Normal mutations participate in the installer interlock. The installer
    // removal hook already holds the exclusive marker and must not acquire it again.
    let _reader = bundle::acquire(paths.bundle_dir())?;
    Startup::current(paths)?.set(enabled)
}

pub(super) fn unregister_for_removal(paths: Paths) -> Result<Status> {
    Startup::current(paths)?.set(false)
}

pub(super) fn startup_status(paths: Paths) -> Result<Status> {
    Startup::current(paths)?.status()
}

fn short_deadline(deadline: Instant) -> Instant {
    deadline.min(Instant::now() + COMMAND_TIMEOUT)
}
