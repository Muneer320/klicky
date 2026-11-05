//! One coordinator activates prepared state and retains ownership through teardown.
use std::os::windows::io::{AsHandle, AsRawHandle, BorrowedHandle, FromRawHandle, OwnedHandle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::{HANDLE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT};
use ::windows::Win32::System::Threading::{
    CreateEventW, ResetEvent, SetEvent, WaitForSingleObject,
};
use anyhow::{bail, Context, Result};
use crossbeam_channel::{bounded, never, Receiver, Select, Sender, TryRecvError};
use serde::Serialize;

use super::audio::{self, Output, Supervisor};
use super::control::{PendingCommand, Server};
use super::input::{self, Input, Press};
use super::pipe::PipePool;
use super::protocol::{Command, EngineState, ErrorCode, Outcome, Snapshot};
use super::transactions::{Change, Phase, Ticket, Worker};
use super::{InstanceOwner, Paths};
use crate::{Config, SoundPack};

const TICK: Duration = Duration::from_millis(250);
const STOP_LIMIT: Duration = Duration::from_secs(3);
const BOOT_LIMIT: Duration = Duration::from_secs(15);

#[derive(Default)]
pub struct Options {
    pub paused: bool,
    pub benchmark: bool,
    pub duration: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Diagnostics {
    pub dispatched: u64,
    pub stale: u64,
    pub obsolete_epoch: u64,
    pub input_overflow: u64,
    pub voice_drops: u64,
    /// Callback entry to mixer enqueue. Final bucket includes >=10 ms.
    pub dispatch_buckets_10us: Vec<u64>,
    pub dispatch_max_us: u64,
    pub audible_latency_qualified: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub snapshot: Snapshot,
    pub busy: bool,
    pub last_error: Option<String>,
    pub finished: bool,
    pub already_running: bool,
    pub diagnostics: Option<Diagnostics>,
}

struct Shared {
    view: Mutex<View>,
    commands: Mutex<Option<Sender<PendingCommand>>>,
    changed: OwnedHandle,
    stop: AtomicBool,
    desktop_active: AtomicBool,
    desktop_epoch: AtomicU64,
    wake: Sender<()>,
}

#[derive(Clone)]
pub struct Controller {
    shared: Arc<Shared>,
}
impl Controller {
    pub fn changed_handle(&self) -> BorrowedHandle<'_> {
        self.shared.changed.as_handle()
    }
    pub fn take_view(&self) -> Result<View> {
        unsafe { ResetEvent(HANDLE(self.shared.changed.as_raw_handle()))? };
        self.shared
            .view
            .lock()
            .map(|view| view.clone())
            .map_err(|_| anyhow::anyhow!("runtime view lock poisoned"))
    }
    pub fn command(&self, command: Command, deadline: Instant) -> Result<Receiver<Outcome>> {
        let (pending, reply) = PendingCommand::local(command, deadline)?;
        let sender = self
            .shared
            .commands
            .lock()
            .map_err(|_| anyhow::anyhow!("runtime command lock poisoned"))?
            .clone()
            .context("runtime control is not available")?;
        sender
            .try_send(pending)
            .map_err(|_| anyhow::anyhow!("runtime is busy or stopping"))?;
        Ok(reply)
    }
    /// Reserved local stop remains available when the ordinary command queue is full.
    pub fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
        let _ = self.shared.wake.try_send(());
    }
    pub fn set_desktop_active(&self, active: bool) {
        self.shared.desktop_active.store(active, Ordering::Release);
        self.shared.desktop_epoch.fetch_add(1, Ordering::AcqRel);
        let _ = self.shared.wake.try_send(());
    }
}

pub struct Host {
    controller: Controller,
    worker: Option<JoinHandle<std::result::Result<(), String>>>,
}
impl Host {
    /// The frontend must verify its desktop/session before enabling capture.
    pub fn spawn(paths: Paths, options: Options) -> Result<Self> {
        let changed = unsafe { CreateEventW(None, true, false, None)? };
        let (wake, wakes) = bounded(1);
        let config = Config::default();
        let shared = Arc::new(Shared {
            view: Mutex::new(View {
                snapshot: Snapshot {
                    state: EngineState::Starting,
                    sound_pack: config.sound_pack,
                    volume: config.volume,
                },
                busy: true,
                last_error: None,
                finished: false,
                already_running: false,
                diagnostics: None,
            }),
            commands: Mutex::new(None),
            changed: unsafe { OwnedHandle::from_raw_handle(changed.0) },
            stop: AtomicBool::new(false),
            desktop_active: AtomicBool::new(false),
            desktop_epoch: AtomicU64::new(0),
            wake,
        });
        let flags = Arc::clone(&shared);
        let worker = thread::Builder::new()
            .name("klicky-engine".into())
            .spawn(move || {
                // Keep resource fields outside catch_unwind. A coordinator panic must
                // enter supervised cleanup, not drop its owner while workers are alive.
                let mut state = State::new(paths, options, flags, wakes);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if state.initialize()? {
                        state.dispatch()?;
                    }
                    Ok::<(), anyhow::Error>(())
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Windows coordinator panicked")));
                if let Err(error) = result {
                    state.failure = Some(format!("{error:#}"));
                }
                state.begin_stop();
                loop {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        state.stop_tick()
                    }));
                    match result {
                        Ok(Ok(true)) => break,
                        Ok(Ok(false)) => (),
                        Ok(Err(error)) => state.last_error = Some(format!("{error:#}")),
                        Err(_) => {
                            state.last_error =
                                Some("Windows cleanup panicked; ownership retained".into())
                        }
                    }
                    state.publish(false);
                    let _ = state.wakes.recv_timeout(Duration::from_millis(50));
                }
                *state
                    .shared
                    .commands
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = None;
                state.publish(true);
                match state.failure {
                    Some(error) => Err(error),
                    None => Ok(()),
                }
            })?;
        Ok(Self {
            controller: Controller { shared },
            worker: Some(worker),
        })
    }
    pub fn controller(&self) -> Controller {
        self.controller.clone()
    }
    pub fn has_exited(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
    /// True only after the native thread was waited and joined, including TLS exit.
    pub fn shutdown_complete(&self) -> bool {
        self.worker.is_none()
    }
    pub fn join(&mut self, deadline: Instant) -> Result<()> {
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
                WAIT_TIMEOUT if Instant::now() >= deadline => bail!("Windows host has not exited"),
                WAIT_TIMEOUT => (),
                WAIT_FAILED => return Err(::windows::core::Error::from_thread().into()),
                _ => bail!("unexpected Windows host wait result"),
            }
        }
        self.worker
            .take()
            .expect("observed Windows host")
            .join()
            .map_err(|_| anyhow::anyhow!("Windows host panicked"))?
            .map_err(anyhow::Error::msg)
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.controller.request_stop();
    }
}

struct Active {
    ticket: Ticket,
    deadline: Instant,
    pending: Option<PendingCommand>,
}

struct State {
    paths: Paths,
    options: Options,
    shared: Arc<Shared>,
    wakes: Receiver<()>,
    server: Option<Server>,
    commands: Receiver<PendingCommand>,
    pipe_failures: Receiver<super::control::ControlError>,
    input: Option<Input>,
    keys: Receiver<Press>,
    input_status: Receiver<input::Status>,
    input_ready: bool,
    audio: Option<Supervisor>,
    output: Option<Output>,
    packs: Option<Worker>,
    active: Option<Active>,
    config: Config,
    pack: Option<SoundPack>,
    paused: bool,
    recovering: bool,
    desktop_epoch: u64,
    started: Instant,
    stopping: Option<Instant>,
    receipts: Vec<Receiver<()>>,
    last_error: Option<String>,
    fault: Option<String>,
    startup_error: Option<String>,
    audio_error: Option<String>,
    failure: Option<String>,
    already_running: bool,
    dirty: bool,
    diagnostics: Diagnostics,
    retired: Vec<SoundPack>,
}

impl State {
    fn new(paths: Paths, options: Options, shared: Arc<Shared>, wakes: Receiver<()>) -> Self {
        Self {
            paths,
            paused: options.paused,
            options,
            shared,
            wakes,
            server: None,
            commands: never(),
            pipe_failures: never(),
            input: None,
            keys: never(),
            input_status: never(),
            input_ready: false,
            audio: None,
            output: None,
            packs: None,
            active: None,
            config: Config::default(),
            pack: None,
            recovering: false,
            desktop_epoch: 0,
            started: Instant::now(),
            stopping: None,
            receipts: Vec::new(),
            last_error: None,
            fault: None,
            startup_error: None,
            audio_error: None,
            failure: None,
            already_running: false,
            dirty: true,
            diagnostics: Diagnostics {
                dispatched: 0,
                stale: 0,
                obsolete_epoch: 0,
                input_overflow: 0,
                voice_drops: 0,
                dispatch_buckets_10us: vec![0; 1001],
                dispatch_max_us: 0,
                audible_latency_qualified: false,
            },
            retired: Vec::new(),
        }
    }

    fn initialize(&mut self) -> Result<bool> {
        if self.shared.stop.load(Ordering::Acquire) {
            return Ok(false);
        }
        let Some(owner) = InstanceOwner::try_acquire(&self.paths)? else {
            self.already_running = true;
            self.last_error = Some(
                "Klicky already owns this session; it may still be starting or stopping".into(),
            );
            return Ok(false);
        };
        let (server, commands, failures) = Server::start(PipePool::bind(owner)?)?;
        *self
            .shared
            .commands
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(server.commands());
        self.server = Some(server);
        self.commands = commands;
        self.pipe_failures = failures;
        let input = Input::spawn()?;
        self.keys = input.events();
        self.input_status = input.status();
        self.input = Some(input);
        let audio = Supervisor::spawn()?;
        audio.set_active(self.shared.desktop_active.load(Ordering::Acquire));
        self.audio = Some(audio);
        self.packs = Some(Worker::spawn(self.paths.clone())?);
        self.submit(Change::Bootstrap, None, Instant::now() + BOOT_LIMIT)?;
        self.publish(false);
        Ok(true)
    }

    fn dispatch(&mut self) -> Result<()> {
        loop {
            if self.shared.stop.load(Ordering::Acquire)
                || self
                    .options
                    .duration
                    .is_some_and(|limit| self.started.elapsed() >= limit)
            {
                self.begin_stop();
            }
            self.poll_components();
            if self.stopping.is_some() {
                return Ok(());
            }
            self.apply_desktop();
            for _ in 0..8 {
                let Ok(pending) = self.commands.try_recv() else {
                    break;
                };
                self.command(pending);
                if self.stopping.is_some() {
                    break;
                }
            }
            self.sync_capture();
            for _ in 0..32 {
                let Ok(press) = self.keys.try_recv() else {
                    break;
                };
                self.play(press);
            }
            self.publish(false);
            if self.stopping.is_some() {
                return Ok(());
            }
            let mut select = Select::new();
            select.recv(&self.wakes);
            select.recv(&self.commands);
            select.recv(&self.keys);
            select.recv(&self.input_status);
            select.recv(&self.pipe_failures);
            if let Some(audio) = &self.audio {
                select.recv(audio.updates());
            }
            if let Some(packs) = &self.packs {
                if !packs.has_exited() {
                    select.recv(packs.completed());
                    select.recv(packs.availability());
                }
            }
            let wait = self.options.duration.map_or(TICK, |limit| {
                limit.saturating_sub(self.started.elapsed()).min(TICK)
            });
            let _ = select.ready_timeout(wait);
            let _ = self.wakes.try_recv();
        }
    }

    fn poll_components(&mut self) {
        while let Ok(status) = self.input_status.try_recv() {
            self.dirty = true;
            match status {
                input::Status::Ready => self.input_ready = true,
                input::Status::Stopped(result) => {
                    self.input_ready = false;
                    if self.stopping.is_none() {
                        self.fault = Some(
                            result
                                .err()
                                .unwrap_or_else(|| "keyboard input stopped unexpectedly".into()),
                        );
                    }
                    self.input_status = never();
                    self.keys = never();
                }
            }
        }
        if let Some(update) = self.audio.as_ref().and_then(Supervisor::take_update) {
            self.dirty = true;
            match update {
                audio::Update::Ready(output) => {
                    self.clear_output();
                    self.output = Some(output);
                    self.recovering = false;
                    self.audio_error = None;
                }
                audio::Update::Waiting(error) => {
                    self.clear_output();
                    self.audio_error = Some(error);
                    self.recovering = false;
                }
                audio::Update::Recovering(error) => {
                    self.clear_output();
                    self.audio_error = Some(error);
                    self.recovering = true;
                }
                audio::Update::Inactive => self.clear_output(),
                audio::Update::Stopped(result) => {
                    self.clear_output();
                    if self.stopping.is_none() {
                        self.audio_error = Some(
                            result
                                .err()
                                .unwrap_or_else(|| "audio owner stopped unexpectedly".into()),
                        );
                    }
                }
            }
        }
        if self
            .output
            .as_ref()
            .is_some_and(|output| !output.is_available())
        {
            self.clear_output();
            self.recovering = true;
            self.dirty = true;
        }
        loop {
            match self.pipe_failures.try_recv() {
                Ok(error) => {
                    self.fault = Some(format!("control worker stopped: {error}"));
                    self.dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    if self.stopping.is_none() {
                        self.fault
                            .get_or_insert_with(|| "control workers exited unexpectedly".into());
                    }
                    self.pipe_failures = never();
                    self.dirty = true;
                    break;
                }
            }
        }
        if let Some(completion) = self.packs.as_mut().and_then(Worker::finish) {
            self.complete(completion);
        }
        if let Some(packs) = &self.packs {
            while packs.availability().try_recv().is_ok() {
                self.dirty = true;
            }
        }
        if self.packs.as_ref().is_some_and(Worker::has_exited) && self.stopping.is_none() {
            self.dirty = true;
            self.fault
                .get_or_insert_with(|| "configuration worker exited unexpectedly".into());
            if let Some(active) = self.active.take() {
                if let Some(pending) = active.pending {
                    pending.respond(failure(
                        ErrorCode::Unavailable,
                        "Configuration worker exited; persisted outcome may be unknown",
                    ));
                }
            }
        }
        if let Some(active) = self.active.as_mut() {
            if Instant::now() >= active.deadline {
                active.ticket.cancel();
                if let Some(pending) = active.pending.take() {
                    self.dirty = true;
                    pending.respond(failure(ErrorCode::Timeout, "Request expired; persisted outcome may be unknown until the worker completes"));
                }
            }
        }
        if !self.input_ready
            && self.started.elapsed() >= Duration::from_secs(5)
            && self.stopping.is_none()
        {
            self.dirty = true;
            self.fault.get_or_insert_with(|| {
                "keyboard input did not become ready; quit and restart Klicky".into()
            });
        }
    }

    fn complete(&mut self, completion: super::transactions::Completion) {
        self.dirty = true;
        if let Some(active) = self.active.take() {
            let activate = self.stopping.is_none()
                && !completion.cancelled_or_expired
                && !active.ticket.is_cancelled()
                && Instant::now() < active.deadline;
            match completion.result {
                Ok(mut prepared) if activate => {
                    match completion.change {
                        Change::Bootstrap | Change::Switch(_) => {
                            if let Some(output) = &self.output {
                                output.discard_voices();
                            }
                            if let Some(old) = self.pack.take() {
                                self.retire_pack(old);
                            }
                            self.config = prepared.config;
                            self.pack = prepared.pack;
                            self.startup_error = None;
                            if let Some(input) = &self.input {
                                if let Err(error) = input.discard_pending() {
                                    self.fault = Some(format!("{error:#}"));
                                }
                            }
                        }
                        Change::Volume(_) => {
                            self.config.volume = prepared.config.volume;
                            if let Some(pack) = prepared.pack.take() {
                                self.retire_pack(pack);
                            }
                        }
                    }
                    self.last_error = None;
                    if let Some(pending) = active.pending {
                        pending.respond(self.success());
                    }
                }
                Ok(mut prepared) => {
                    if let Some(pack) = prepared.pack.take() {
                        self.retire_pack(pack);
                    }
                    let message = "Change persisted for next start; cancellation, Stop or expiry prevented active playback";
                    self.last_error = Some(message.into());
                    if self.pack.is_none() && self.stopping.is_none() {
                        self.startup_error = Some(message.into());
                    }
                    if let Some(pending) = active.pending {
                        pending.respond(failure(ErrorCode::Timeout, message));
                    }
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    self.last_error = Some(message.clone());
                    if self.pack.is_none() && self.stopping.is_none() {
                        self.startup_error = Some(message.clone());
                    }
                    if let Some(pending) = active.pending {
                        let code = if !activate {
                            ErrorCode::Timeout
                        } else if completion.phase == Phase::Persisting {
                            ErrorCode::PersistenceFailure
                        } else {
                            ErrorCode::InvalidPack
                        };
                        pending.respond(failure(code, &message));
                    }
                }
            }
        }
    }

    fn apply_desktop(&mut self) {
        let epoch = self.shared.desktop_epoch.load(Ordering::Acquire);
        if epoch != self.desktop_epoch {
            self.dirty = true;
            self.desktop_epoch = epoch;
            if let Some(input) = &self.input {
                if let Err(error) = input.reset() {
                    self.fault = Some(format!("{error:#}"));
                }
            }
            if let Some(output) = &self.output {
                output.discard_voices();
            }
            if let Some(audio) = &self.audio {
                let active = self.shared.desktop_active.load(Ordering::Acquire);
                audio.set_active(active);
                if active {
                    audio.retry();
                }
            }
        }
    }

    fn engine_state(&self) -> EngineState {
        if self.stopping.is_some() {
            EngineState::Stopping
        } else if self.fault.is_some() || self.startup_error.is_some() {
            EngineState::Failed
        } else if self.pack.is_none() || !self.input_ready {
            EngineState::Starting
        } else if self.paused || !self.shared.desktop_active.load(Ordering::Acquire) {
            EngineState::Paused
        } else if self.output.as_ref().is_some_and(Output::is_available) {
            EngineState::Ready
        } else if self.recovering {
            EngineState::Recovering
        } else {
            EngineState::WaitingForAudio
        }
    }
    fn snapshot(&self) -> Snapshot {
        Snapshot {
            state: self.engine_state(),
            sound_pack: self.config.sound_pack.clone(),
            volume: self.config.volume,
        }
    }
    fn success(&self) -> Outcome {
        Outcome::Success {
            snapshot: self.snapshot(),
        }
    }

    fn command(&mut self, pending: PendingCommand) {
        self.dirty = true;
        if Instant::now() >= pending.deadline {
            pending.respond(failure(
                ErrorCode::Timeout,
                "Command expired before execution",
            ));
            return;
        }
        if self.fault.is_some()
            && matches!(
                &pending.request.command,
                Command::Switch { .. } | Command::Volume { .. }
            )
        {
            pending.respond(failure(
                ErrorCode::Unavailable,
                "A component failed; quit and restart Klicky",
            ));
            return;
        }
        let command = pending.request.command.clone();
        if matches!(command, Command::Stop {} | Command::PrepareUpdate {}) {
            let update = super::protocol::UpdateState {
                restart: !self.shared.stop.swap(true, Ordering::AcqRel) && self.stopping.is_none(),
                paused: self.paused,
            };
            self.begin_stop();
            self.receipts
                .retain(|receipt| matches!(receipt.try_recv(), Err(TryRecvError::Empty)));
            let outcome = if matches!(command, Command::PrepareUpdate {}) {
                Outcome::UpdatePrepared { state: update }
            } else {
                self.success()
            };
            if let Some(receipt) = pending.respond(outcome) {
                self.receipts.push(receipt);
            }
            return;
        }
        if matches!(command, Command::Status {}) {
            pending.respond(self.success());
            return;
        }
        if self.stopping.is_some() {
            pending.respond(failure(ErrorCode::Unavailable, "Klicky is stopping"));
            return;
        }
        match command {
            Command::Pause { paused } => {
                self.paused = paused;
                self.sync_capture();
                pending.respond(self.success());
            }
            Command::Switch { pack } => {
                let deadline = pending.deadline;
                if let Err(error) = self.submit(Change::Switch(pack), Some(pending), deadline) {
                    self.last_error = Some(format!("{error:#}"));
                }
            }
            Command::Volume { level } => {
                let deadline = pending.deadline;
                if let Err(error) = self.submit(Change::Volume(level), Some(pending), deadline) {
                    self.last_error = Some(format!("{error:#}"));
                }
            }
            Command::RetryAudio {} => {
                let result = self.retry();
                match result {
                    Ok(()) => {
                        pending.respond(self.success());
                    }
                    Err(error) => {
                        pending.respond(failure(ErrorCode::Unavailable, &format!("{error:#}")));
                    }
                }
            }
            Command::Stop {} | Command::PrepareUpdate {} | Command::Status {} => {
                unreachable!("handled above")
            }
        }
    }

    fn submit(
        &mut self,
        change: Change,
        pending: Option<PendingCommand>,
        deadline: Instant,
    ) -> Result<()> {
        if self.active.is_some() || self.packs.as_ref().is_none_or(Worker::is_busy) {
            if let Some(pending) = pending {
                pending.respond(failure(
                    ErrorCode::Busy,
                    "Another configuration change is in progress",
                ));
            }
            bail!("another configuration change is in progress");
        }
        match self
            .packs
            .as_mut()
            .expect("available pack worker")
            .submit(change, deadline)
        {
            Ok(ticket) => {
                self.dirty = true;
                self.active = Some(Active {
                    ticket,
                    deadline,
                    pending,
                });
                Ok(())
            }
            Err(error) => {
                if let Some(pending) = pending {
                    pending.respond(failure(ErrorCode::Unavailable, &format!("{error:#}")));
                }
                Err(error)
            }
        }
    }

    fn retry(&mut self) -> Result<()> {
        if !self.input_ready {
            bail!("keyboard input is unavailable; quit and restart Klicky");
        }
        self.last_error = None;
        if self.audio.as_ref().is_some_and(Supervisor::has_exited) {
            if let Some(mut audio) = self.audio.take() {
                if let Err(error) = audio.shutdown(Instant::now() + Duration::from_millis(10)) {
                    if !audio.shutdown_complete() {
                        self.audio = Some(audio);
                        return Err(error);
                    }
                    self.last_error = Some(format!(
                        "Previous audio owner exited with an error: {error:#}"
                    ));
                }
            }
            self.audio = Some(Supervisor::spawn()?);
        }
        let audio = self.audio.as_ref().context("audio owner unavailable")?;
        audio.set_active(self.shared.desktop_active.load(Ordering::Acquire));
        audio.retry();
        self.clear_output();
        self.recovering = true;
        if self.pack.is_none() && self.active.is_none() {
            self.startup_error = None;
            self.submit(Change::Bootstrap, None, Instant::now() + BOOT_LIMIT)?;
        }
        Ok(())
    }

    fn sync_capture(&mut self) {
        let enabled = self.engine_state() == EngineState::Ready;
        if let Some(input) = &self.input {
            if let Err(error) = input.set_enabled(enabled) {
                self.fault = Some(format!("{error:#}"));
                self.dirty = true;
            }
        }
        if let Some(output) = &self.output {
            output.set_enabled(enabled);
        }
    }

    fn play(&mut self, press: Press) {
        let Some(input) = &self.input else {
            return;
        };
        if press.epoch != input.epoch() {
            self.diagnostics.obsolete_epoch += 1;
            return;
        }
        if press.key.timestamp.elapsed() > input::MAX_KEY_AGE {
            self.diagnostics.stale += 1;
            return;
        }
        if self.engine_state() != EngineState::Ready {
            return;
        }
        let (Some(pack), Some(output)) = (&self.pack, &self.output) else {
            return;
        };
        if let Some(samples) = pack.samples.get(press.key.key_name) {
            let volume =
                crate::playback::key_playback_volume(self.config.volume, press.key.key_name);
            if output.play(samples, pack.channels, pack.sample_rate, volume) {
                self.diagnostics.dispatched += 1;
                if self.options.benchmark {
                    let micros = press
                        .observed_at
                        .elapsed()
                        .as_micros()
                        .min(u128::from(u64::MAX)) as u64;
                    self.diagnostics.dispatch_buckets_10us[(micros / 10).min(1000) as usize] += 1;
                    self.diagnostics.dispatch_max_us = self.diagnostics.dispatch_max_us.max(micros);
                }
            }
        }
    }

    fn clear_output(&mut self) {
        if let Some(output) = self.output.take() {
            output.set_enabled(false);
            self.diagnostics.voice_drops += output.voice_drops();
        }
    }

    fn retire_pack(&mut self, pack: SoundPack) {
        if self.stopping.is_some() && self.input.is_none() && self.audio.is_none() {
            // Dispatch and every renderer are confirmed closed. A completion
            // racing worker shutdown needs no retirement or false cleanup fault.
            drop(pack);
            return;
        }
        let result = match &self.packs {
            Some(worker) => worker.retire(pack),
            None => Err(pack),
        };
        if let Err(pack) = result {
            // One transaction is outstanding. A failed retirement closes further
            // mutations, bounding this quarantine until supervised shutdown.
            self.retired.push(pack);
            self.fault = Some("pack cleanup is unavailable; quit and restart Klicky".into());
        }
    }

    fn begin_stop(&mut self) {
        self.dirty = true;
        self.stopping
            .get_or_insert_with(|| Instant::now() + STOP_LIMIT);
        self.shared.stop.store(true, Ordering::Release);
        if let Some(active) = &self.active {
            active.ticket.cancel();
        }
        if let Some(input) = &self.input {
            let _ = input.request_stop();
        }
        if let Some(audio) = &self.audio {
            audio.request_stop();
        }
        self.clear_output();
        self.publish(false);
    }

    fn stop_tick(&mut self) -> Result<bool> {
        self.poll_components();
        for _ in 0..8 {
            let Ok(pending) = self.commands.try_recv() else {
                break;
            };
            self.command(pending);
        }
        self.receipts
            .retain(|receipt| matches!(receipt.try_recv(), Err(TryRecvError::Empty)));
        if self.input.as_ref().is_some_and(Input::has_exited) {
            if let Some(input) = &mut self.input {
                input.shutdown(Instant::now() + Duration::from_millis(10))?;
                self.diagnostics.input_overflow = input.counters().overflow.load(Ordering::Relaxed);
            }
            self.input.take();
            self.input_status = never();
            self.keys = never();
        }
        if self.audio.as_ref().is_some_and(Supervisor::has_exited) {
            if let Some(audio) = &mut self.audio {
                if let Err(error) = audio.shutdown(Instant::now() + Duration::from_millis(10)) {
                    if !audio.shutdown_complete() {
                        return Err(error);
                    }
                    self.failure.get_or_insert_with(|| format!("{error:#}"));
                }
            }
            self.audio.take();
        }
        if self.input.is_none() && self.audio.is_none() {
            if let Some(packs) = &mut self.packs {
                packs.request_stop();
            }
        }
        if self.packs.as_ref().is_some_and(Worker::has_exited) {
            if let Some(packs) = &mut self.packs {
                packs.shutdown(Instant::now() + Duration::from_millis(10))?;
            }
            self.packs.take();
            self.active.take();
            self.retired.clear();
        }
        let components_done = self.input.is_none() && self.audio.is_none() && self.packs.is_none();
        if components_done && self.receipts.is_empty() {
            if let Some(server) = &mut self.server {
                server.request_shutdown()?;
                if server.has_exited() {
                    server.shutdown(Instant::now() + Duration::from_millis(10))?;
                    self.server.take();
                    return Ok(true);
                }
            } else {
                return Ok(true);
            }
        }
        if self
            .stopping
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            self.last_error = Some(
                "Shutdown has not completed; ownership is retained while workers finish".into(),
            );
            self.dirty = true;
        }
        Ok(false)
    }

    fn publish(&mut self, finished: bool) {
        if !finished && !self.dirty {
            return;
        }
        self.dirty = false;
        let view = View {
            snapshot: self.snapshot(),
            busy: self.active.is_some() || self.packs.as_ref().is_some_and(Worker::is_busy),
            last_error: self
                .failure
                .clone()
                .or_else(|| self.fault.clone())
                .or_else(|| self.startup_error.clone())
                .or_else(|| self.last_error.clone())
                .or_else(|| self.audio_error.clone()),
            finished,
            already_running: self.already_running,
            diagnostics: (finished && self.options.benchmark).then(|| self.diagnostics.clone()),
        };
        let mut old = self
            .shared
            .view
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if *old != view {
            *old = view;
            unsafe {
                let _ = SetEvent(HANDLE(self.shared.changed.as_raw_handle()));
            }
        }
    }
}

fn failure(code: ErrorCode, message: &str) -> Outcome {
    let mut end = message.len().min(512);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    Outcome::Failure {
        code,
        message: message[..end].to_owned(),
    }
}
