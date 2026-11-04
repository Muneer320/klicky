//! Windows stream ownership and recovery stay off engine/input/GUI callbacks.
mod notifications;
mod sink;
mod voices;

use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::System::Threading::WaitForSingleObject;
use anyhow::{bail, Context, Result};
use cpal_windows::traits::HostTrait;
use cpal_windows::{BufferSize, DeviceId, HostId};
use crossbeam_channel::{bounded, Receiver, Sender};
use sink::Sink;

pub use voices::{Output, MAX_VOICES};

const HEALTH_INTERVAL: Duration = Duration::from_millis(250);
const STALL_LIMIT: Duration = Duration::from_secs(1);
const RETRY_INTERVAL: Duration = Duration::from_secs(1);

pub enum Update {
    Ready(Output),
    Waiting(String),
    Recovering(String),
    Inactive,
    Stopped(std::result::Result<(), String>),
}

struct Mailbox {
    latest: Mutex<Option<Update>>,
    signal: Sender<()>,
}
impl Mailbox {
    fn publish(&self, update: Update) {
        if let Ok(mut latest) = self.latest.lock() {
            *latest = Some(update);
            let _ = self.signal.try_send(());
        }
    }
}

struct Shared {
    stop: AtomicBool,
    active: AtomicBool,
    epoch: AtomicU64,
    retry: AtomicBool,
    route_dirty: AtomicBool,
    device_id: Mutex<Vec<u16>>,
    changed: Sender<()>,
}
impl Shared {
    fn wake(&self) {
        let _ = self.changed.try_send(());
    }
}

struct Render {
    shared: Arc<Shared>,
    epoch: u64,
    valid: AtomicBool,
    enabled: AtomicBool,
    failed: AtomicBool,
    heartbeat: AtomicU64,
    generation: AtomicU64,
    voices: AtomicUsize,
    voice_drops: AtomicU64,
}
impl Render {
    fn live(&self) -> bool {
        self.valid.load(Ordering::Acquire)
            && self.enabled.load(Ordering::Acquire)
            && voices::shared_live(&self.shared, self.epoch)
    }
}

pub struct Supervisor {
    shared: Arc<Shared>,
    mailbox: Arc<Mailbox>,
    updates: Receiver<()>,
    worker: Option<JoinHandle<std::result::Result<(), String>>>,
}
impl Supervisor {
    /// Starts inactive and opens no stream until set_active(true).
    pub fn spawn() -> Result<Self> {
        let (changed, changes) = bounded(1);
        let (signal, updates) = bounded(1);
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            active: AtomicBool::new(false),
            epoch: AtomicU64::new(1),
            retry: AtomicBool::new(false),
            route_dirty: AtomicBool::new(false),
            device_id: Mutex::new(Vec::new()),
            changed,
        });
        let mailbox = Arc::new(Mailbox {
            latest: Mutex::new(None),
            signal,
        });
        let flags = Arc::clone(&shared);
        let results = Arc::clone(&mailbox);
        let worker = thread::Builder::new()
            .name("klicky-audio".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(flags, changes, &results)
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("audio owner panicked")));
                let result = result.map_err(|error| format!("{error:#}"));
                results.publish(Update::Stopped(result.clone()));
                result
            })?;
        Ok(Self {
            shared,
            mailbox,
            updates,
            worker: Some(worker),
        })
    }

    pub fn updates(&self) -> &Receiver<()> {
        &self.updates
    }
    pub fn take_update(&self) -> Option<Update> {
        let _ = self.updates.try_recv();
        self.mailbox.latest.lock().ok()?.take()
    }
    pub fn has_exited(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub fn shutdown_complete(&self) -> bool {
        self.worker.is_none()
    }
    pub fn set_active(&self, active: bool) {
        if self.shared.active.swap(active, Ordering::AcqRel) != active {
            self.shared.epoch.fetch_add(1, Ordering::AcqRel);
            self.shared.wake();
        }
    }
    pub fn retry(&self) {
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        self.shared.retry.store(true, Ordering::Release);
        self.shared.wake();
    }
    pub fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake();
    }
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
                    bail!("audio owner shutdown timed out")
                }
                WAIT_TIMEOUT => (),
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => bail!("unexpected audio owner wait result"),
            }
        }
        self.worker
            .take()
            .expect("observed audio owner")
            .join()
            .map_err(|_| anyhow::anyhow!("audio owner panicked"))?
            .map_err(anyhow::Error::msg)?;
        Ok(())
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        self.request_stop();
    }
}

struct Playing {
    sink: Sink,
    render: Arc<Render>,
    id: Vec<u16>,
    last_heartbeat: u64,
    pulse_at: Instant,
    announced: bool,
}
impl Drop for Playing {
    fn drop(&mut self) {
        self.render.valid.store(false, Ordering::Release);
    }
}

fn open(shared: &Arc<Shared>, notifications: &notifications::Notifications) -> Result<Playing> {
    let id = notifications.default_id()?;
    // Select the observed multimedia endpoint explicitly so playback and
    // notifications cannot diverge when Windows default roles differ.
    let device_id = DeviceId::new(HostId::Wasapi, String::from_utf16(&id)?);
    let device = cpal_windows::default_host()
        .device_by_id(&device_id)
        .context("default multimedia audio endpoint is unavailable")?;
    let mut last_error = None;
    for buffer in crate::player::BUFFER_SIZES
        .into_iter()
        .map(BufferSize::Fixed)
        .chain(std::iter::once(BufferSize::Default))
    {
        if shared.stop.load(Ordering::Acquire) || !shared.active.load(Ordering::Acquire) {
            bail!("audio opening cancelled");
        }
        let render = Arc::new(Render {
            shared: Arc::clone(shared),
            epoch: shared.epoch.load(Ordering::Acquire),
            valid: AtomicBool::new(false),
            enabled: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            heartbeat: AtomicU64::new(0),
            generation: AtomicU64::new(1),
            voices: AtomicUsize::new(0),
            voice_drops: AtomicU64::new(0),
        });
        let errors = Arc::clone(&render);
        let opened = Sink::open(&device, buffer, move |_| {
            errors.failed.store(true, Ordering::Release);
            errors.shared.wake();
        });
        match opened {
            Ok(sink) => {
                if notifications.default_id()? != id {
                    bail!("default audio changed during initialization");
                }
                sink.mixer().add(voices::Heartbeat {
                    render: Arc::clone(&render),
                    channels: sink.channel_count(),
                    rate: sink.sample_rate(),
                    until_tick: 0,
                });
                *shared
                    .device_id
                    .lock()
                    .map_err(|_| anyhow::anyhow!("audio route lock poisoned"))? = id.clone();
                return Ok(Playing {
                    sink,
                    render,
                    id,
                    last_heartbeat: 0,
                    pulse_at: Instant::now(),
                    announced: false,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.context("no audio configuration tried")?)
}

fn run(shared: Arc<Shared>, changes: Receiver<()>, mailbox: &Mailbox) -> Result<()> {
    let mut notifications = notifications::Notifications::register(Arc::clone(&shared))?;
    let mut playing: Option<Playing> = None;
    let mut retry_at = Instant::now();
    let mut inactive = false;
    loop {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let active = shared.active.load(Ordering::Acquire);
        let retry = shared.retry.swap(false, Ordering::AcqRel);
        let dirty = shared.route_dirty.swap(false, Ordering::AcqRel);
        if !active {
            if !inactive {
                mailbox.publish(Update::Inactive);
                inactive = true;
            }
            playing.take();
        } else {
            if inactive {
                retry_at = Instant::now();
                inactive = false;
            }
            let mut reason = None;
            let mut backoff = false;
            if let Some(current) = playing.as_mut() {
                if retry || !voices::shared_live(&shared, current.render.epoch) {
                    reason = Some("audio retry requested");
                } else if dirty && notifications.default_id().ok().as_ref() != Some(&current.id) {
                    reason = Some("default audio endpoint changed");
                } else if current.render.failed.load(Ordering::Acquire) {
                    reason = Some("audio stream reported an error");
                    backoff = true;
                } else {
                    let heartbeat = current.render.heartbeat.load(Ordering::Relaxed);
                    if heartbeat != current.last_heartbeat {
                        current.last_heartbeat = heartbeat;
                        current.pulse_at = Instant::now();
                        if !current.announced {
                            current.render.valid.store(true, Ordering::Release);
                            mailbox.publish(Update::Ready(Output {
                                description: current.sink.description().to_owned(),
                                mixer: current.sink.mixer().clone(),
                                render: Arc::clone(&current.render),
                            }));
                            current.announced = true;
                        }
                    } else if current.pulse_at.elapsed() >= STALL_LIMIT {
                        reason = Some("audio render heartbeat stopped");
                    }
                }
            }
            if let Some(reason) = reason {
                if let Some(current) = playing.as_ref() {
                    current.render.valid.store(false, Ordering::Release);
                }
                mailbox.publish(Update::Recovering(reason.into()));
                // CPAL teardown joins its renderer and can stall under a broken
                // driver. Publish degradation before touching that blocking path.
                playing.take();
                retry_at = Instant::now()
                    + if backoff {
                        RETRY_INTERVAL
                    } else {
                        Duration::ZERO
                    };
            }
            if playing.is_none() && (retry || dirty || Instant::now() >= retry_at) {
                match open(&shared, &notifications) {
                    Ok(stream) => playing = Some(stream),
                    Err(error) => {
                        mailbox.publish(Update::Waiting(format!("{error:#}")));
                        retry_at = Instant::now() + RETRY_INTERVAL;
                    }
                }
            }
        }
        let wait = if !active {
            Duration::from_secs(3600)
        } else if playing.as_ref().is_some_and(|stream| !stream.announced) {
            Duration::from_millis(10)
        } else if playing.is_some() {
            HEALTH_INTERVAL
        } else {
            retry_at
                .saturating_duration_since(Instant::now())
                .min(RETRY_INTERVAL)
        };
        let _ = changes.recv_timeout(wait);
    }
    drop(playing);
    notifications.close()
}
