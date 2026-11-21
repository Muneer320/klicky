//! Per-application desktop routing; no global device/profile changes.
mod pulse;

use std::num::{NonZeroU16, NonZeroU32};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use rodio::mixer::Mixer;
use rodio::Source;

use crate::audio::{
    AudioCommand, AudioStatus, Availability, Backend, OutputDevice, OutputList, OutputPreference,
};
use crate::config::Config;
use crate::player::PcmSource;

const HEALTH: Duration = Duration::from_millis(250);
const RETRY: Duration = Duration::from_secs(1);

pub(crate) fn validate_preference(preference: &OutputPreference) -> Result<()> {
    preference.validate()?;
    if matches!(preference, OutputPreference::Device { backend, .. } if *backend != Backend::PulseAudio)
    {
        bail!("desktop Linux selection requires pulse-audio sink IDs, including on PipeWire; ALSA-only playback supports system default");
    }
    Ok(())
}

pub(crate) fn outputs() -> Result<OutputList> {
    pulse::Connection::connect()?.outputs()
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
    validate_preference(&preference)?;
    if !preference.is_default() {
        pulse::Connection::connect()?.resolve(&preference)?;
    }
    Ok(Some(preference))
}

#[derive(Clone)]
struct State {
    epoch: u64,
    preference: OutputPreference,
    active: Option<OutputDevice>,
    availability: Availability,
    mixer: Option<Mixer>,
    selectable: bool,
}
struct Shared {
    stop: AtomicBool,
    changed: AtomicBool,
    route_epoch: Arc<AtomicU64>,
    state: Mutex<State>,
}
impl Shared {
    fn publish(&self, state: &State) {
        if let Ok(mut latest) = self.state.lock() {
            *latest = state.clone();
            latest.epoch = self.route_epoch.load(Ordering::Acquire);
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
    voices: Arc<AtomicU64>,
    jobs: SyncSender<Job>,
    done: Receiver<()>,
    state: State,
    volume: f32,
}
impl Player {
    pub(crate) fn with_config(config: &Config) -> Result<Self> {
        validate_preference(&config.audio_output)?;
        let state = State {
            epoch: 0,
            preference: config.audio_output.clone(),
            active: None,
            availability: Availability::Initializing,
            mixer: None,
            selectable: true,
        };
        let shared = Arc::new(Shared {
            stop: AtomicBool::new(false),
            changed: AtomicBool::new(false),
            route_epoch: Arc::new(AtomicU64::new(0)),
            state: Mutex::new(state.clone()),
        });
        let (jobs, receive) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::sync_channel(1);
        let owner = Arc::clone(&shared);
        let initial = state.clone();
        thread::Builder::new()
            .name("klicky-linux-audio".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run(&owner, receive, initial)
                }));
                if result.is_err() {
                    owner.route_epoch.fetch_add(1, Ordering::AcqRel);
                    if let Ok(mut state) = owner.state.lock() {
                        state.active = None;
                        state.mixer = None;
                        state.availability = Availability::Unavailable;
                        owner.changed.store(true, Ordering::Release);
                    }
                    eprintln!("[klicky] Linux audio worker stopped unexpectedly");
                }
                let _ = finished.try_send(());
            })?;
        Ok(Self {
            shared,
            voices: Arc::new(AtomicU64::new(0)),
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
    pub(crate) fn status(&self, paused: bool) -> AudioStatus {
        AudioStatus {
            schema_version: 1,
            requested: Some(self.state.preference.clone()),
            active: self.state.active.clone(),
            availability: self.state.availability,
            manual_paused: paused,
            can_select: self.state.selectable,
            can_enumerate: self.state.selectable,
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
            .context("Linux audio worker is busy or unavailable")?;
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
        source.cancel_with(
            Arc::clone(&self.voices),
            self.voices.load(Ordering::Acquire),
        );
        mixer.add(
            RouteSource {
                source,
                route_epoch: Arc::clone(&self.shared.route_epoch),
                expected: self.state.epoch,
                until_check: 0,
            }
            .amplify(volume),
        );
    }
    pub fn clear(&mut self) {
        self.voices.fetch_add(1, Ordering::AcqRel);
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
            eprintln!("[klicky] Linux audio shutdown did not finish before the deadline");
        }
    }
}

enum Output {
    Desktop(pulse::Output),
    Alsa(crate::player::Player),
}
struct Playing {
    output: Output,
    heartbeat: Option<Arc<AtomicU64>>,
    last_tick: u64,
    pulse_at: Instant,
    announced: bool,
}
impl Playing {
    fn mixer_format(&self) -> (Mixer, NonZeroU16, NonZeroU32) {
        match &self.output {
            Output::Desktop(output) => (output.mixer.clone(), output.channels, output.rate),
            Output::Alsa(player) => player.output(),
        }
    }
    fn device(&self) -> Option<OutputDevice> {
        match &self.output {
            Output::Desktop(output) => Some(output.device.clone()),
            Output::Alsa(_) => None,
        }
    }
}

#[derive(Default)]
struct Host {
    desktop: Option<pulse::Connection>,
    connected_once: bool,
    stale: bool,
    last_probe: Option<Instant>,
}
impl Host {
    fn desktop(&mut self) -> Result<&mut pulse::Connection> {
        if self.desktop.is_none() {
            self.desktop = Some(pulse::Connection::connect()?);
            self.connected_once = true;
            self.stale = false;
            self.last_probe = Some(Instant::now());
        }
        let connection = self.desktop.as_mut().unwrap();
        if self.stale || !connection.alive() {
            bail!("desktop audio server is reconnecting");
        }
        Ok(connection)
    }
    fn open(&mut self, preference: &OutputPreference) -> Result<Playing> {
        let output = match self.desktop() {
            Ok(connection) => {
                let device = connection.resolve(preference)?;
                Output::Desktop(connection.open(device)?)
            }
            Err(error) => {
                let local_absent = error
                    .chain()
                    .filter_map(|cause| cause.downcast_ref::<pulse::Error>())
                    .any(pulse::Error::is_absent);
                if self.connected_once
                    || !preference.is_default()
                    || std::env::var_os("PULSE_SERVER").is_some()
                    || !local_absent
                {
                    return Err(error);
                }
                Output::Alsa(crate::player::Player::new()?)
            }
        };
        let heartbeat = if matches!(output, Output::Alsa(_)) {
            Some(Arc::new(AtomicU64::new(0)))
        } else {
            None
        };
        let playing = Playing {
            output,
            heartbeat: heartbeat.clone(),
            last_tick: 0,
            pulse_at: Instant::now(),
            announced: false,
        };
        let (mixer, channels, rate) = playing.mixer_format();
        if let Some(heartbeat) = heartbeat {
            mixer.add(Heartbeat {
                ticks: heartbeat,
                channels,
                rate,
                until_tick: 0,
            });
        }
        Ok(playing)
    }
    fn prepare(&mut self, job: &Job) -> Result<Reply> {
        match &job.command {
            AudioCommand::Outputs => Ok(Reply::Outputs(self.desktop()?.outputs()?)),
            AudioCommand::Retry => Ok(Reply::Retrying),
            command => {
                let preference = match command {
                    AudioCommand::Select { backend, id } => OutputPreference::Device {
                        backend: *backend,
                        id: id.clone(),
                    },
                    AudioCommand::FollowDefault => OutputPreference::SystemDefault,
                    _ => bail!("unexpected Linux audio request"),
                };
                validate_preference(&preference)?;
                if !preference.is_default() {
                    self.desktop()?.resolve(&preference)?;
                }
                let mut next = job.config.clone();
                next.audio_output = preference;
                next.save()?;
                Ok(Reply::Changed(next))
            }
        }
    }
    fn healthy(&mut self, current: &mut Playing, preference: &OutputPreference) -> bool {
        if let Some(heartbeat) = &current.heartbeat {
            let tick = heartbeat.load(Ordering::Relaxed);
            if tick != current.last_tick {
                current.last_tick = tick;
                current.pulse_at = Instant::now();
            }
            if current.pulse_at.elapsed() >= RETRY {
                eprintln!("[klicky] ALSA audio renderer heartbeat stopped");
                return false;
            }
        }
        match &mut current.output {
            Output::Alsa(_) => true,
            Output::Desktop(output) => {
                let Some(connection) = &self.desktop else {
                    return false;
                };
                if !connection.alive() || !output.healthy() {
                    return false;
                }
                if self.last_probe.is_none_or(|last| last.elapsed() >= RETRY) {
                    if !connection.responsive() {
                        self.stale = true;
                        eprintln!("[klicky] desktop audio server did not answer its health query");
                        return false;
                    }
                    self.last_probe = Some(Instant::now());
                }
                if connection.dirty.swap(false, Ordering::AcqRel) {
                    return connection
                        .resolve(preference)
                        .is_ok_and(|device| device.output.id == output.device.id);
                }
                true
            }
        }
    }
}

fn invalidate(shared: &Shared, state: &mut State) {
    shared.route_epoch.fetch_add(1, Ordering::AcqRel);
    state.active = None;
    state.mixer = None;
    state.availability = Availability::Recovering;
    shared.publish(state);
}
fn run(shared: &Shared, jobs: Receiver<Job>, mut state: State) {
    let mut host = Host::default();
    let mut playing: Option<Playing> = None;
    let mut retry_at = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        let wait = if playing.as_ref().is_some_and(|stream| !stream.announced) {
            Duration::from_millis(10)
        } else if playing.is_some() {
            HEALTH
        } else {
            retry_at
                .saturating_duration_since(Instant::now())
                .min(HEALTH)
        };
        match jobs.recv_timeout(wait) {
            Ok(job) => {
                let result = host.prepare(&job);
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
                    playing.take();
                    retry_at = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => (),
        }
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if playing
            .as_mut()
            .is_some_and(|stream| !host.healthy(stream, &state.preference))
        {
            invalidate(shared, &mut state);
            playing.take();
            retry_at = Instant::now() + RETRY;
        }
        if host.stale
            || host
                .desktop
                .as_ref()
                .is_some_and(|connection| !connection.alive())
        {
            // Drop the stream before disconnecting its context/mainloop.
            if playing.is_some() {
                invalidate(shared, &mut state);
                playing.take();
            }
            host.desktop.take();
        }
        if let Some(stream) = playing.as_mut() {
            if !stream.announced && (stream.heartbeat.is_none() || stream.last_tick != 0) {
                state.active = stream.device();
                state.mixer = Some(stream.mixer_format().0);
                state.selectable = matches!(stream.output, Output::Desktop(_));
                state.availability = Availability::Available;
                shared.publish(&state);
                stream.announced = true;
            }
        } else if Instant::now() >= retry_at {
            match host.open(&state.preference) {
                Ok(stream) => playing = Some(stream),
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
    drop(playing);
    drop(host);
}

struct RouteSource {
    source: PcmSource,
    route_epoch: Arc<AtomicU64>,
    expected: u64,
    until_check: u8,
}
impl Iterator for RouteSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.until_check == 0 {
            if self.route_epoch.load(Ordering::Acquire) != self.expected {
                return None;
            }
            self.until_check = 127;
        } else {
            self.until_check -= 1;
        }
        self.source.next()
    }
}
impl Source for RouteSource {
    fn channels(&self) -> NonZeroU16 {
        self.source.channels()
    }
    fn sample_rate(&self) -> NonZeroU32 {
        self.source.sample_rate()
    }
    fn current_span_len(&self) -> Option<usize> {
        self.source.current_span_len()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.source.total_duration()
    }
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
