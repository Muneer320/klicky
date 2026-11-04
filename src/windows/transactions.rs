//! One blocking pack/configuration worker; the engine remains the sole activator.
use std::collections::HashMap;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::System::Threading::WaitForSingleObject;
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, never, select_biased, Receiver, Sender};

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
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
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
    pub phase: Phase,
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
    retired: Option<Sender<SoundPack>>,
    retiring: Arc<AtomicUsize>,
    availability: Receiver<()>,
}

const MAX_RETIRING: usize = 2;

struct Retired {
    pack: SoundPack,
    internal: HashMap<usize, usize>,
}
impl Retired {
    fn new(pack: SoundPack) -> Self {
        let mut internal = HashMap::new();
        for samples in pack.samples.values() {
            *internal.entry(Arc::as_ptr(samples) as usize).or_default() += 1;
        }
        Self { pack, internal }
    }
    fn ready(&self) -> bool {
        self.pack.samples.values().all(|samples| {
            Arc::strong_count(samples) <= self.internal[&(Arc::as_ptr(samples) as usize)]
        })
    }
}

impl Worker {
    pub fn spawn(paths: Paths) -> Result<Self> {
        let (jobs, receiver) = bounded::<Job>(1);
        let (sender, completed) = bounded(1);
        let (retired, mut retire_rx) = bounded(MAX_RETIRING);
        let retiring = Arc::new(AtomicUsize::new(0));
        let count = Arc::clone(&retiring);
        let (available, availability) = bounded(1);
        let worker = thread::Builder::new()
            .name("klicky-packs".into())
            .spawn(move || {
                let store = ConfigStore::new(paths.clone());
                let mut old: Vec<Retired> = Vec::with_capacity(MAX_RETIRING);
                loop {
                    old.retain(|pack| {
                        if pack.ready() {
                            count.fetch_sub(1, Ordering::AcqRel);
                            let _ = available.try_send(());
                            false
                        } else { true }
                    });
                    let wait = if old.is_empty() { Duration::from_secs(3600) }
                        else { Duration::from_millis(100) };
                    let job = select_biased! {
                        recv(retire_rx) -> pack => {
                            match pack { Ok(pack) => old.push(Retired::new(pack)), Err(_) => retire_rx = never() }
                            continue;
                        },
                        recv(receiver) -> job => match job { Ok(job) => job, Err(_) => break },
                        default(wait) => continue,
                    };
                    let mut result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        prepare(&paths, &store, &job)
                    }))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!(
                            "pack worker panicked; a persistence outcome may be unknown"
                        ))
                    });
                    let cancelled_or_expired = job.ticket.is_cancelled() || Instant::now() >= job.deadline;
                    if cancelled_or_expired {
                        if let Ok(prepared) = result.as_mut() { prepared.pack.take(); }
                    }
                    let phase = job.ticket.phase();
                    job.ticket.phase.store(2, Ordering::Release);
                    let completion = Completion {
                        change: job.change,
                        cancelled_or_expired,
                        phase,
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
            retired: Some(retired),
            retiring,
            availability,
        })
    }

    pub fn submit(&mut self, change: Change, deadline: Instant) -> Result<Ticket> {
        if self.is_busy() {
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
        self.active.is_some() || self.retiring.load(Ordering::Acquire) >= MAX_RETIRING
    }

    pub fn availability(&self) -> &Receiver<()> {
        &self.availability
    }

    /// Invalidate old voices before retiring. Keep audio alive until its sources
    /// release their Arcs, and close audio before stopping this worker.
    pub fn retire(&self, pack: SoundPack) -> std::result::Result<(), SoundPack> {
        if self
            .retiring
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_RETIRING).then_some(count + 1)
            })
            .is_err()
        {
            return Err(pack);
        }
        let result = match &self.retired {
            Some(sender) => sender.try_send(pack).map_err(|error| error.into_inner()),
            None => Err(pack),
        };
        if result.is_err() {
            self.retiring.fetch_sub(1, Ordering::AcqRel);
        }
        result
    }

    pub fn has_exited(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn request_stop(&mut self) {
        if let Some(ticket) = &self.active {
            ticket.cancel();
        }
        self.jobs.take();
        self.retired.take();
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
