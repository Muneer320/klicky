//! One blocking pack/configuration worker; the engine remains the sole activator.
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

use ::windows::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::System::Threading::WaitForSingleObject;
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, Receiver, Sender};

use super::{ConfigStore, Paths};
use crate::{Config, SoundPack};

#[derive(Clone, Debug)]
pub enum Change {
    Bootstrap,
    Switch(String),
    Volume(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Persisting,
    Finished,
}

#[derive(Clone)]
pub struct Ticket {
    cancelled: Arc<AtomicBool>,
    phase: Arc<AtomicU8>,
}
impl Ticket {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn phase(&self) -> Phase {
        match self.phase.load(Ordering::Acquire) {
            0 => Phase::Preparing,
            1 => Phase::Persisting,
            _ => Phase::Finished,
        }
    }
}

pub struct Prepared {
    /// A persisted snapshot, not proof of active playback in any session.
    pub config: Config,
    pub pack: Option<SoundPack>,
}

pub struct Completion {
    pub change: Change,
    /// Cancellation can race an atomic save. A successful result must then be
    /// handled as persisted for next start, never acknowledged as active.
    pub cancelled_or_expired: bool,
    pub result: Result<Prepared>,
}

struct Job {
    change: Change,
    deadline: Instant,
    ticket: Ticket,
}

pub struct Worker {
    jobs: Option<Sender<Job>>,
    completed: Receiver<Completion>,
    active: Option<Ticket>,
    worker: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(paths: Paths) -> Result<Self> {
        let (jobs, receiver) = bounded::<Job>(1);
        let (sender, completed) = bounded(1);
        let worker = thread::Builder::new()
            .name("klicky-packs".into())
            .spawn(move || {
                let store = ConfigStore::new(paths.clone());
                while let Ok(job) = receiver.recv() {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        prepare(&paths, &store, &job)
                    }))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "pack worker panicked; a persistence outcome may be unknown"
                        ))
                    });
                    job.ticket.phase.store(2, Ordering::Release);
                    let completion = Completion {
                        change: job.change,
                        cancelled_or_expired: job.ticket.cancelled.load(Ordering::Acquire)
                            || Instant::now() >= job.deadline,
                        result,
                    };
                    if sender.send(completion).is_err() {
                        break;
                    }
                }
            })?;
        Ok(Self {
            jobs: Some(jobs),
            completed,
            active: None,
            worker: Some(worker),
        })
    }

    pub fn submit(&mut self, change: Change, deadline: Instant) -> Result<Ticket> {
        if self.active.is_some() {
            bail!("another configuration transaction is in progress");
        }
        if Instant::now() >= deadline {
            bail!("configuration transaction deadline expired");
        }
        let ticket = Ticket {
            cancelled: Arc::new(AtomicBool::new(false)),
            phase: Arc::new(AtomicU8::new(0)),
        };
        self.jobs
            .as_ref()
            .context("pack worker is stopping")?
            .try_send(Job {
                change,
                deadline,
                ticket: ticket.clone(),
            })
            .map_err(|_| anyhow::anyhow!("pack worker is unavailable"))?;
        self.active = Some(ticket.clone());
        Ok(ticket)
    }

    /// Use for readiness selection; consume through finish() to release the slot.
    pub fn completed(&self) -> &Receiver<Completion> {
        &self.completed
    }

    pub fn finish(&mut self) -> Option<Completion> {
        let result = self.completed.try_recv().ok()?;
        self.active = None;
        Some(result)
    }

    pub fn is_busy(&self) -> bool {
        self.active.is_some()
    }

    pub fn request_stop(&mut self) {
        if let Some(ticket) = &self.active {
            ticket.cancel();
        }
        self.jobs.take();
    }

    /// Keep the host's instance lease until this confirms exit. A blocked OS read
    /// or save cannot be made safe by terminating a thread or freeing its buffers.
    pub fn shutdown(&mut self, deadline: Instant) -> Result<()> {
        self.request_stop();
        let Some(worker) = self.worker.as_ref() else {
            return Ok(());
        };
        loop {
            match unsafe {
                WaitForSingleObject(
                    HANDLE(worker.as_raw_handle()),
                    super::pipe::io::wait_milliseconds(deadline),
                )
            } {
                WAIT_OBJECT_0 => break,
                WAIT_TIMEOUT if Instant::now() >= deadline => {
                    bail!("pack worker shutdown timed out")
                }
                WAIT_TIMEOUT => (),
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => bail!("unexpected pack worker wait result"),
            }
        }
        self.worker
            .take()
            .expect("observed pack worker")
            .join()
            .map_err(|_| anyhow::anyhow!("pack worker panicked"))?;
        Ok(())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn prepare(paths: &Paths, store: &ConfigStore, job: &Job) -> Result<Prepared> {
    let cancelled =
        || job.ticket.cancelled.load(Ordering::Acquire) || Instant::now() >= job.deadline;
    let check = || -> Result<()> {
        if cancelled() {
            bail!("configuration transaction cancelled or expired");
        }
        Ok(())
    };
    check()?;
    let persist = |patch: &mut dyn FnMut(&mut Config) -> Result<()>| -> Result<Config> {
        check()?;
        job.ticket.phase.store(1, Ordering::Release);
        store.update(|config| {
            check()?;
            patch(config)
        })
    };
    match &job.change {
        Change::Bootstrap => {
            let mut config = store.load()?;
            check()?;
            let mut fallback = false;
            if paths.resolve_sound_pack(&config.sound_pack)?.is_none() {
                config.sound_pack = paths
                    .available_sound_packs()?
                    .into_iter()
                    .next()
                    .context("no sound packs found")?;
                fallback = true;
            }
            let directory = paths
                .resolve_sound_pack(&config.sound_pack)?
                .context("selected sound pack disappeared")?;
            let pack = SoundPack::load_bounded(&directory, &cancelled)?;
            if fallback {
                let name = config.sound_pack.clone();
                config = persist(&mut |config| {
                    config.sound_pack.clone_from(&name);
                    Ok(())
                })?;
            }
            Ok(Prepared {
                config,
                pack: Some(pack),
            })
        }
        Change::Switch(name) => {
            let directory = paths
                .resolve_sound_pack(name)?
                .context("sound pack not found")?;
            let pack = SoundPack::load_bounded(&directory, &cancelled)?;
            let config = persist(&mut |config| {
                config.sound_pack.clone_from(name);
                Ok(())
            })?;
            Ok(Prepared {
                config,
                pack: Some(pack),
            })
        }
        Change::Volume(level) => {
            let config = persist(&mut |config| {
                config.volume = *level;
                Ok(())
            })?;
            Ok(Prepared { config, pack: None })
        }
    }
}
