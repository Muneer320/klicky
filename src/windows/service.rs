//! Per-user desktop launch and verified bundle-specific lifecycle commands.
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use super::bundle;
use super::control::{Client, ControlError, COMMAND_TIMEOUT};
use super::process::{spawn_desktop, LaunchedProcess};
use super::protocol::{Command, EngineState, Outcome, Snapshot, UpdateState};
use super::startup::{ForeignShortcut, Startup, Status};
use super::{InstanceOwner, Paths, ServerProcess};

pub(super) fn start(paths: &Paths) -> Result<Snapshot> {
    start_with_pause(paths, false)
}

pub(super) fn start_with_pause(paths: &Paths, paused: bool) -> Result<Snapshot> {
    let client = Client::current()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut child: Option<LaunchedProcess> = None;
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
                    Outcome::UpdatePrepared { .. } => {
                        bail!("Unexpected installer preparation response")
                    }
                    Outcome::Diagnostics { .. } => bail!("Unexpected diagnostics response"),
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
                        child = Some(spawn_desktop(paths, paused)?);
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
    if let Some(process) = matching_process(paths, &client, deadline)? {
        client.stop_for(&process, deadline)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub(super) fn prepare_update(paths: &Paths) -> Result<UpdateState> {
    let client = Client::current()?;
    let deadline = Instant::now() + COMMAND_TIMEOUT;
    match matching_process(paths, &client, deadline)? {
        Some(process) => Ok(client.prepare_update_for(&process, deadline)?),
        None => Ok(UpdateState {
            restart: false,
            paused: false,
        }),
    }
}

fn matching_process(
    paths: &Paths,
    client: &Client,
    deadline: Instant,
) -> Result<Option<ServerProcess>> {
    match client.command(Command::Status {}, deadline) {
        Ok(reply) => {
            if !bundle::owns_image(paths, &reply.process.image_path()?)? {
                return Ok(None);
            }
            Ok(Some(reply.process))
        }
        Err(ControlError::Absent) => {
            let _owner = InstanceOwner::try_acquire(paths)?.context(
                "Session ownership exists without a verified endpoint; update must wait",
            )?;
            Ok(None)
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
    match Startup::current(paths)?.set(false) {
        Err(error) if error.is::<ForeignShortcut>() => Ok(Status::Foreign),
        result => result,
    }
}

pub(super) fn startup_status(paths: Paths) -> Result<Status> {
    Startup::current(paths)?.status()
}

fn short_deadline(deadline: Instant) -> Instant {
    deadline.min(Instant::now() + COMMAND_TIMEOUT)
}
