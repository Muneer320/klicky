//! Core Audio ownership stays on one worker; input dispatch only uses a mixer.
mod devices;

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use cpal_native::traits::DeviceTrait;
use cpal_native::{BufferSize, Device, SampleFormat};
use rodio::mixer::Mixer;
use rodio::Source;

use crate::audio::{
    AudioCommand, AudioStatus, Availability, OutputDevice, OutputList, OutputPreference,
};
use crate::config::Config;
use crate::native_sink::Sink;
use crate::player::PcmSource;

pub(crate) use devices::outputs;

const HEALTH: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(1);

#[derive(Clone)]
struct State {
    epoch: u64,
    preference: OutputPreference,
    active: Option<OutputDevice>,
    availability: Availability,
    mixer: Option<Mixer>,
}

struct Shared {
    stop: AtomicBool,
    changed: AtomicBool,
    generation: Arc<AtomicU64>,
    state: Mutex<State>,
}
impl Shared {
    fn publish(&self, state: &State) {
        if let Ok(mut latest) = self.state.lock() {
            *latest = state.clone();
            latest.epoch = self.generation.load(Ordering::Acquire);
            self.changed.store(true, Ordering::Release);
        }
    }
}

pub(crate) enum Reply {
    Outputs(OutputList),
    Changed(Config),
    Retrying,
}
struct Job {
    command: AudioCommand,
    config: Config,
    reply: SyncSender<std::result::Result<Reply, String>>,
}

pub(crate) struct Player {
    shared: Arc<Shared>,
    jobs: SyncSender<Job>,
    done: Receiver<()>,
    state: State,
    volume: f32,
}
impl Player {
    pub(crate) fn with_config(config: &Config) -> Result<Self> {
        devices::validate(&config.audio_output)?;
        let state = State {
            epoch: 0,
            preference: config.audio_output.clone(),
            active: None,
            availability: Availability::Initializing,
            mixer: None,
        };
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            changed: AtomicBool::new(false),
            generation: Arc::new(AtomicU64::new(0)),
            state: Mutex::new(state.clone()),
        });
        let (jobs, receive) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::sync_channel(1);
        let owner = Arc::clone(&shared);
        let initial = state.clone();
        thread::Builder::new()
            .name("klicky-core-audio".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&owner, receive, initial)
                }));
                if result.is_err() {
                    owner.generation.fetch_add(1, Ordering::AcqRel);
                    if let Ok(mut state) = owner.state.lock() {
                        state.active = None;
                        state.mixer = None;
                        state.availability = Availability::Unavailable;
                        owner.changed.store(true, Ordering::Release);
                    }
                    eprintln!("[klicky] Core Audio worker stopped unexpectedly");
                }
                let _ = finished.try_send(());
            })?;
        Ok(Self {
            shared,
            jobs,
            done,
            state,
            volume: config.volume,
        })
    }

    pub(crate) fn poll(&mut self) -> bool {
        if !self.shared.changed.swap(false, Ordering::AcqRel) {
            return false;
        }
        if let Ok(state) = self.shared.state.lock() {
            self.state = state.clone();
        }
        true
    }

    pub(crate) fn status(&mut self, paused: bool) -> AudioStatus {
        self.poll();
        AudioStatus {
            schema_version: 1,
            requested: Some(self.state.preference.clone()),
            active: self.state.active.clone(),
            availability: self.state.availability,
            manual_paused: paused,
            can_select: true,
            can_enumerate: true,
            can_retry: true,
        }
    }

    pub(crate) fn submit(
        &self,
        command: AudioCommand,
        config: &Config,
    ) -> Result<Receiver<std::result::Result<Reply, String>>> {
        command.validate()?;
        let (reply, response) = mpsc::sync_channel(1);
        self.jobs
            .try_send(Job {
                command,
                config: config.clone(),
                reply,
            })
            .context("Core Audio worker is busy or unavailable")?;
        Ok(response)
    }

    pub fn play(&mut self, samples: &Arc<Vec<i16>>, channels: u16, rate: u32) {
        self.play_with_volume(samples, channels, rate, self.volume);
    }
    pub fn play_with_volume(
        &mut self,
        samples: &Arc<Vec<i16>>,
        channels: u16,
        rate: u32,
        volume: f32,
    ) {
        let Some(mixer) = &self.state.mixer else {
            return;
        };
        let Some(mut source) = PcmSource::new(samples, channels, rate) else {
            return;
        };
        source.cancel_with(Arc::clone(&self.shared.generation), self.state.epoch);
        mixer.add(source.amplify(volume));
    }
    pub fn clear(&mut self) {
        self.state.epoch = self.shared.generation.fetch_add(1, Ordering::AcqRel) + 1;
    }
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.clear();
        if self.done.recv_timeout(Duration::from_secs(3)).is_err() {
            eprintln!("[klicky] Core Audio shutdown did not finish before the deadline");
        }
    }
}

pub(crate) fn validate_selection(command: &AudioCommand) -> Result<Option<OutputPreference>> {
    let preference = match command {
        AudioCommand::Select { backend, id } => OutputPreference::Device {
            backend: *backend,
            id: id.clone(),
        },
        AudioCommand::FollowDefault => OutputPreference::SystemDefault,
        _ => return Ok(None),
    };
    devices::validate(&preference)?;
    if !preference.is_default() {
        devices::resolve(&preference)?;
    }
    Ok(Some(preference))
}

struct Stream {
    sink: Sink,
    device: Device,
    output: OutputDevice,
    format: (u16, u32, SampleFormat),
    failed: Arc<AtomicBool>,
    heartbeat: Arc<AtomicU64>,
    last_tick: u64,
    pulse_at: Instant,
    announced: bool,
}
fn open(preference: &OutputPreference) -> Result<Stream> {
    let (device, output) = devices::resolve(preference)?;
    let config = device.default_output_config()?;
    let format = (
        config.channels(),
        config.sample_rate(),
        config.sample_format(),
    );
    let failed = Arc::new(AtomicBool::new(false));
    let mut error = None;
    for buffer in crate::player::BUFFER_SIZES
        .into_iter()
        .map(BufferSize::Fixed)
        .chain(Some(BufferSize::Default))
    {
        let errors = Arc::clone(&failed);
        match Sink::open(&device, buffer, move |_| {
            errors.store(true, Ordering::Release);
        }) {
            Ok(sink) => {
                eprintln!("[klicky] audio output: {}", sink.description());
                let heartbeat = Arc::new(AtomicU64::new(0));
                sink.mixer().add(Heartbeat {
                    ticks: Arc::clone(&heartbeat),
                    channels: sink.channel_count(),
                    rate: sink.sample_rate(),
                    until_tick: 0,
                });
                return Ok(Stream {
                    sink,
                    device,
                    output,
                    format,
                    failed,
                    heartbeat,
                    last_tick: 0,
                    pulse_at: Instant::now(),
                    announced: false,
                });
            }
            Err(cause) => error = Some(cause),
        }
    }
    Err(error.context("no Core Audio stream configuration tried")?)
}

fn invalidate(shared: &Shared, state: &mut State) {
    shared.generation.fetch_add(1, Ordering::AcqRel);
    state.active = None;
    state.mixer = None;
    state.availability = Availability::Recovering;
    shared.publish(state);
}

fn prepare(job: &Job) -> Result<Reply> {
    match &job.command {
        AudioCommand::Outputs => Ok(Reply::Outputs(outputs()?)),
        AudioCommand::Retry => Ok(Reply::Retrying),
        command => {
            let preference = validate_selection(command)?.context("unexpected audio request")?;
            let mut next = job.config.clone();
            next.audio_output = preference;
            next.save()?;
            Ok(Reply::Changed(next))
        }
    }
}

fn run(shared: &Shared, jobs: Receiver<Job>, mut state: State) {
    let mut stream: Option<Stream> = None;
    let mut retry_at = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        match jobs.recv_timeout(if stream.is_some() {
            HEALTH
        } else {
            retry_at
                .saturating_duration_since(Instant::now())
                .min(HEALTH)
        }) {
            Ok(job) => {
                let result = prepare(&job);
                let reopen = match &result {
                    Ok(Reply::Changed(config)) => {
                        let changed = state.preference != config.audio_output;
                        state.preference = config.audio_output.clone();
                        changed
                    }
                    Ok(Reply::Retrying) => true,
                    _ => false,
                };
                if reopen {
                    invalidate(shared, &mut state);
                }
                let _ = job
                    .reply
                    .try_send(result.map_err(|error| format!("{error:#}")));
                if reopen {
                    stream.take();
                    retry_at = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let unhealthy = stream.as_mut().is_some_and(|current| {
            let tick = current.heartbeat.load(Ordering::Relaxed);
            if tick != current.last_tick {
                current.last_tick = tick;
                current.pulse_at = Instant::now();
            }
            let config = current.device.default_output_config();
            let route_changed = state.preference.is_default()
                && devices::default_id().ok().as_ref() != Some(&current.output.id);
            let format_changed = config
                .map(|config| {
                    (
                        config.channels(),
                        config.sample_rate(),
                        config.sample_format(),
                    ) != current.format
                })
                .unwrap_or(true);
            if current.failed.load(Ordering::Acquire)
                || current.pulse_at.elapsed() >= RETRY
                || route_changed
                || format_changed
            {
                return true;
            }
            if !current.announced && tick != 0 {
                state.active = Some(current.output.clone());
                state.mixer = Some(current.sink.mixer().clone());
                state.availability = Availability::Available;
                shared.publish(&state);
                current.announced = true;
            }
            false
        });
        if unhealthy {
            invalidate(shared, &mut state);
            stream.take();
            retry_at = Instant::now();
        }
        if stream.is_none() && Instant::now() >= retry_at {
            match open(&state.preference) {
                Ok(opened) => stream = Some(opened),
                Err(error) => {
                    if state.availability != Availability::Unavailable {
                        eprintln!("[klicky] audio unavailable: {error:#}");
                    }
                    state.availability = Availability::Unavailable;
                    shared.publish(&state);
                    retry_at = Instant::now() + RETRY;
                }
            }
        }
    }
    invalidate(shared, &mut state);
}

struct Heartbeat {
    ticks: Arc<AtomicU64>,
    channels: NonZeroU16,
    rate: NonZeroU32,
    until_tick: u8,
}
impl Iterator for Heartbeat {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.until_tick == 0 {
            self.ticks.fetch_add(1, Ordering::Relaxed);
            self.until_tick = 127;
        } else {
            self.until_tick -= 1;
        }
        Some(0.0)
    }
}
impl Source for Heartbeat {
    fn channels(&self) -> NonZeroU16 {
        self.channels
    }
    fn sample_rate(&self) -> NonZeroU32 {
        self.rate
    }
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
