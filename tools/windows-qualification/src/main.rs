//! Development-only qualification. This is not the Windows application.
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;

#[path = "../../../src/soundpack.rs"]
// The standalone baseline uses load(), not Windows transaction preparation.
#[allow(dead_code)]
mod soundpack;

// Compile the production PCM source and mixer unchanged. Alternative buffer
// requests are confined to this tool; the application policy is not modified.
#[cfg(target_os = "windows")]
mod player {
    include!("../../../src/player.rs");

    impl Player {
        pub fn qualification(policy: super::BufferPolicy) -> anyhow::Result<Self> {
            match policy {
                super::BufferPolicy::Baseline => Self::new(),
                super::BufferPolicy::DeviceDefault => {
                    let device = rodio::cpal::default_host()
                        .default_output_device()
                        .context("no default audio output device")?;
                    let sink = DeviceSinkBuilder::from_device(device)?
                        .with_buffer_size(BufferSize::Default)
                        .open_stream()?;
                    Ok(Self { sink, volume: 0.8 })
                }
            }
        }

        pub fn configured_request(&self) -> String {
            format!("{:?}", self.sink.config())
        }
    }
}

#[cfg(target_os = "windows")]
mod windows_input;

#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum BufferPolicy {
    /// Use the exact production 256/512/1024/fallback policy.
    Baseline,
    /// Explicit CPAL default; avoids Rodio from_device's fixed-size default.
    DeviceDefault,
}

#[derive(Parser)]
#[command(about = "Opt-in qualification only; dispatch timing is not audible latency")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build identity only; does not open audio or input.
    Info,
    /// Bounded passive Space-key playback. Does not record microphone audio.
    Capture {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=300))]
        seconds: u64,
        #[arg(long, default_value_t = 0.8, value_parser = volume)]
        volume: f32,
        #[arg(long, value_enum, default_value_t = BufferPolicy::Baseline)]
        buffer: BufferPolicy,
        /// New local JSON file; never uploaded. Refuses to overwrite.
        #[arg(long)]
        output: PathBuf,
    },
    /// Export the prepared Space sound for identifying its acoustic waveform.
    Reference {
        #[arg(long)]
        pack: PathBuf,
        #[arg(long, default_value_t = 0.8, value_parser = volume)]
        volume: f32,
        #[arg(long)]
        output: PathBuf,
    },
}

fn volume(text: &str) -> std::result::Result<f32, String> {
    let value: f32 = text.parse().map_err(|_| "expected a number".to_string())?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err("volume must be finite and between 0 and 1".into());
    }
    Ok(value)
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Info => {
            println!(
                "{}",
                serde_json::json!({
                    "source_revision": option_env!("KLICKY_QUALIFICATION_REVISION").unwrap_or("unknown"),
                    "source_tree_dirty": option_env!("KLICKY_QUALIFICATION_TREE_STATE") != Some("clean"),
                    "tool_version": env!("CARGO_PKG_VERSION"),
                    "architecture": std::env::consts::ARCH,
                })
            );
            Ok(())
        }
        Command::Capture {
            pack,
            seconds,
            volume,
            buffer,
            output,
        } => capture(&pack, Duration::from_secs(seconds), volume, buffer, &output),
        Command::Reference {
            pack,
            volume,
            output,
        } => reference(&pack, volume, &output),
    }
}

fn new_output(path: &Path) -> Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("cannot create new output {}", path.display()))
}

fn reference(pack: &Path, volume: f32, output: &Path) -> Result<()> {
    let pack = soundpack::SoundPack::load(pack)?;
    let samples = pack
        .samples
        .get("Space")
        .context("pack has no Space sound")?;
    let bytes = u32::try_from(
        samples
            .len()
            .checked_mul(2)
            .context("sample size overflow")?,
    )?;
    let mut writer = BufWriter::new(new_output(output)?);
    writer.write_all(b"RIFF")?;
    writer.write_all(
        &bytes
            .checked_add(36)
            .context("WAV too large")?
            .to_le_bytes(),
    )?;
    writer.write_all(b"WAVEfmt ")?;
    writer.write_all(&16u32.to_le_bytes())?;
    writer.write_all(&1u16.to_le_bytes())?;
    writer.write_all(&pack.channels.to_le_bytes())?;
    writer.write_all(&pack.sample_rate.to_le_bytes())?;
    let block = pack
        .channels
        .checked_mul(2)
        .context("channel size overflow")?;
    writer.write_all(
        &pack
            .sample_rate
            .checked_mul(u32::from(block))
            .context("rate overflow")?
            .to_le_bytes(),
    )?;
    writer.write_all(&block.to_le_bytes())?;
    writer.write_all(&16u16.to_le_bytes())?;
    writer.write_all(b"data")?;
    writer.write_all(&bytes.to_le_bytes())?;
    for &sample in samples.iter() {
        writer.write_all(&((f32::from(sample) * volume) as i16).to_le_bytes())?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct Dispatch {
    ordinal: usize,
    callback_since_ready_us: u128,
    callback_to_mixer_us: u128,
}

#[cfg(target_os = "windows")]
#[derive(Serialize)]
struct Report {
    schema_version: u32,
    purpose: &'static str,
    architecture: &'static str,
    source_revision: &'static str,
    source_tree_dirty: bool,
    buffer_policy: BufferPolicy,
    configured_request_not_actual_buffer: String,
    actual_buffer_frames: Option<u32>,
    queued_padding_frames: Option<u32>,
    volume: f32,
    startup_to_ready_us: u128,
    input: windows_input::Stats,
    events: Vec<Dispatch>,
    physical_to_audible_measured: bool,
    qualification_passed: bool,
}

#[cfg(target_os = "windows")]
fn capture(
    path: &Path,
    duration: Duration,
    volume: f32,
    policy: BufferPolicy,
    output: &Path,
) -> Result<()> {
    let mut writer = BufWriter::new(new_output(output)?);
    let startup = Instant::now();
    let pack = soundpack::SoundPack::load(path)?;
    let samples = pack
        .samples
        .get("Space")
        .context("pack has no Space sound")?;
    let mut player = player::Player::qualification(policy)?;
    player.set_volume(volume);
    let mut input = windows_input::Input::start()?;
    let ready = Instant::now();
    eprintln!("READY: Space plays a sound; Escape ends this bounded session.");
    eprintln!(
        "Qualifier: no microphone access or text/key-name logging; an external recorder is separate. Limit: {} seconds.",
        duration.as_secs()
    );
    let mut events = Vec::with_capacity(4096);
    let deadline = ready + duration;
    let outcome = (|| -> Result<()> {
        loop {
            if Instant::now() >= deadline {
                return Ok(());
            }
            match input.receive(deadline.saturating_duration_since(Instant::now()))? {
                Some(timestamp) => {
                    player.play(samples, pack.channels, pack.sample_rate);
                    if events.len() >= 100_000 {
                        bail!("event limit reached; session cannot qualify");
                    }
                    events.push(Dispatch {
                        ordinal: events.len() + 1,
                        callback_since_ready_us: timestamp
                            .saturating_duration_since(ready)
                            .as_micros(),
                        callback_to_mixer_us: timestamp.elapsed().as_micros(),
                    });
                }
                None => return Ok(()),
            }
        }
    })();
    let stats = input.stop()?;
    outcome?;
    let report = Report {
        schema_version: 1,
        purpose: "internal dispatch only; use separately acquired physical/audio evidence",
        architecture: std::env::consts::ARCH,
        source_revision: option_env!("KLICKY_QUALIFICATION_REVISION").unwrap_or("unknown"),
        source_tree_dirty: option_env!("KLICKY_QUALIFICATION_TREE_STATE") != Some("clean"),
        buffer_policy: policy,
        configured_request_not_actual_buffer: player.configured_request(),
        actual_buffer_frames: None,
        queued_padding_frames: None,
        volume,
        startup_to_ready_us: ready.duration_since(startup).as_micros(),
        input: stats,
        events,
        physical_to_audible_measured: false,
        qualification_passed: false,
    };
    serde_json::to_writer_pretty(&mut writer, &report)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    eprintln!(
        "Session ended: {} dispatch samples. No audible-latency qualification claimed.",
        report.events.len()
    );
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn capture(_: &Path, _: Duration, _: f32, _: BufferPolicy, _: &Path) -> Result<()> {
    bail!("capture targets Windows 11 x64; use the existing Klicky application for other-platform baselines")
}
