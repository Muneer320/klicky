mod config;
mod ipc;
mod lifecycle;
mod listener;
#[cfg(target_os = "macos")]
mod media_keys;
mod player;
mod service;
mod soundpack;

use std::sync::mpsc;
use std::thread;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "klicky",
    version,
    about = "Low-latency mechanical keyboard sounds"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the klicky daemon
    Start {
        /// Print latency measurements for each keypress
        #[arg(long)]
        benchmark: bool,
    },
    /// Stop the klicky daemon
    Stop,
    /// List available sound packs
    List,
    /// Switch to a different sound pack
    Switch { name: String },
    /// Set volume (0.0 - 1.0)
    Volume { level: f32 },
    /// Show current status
    Status,
    /// Manage automatic startup and the background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Start at login and start now
    Enable,
    /// Stop now and do not start at login
    Disable,
    /// Start the installed service for this session
    Start,
    /// Stop the service for this session
    Stop,
    /// Show whether automatic startup is enabled and the daemon is running
    Status,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { benchmark } => cmd_start(benchmark)?,
        Commands::Stop => service::stop()?,
        Commands::List => cmd_list()?,
        Commands::Switch { name } => cmd_switch(name)?,
        Commands::Volume { level } => cmd_volume(level)?,
        Commands::Status => cmd_status()?,
        Commands::Service { action } => service::run(action)?,
    }

    Ok(())
}

fn cmd_start(benchmark: bool) -> Result<()> {
    let directory = config::ensure_runtime_dir()?;
    let _owner = match lifecycle::Owner::acquire(&directory)? {
        Some(owner) => owner,
        None => {
            if lifecycle::responsive(&directory) {
                println!("klicky is already running");
                return Ok(());
            }
            bail!("klicky is starting, stopping, or unresponsive; retry after it has stopped");
        }
    };

    let mut cfg = config::Config::load()?;
    let sound_roots = config::sound_roots();
    if config::resolve_sound_pack(&cfg.sound_pack, &sound_roots).is_none() {
        eprintln!(
            "Sound pack '{}' not found, trying first available...",
            cfg.sound_pack
        );
        let first = config::available_sound_packs(&sound_roots)?
            .into_iter()
            .next()
            .context("No sound packs found")?;
        cfg.sound_pack = first;
        cfg.save()?;
    }

    run_daemon(cfg, benchmark)?;

    Ok(())
}

fn run_daemon(mut cfg: config::Config, benchmark: bool) -> Result<()> {
    let sound_roots = config::sound_roots();

    // Load sound pack
    let pack_path = config::resolve_sound_pack(&cfg.sound_pack, &sound_roots)
        .with_context(|| format!("Sound pack '{}' not found", cfg.sound_pack))?;
    let mut pack = soundpack::SoundPack::load(&pack_path)?;
    let mut player = player::Player::new()?;
    player.set_volume(cfg.volume);

    // IPC channel
    let (ipc_tx, ipc_rx) = mpsc::channel();
    ipc::start_server(ipc_tx)?;

    // Key listener channel
    let (key_tx, key_rx) = mpsc::channel();
    let (listener_error_tx, listener_error_rx) = mpsc::channel();
    #[cfg(target_os = "macos")]
    let media_key_tx = key_tx.clone();
    thread::spawn(move || {
        if let Err(error) = listener::start_listening(key_tx) {
            let _ = listener_error_tx.send(error);
        }
    });

    #[cfg(target_os = "macos")]
    media_keys::start_media_key_listener(media_key_tx);

    // Latency tracking
    let mut latencies: Vec<f64> = Vec::new();

    println!(
        "klicky started with '{}' (volume: {})",
        cfg.sound_pack, cfg.volume
    );

    // Main event loop
    loop {
        if let Ok(error) = listener_error_rx.try_recv() {
            return Err(error.context("key listener stopped"));
        }
        // Check for IPC commands (non-blocking)
        while let Ok(mut request) = ipc_rx.try_recv() {
            match request.command.clone() {
                ipc::Command::Stop => {
                    let _ = request.acknowledge();
                    if benchmark && !latencies.is_empty() {
                        print_latency_summary(&latencies);
                    }
                    return Ok(());
                }
                ipc::Command::Switch { pack: name } => {
                    let result = switch_pack(
                        &mut cfg,
                        &mut pack,
                        &name,
                        &sound_roots,
                        config::Config::save,
                    );
                    let response = match &result {
                        Ok(()) => request.acknowledge(),
                        Err(error) => request.reject(format!("{error:#}")),
                    };
                    if let Err(error) = result {
                        eprintln!("Failed to switch to '{name}': {error:#}");
                    } else {
                        eprintln!("Switched to '{name}'");
                    }
                    if let Err(error) = response {
                        eprintln!("Failed to respond to switch request: {error:#}");
                    }
                }
                ipc::Command::Volume { level } => {
                    let result = set_config_volume(&mut cfg, level, config::Config::save);
                    let response = match &result {
                        Ok(volume) => {
                            player.set_volume(*volume);
                            request.acknowledge()
                        }
                        Err(error) => request.reject(format!("{error:#}")),
                    };
                    if let Err(error) = result {
                        eprintln!("Failed to set volume: {error:#}");
                    }
                    if let Err(error) = response {
                        eprintln!("Failed to respond to volume request: {error:#}");
                    }
                }
                ipc::Command::Status => {
                    let _ = request.acknowledge();
                }
            }
        }

        // Play sounds for key events (non-blocking, drain all pending)
        while let Ok(key_event) = key_rx.try_recv() {
            if let Some(samples) = pack.samples.get(key_event.key_name) {
                if is_function_key(key_event.key_name) {
                    player.play_with_volume(
                        samples,
                        pack.channels,
                        pack.sample_rate,
                        key_playback_volume(cfg.volume, key_event.key_name),
                    );
                } else {
                    player.play(samples, pack.channels, pack.sample_rate);
                }

                if benchmark {
                    let latency_us = key_event.timestamp.elapsed().as_micros() as f64;
                    let latency_ms = latency_us / 1000.0;
                    latencies.push(latency_ms);
                    eprintln!(
                        "[{}] {:.2}ms  (avg: {:.2}ms, min: {:.2}ms, max: {:.2}ms, n={})",
                        key_event.key_name,
                        latency_ms,
                        latencies.iter().sum::<f64>() / latencies.len() as f64,
                        latencies.iter().cloned().fold(f64::INFINITY, f64::min),
                        latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
                        latencies.len(),
                    );
                }
            }
        }

        // Small sleep to avoid busy-spinning
        thread::sleep(std::time::Duration::from_micros(500));
    }
}

fn print_latency_summary(latencies: &[f64]) {
    let n = latencies.len();
    let avg = latencies.iter().sum::<f64>() / n as f64;
    let min = latencies.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = latencies.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut sorted = latencies.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = sorted[n / 2];
    let p95 = sorted[(n as f64 * 0.95) as usize];
    let p99 = sorted[(n as f64 * 0.99) as usize];

    eprintln!("\n=== Latency Summary ({} keypresses) ===", n);
    eprintln!("  avg:  {:.2}ms", avg);
    eprintln!("  min:  {:.2}ms", min);
    eprintln!("  max:  {:.2}ms", max);
    eprintln!("  p50:  {:.2}ms", p50);
    eprintln!("  p95:  {:.2}ms", p95);
    eprintln!("  p99:  {:.2}ms", p99);
}

pub(crate) fn cmd_stop() -> Result<()> {
    match lifecycle::stop(&config::runtime_dir())? {
        true => println!("klicky stopped"),
        false => println!("klicky is not running"),
    }
    Ok(())
}

fn cmd_list() -> Result<()> {
    let sound_roots = config::sound_roots();
    let packs = config::available_sound_packs(&sound_roots)?;
    if packs.is_empty() {
        println!("No sound packs found");
        return Ok(());
    }

    let cfg = config::Config::load()?;
    for name in packs {
        let marker = if name == cfg.sound_pack {
            " (active)"
        } else {
            ""
        };
        println!("  {}{}", name, marker);
    }
    Ok(())
}

fn cmd_switch(name: String) -> Result<()> {
    if config::resolve_sound_pack(&name, &config::sound_roots()).is_none() {
        bail!("Sound pack '{}' not found", name);
    }

    match ipc::send_command(&ipc::Command::Switch { pack: name.clone() }) {
        Ok(_) => println!("Switched to '{name}'"),
        Err(error) if ipc::is_daemon_unavailable(&error) => {
            let mut cfg = config::Config::load()?;
            cfg.sound_pack = name.clone();
            cfg.save()?;
            println!("Switched to '{name}' (will apply on next start)");
        }
        Err(error) => {
            return Err(error).context(
                "Klicky did not confirm the sound-pack change; no offline change was saved",
            )
        }
    }
    Ok(())
}

fn cmd_volume(level: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&level) {
        bail!("Volume must be between 0.0 and 1.0");
    }

    match ipc::send_command(&ipc::Command::Volume { level }) {
        Ok(_) => println!("Volume set to {level:.1}"),
        Err(error) if ipc::is_daemon_unavailable(&error) => {
            let mut cfg = config::Config::load()?;
            cfg.volume = level;
            cfg.save()?;
            println!("Volume set to {level:.1} (will apply on next start)");
        }
        Err(error) => {
            return Err(error)
                .context("Klicky did not confirm the volume change; no offline change was saved")
        }
    }
    Ok(())
}

fn switch_pack(
    config: &mut config::Config,
    pack: &mut soundpack::SoundPack,
    name: &str,
    roots: &[std::path::PathBuf],
    persist: impl FnOnce(&config::Config) -> Result<()>,
) -> Result<()> {
    let path = config::resolve_sound_pack(name, roots)
        .with_context(|| format!("sound pack '{name}' not found"))?;
    let new_pack = soundpack::SoundPack::load(&path)?;
    let next = config::Config {
        sound_pack: name.to_string(),
        volume: config.volume,
    };
    persist(&next)?;
    *config = next;
    *pack = new_pack;
    Ok(())
}

fn set_config_volume(
    config: &mut config::Config,
    level: f32,
    save: impl FnOnce(&config::Config) -> Result<()>,
) -> Result<f32> {
    if !level.is_finite() || !(0.0..=1.0).contains(&level) {
        bail!("Volume must be between 0.0 and 1.0");
    }
    let next = config::Config {
        sound_pack: config.sound_pack.clone(),
        volume: level,
    };
    save(&next)?;
    *config = next;
    Ok(level)
}

fn key_playback_volume(volume: f32, key_name: &str) -> f32 {
    if !is_function_key(key_name) {
        return volume;
    }
    if volume <= 0.0 {
        0.0
    } else {
        (volume + 0.2).min(1.0)
    }
}

fn is_function_key(key_name: &str) -> bool {
    key_name.starts_with('F') && key_name.len() <= 3 && key_name[1..].parse::<u32>().is_ok()
}

fn cmd_status() -> Result<()> {
    let cfg = config::Config::load()?;
    let status = lifecycle::status(&config::runtime_dir())?;

    println!("klicky status:");
    println!(
        "  running:    {}",
        if status.responsive { "yes" } else { "no" }
    );
    println!(
        "  process exists: {}",
        if status.process_exists { "yes" } else { "no" }
    );
    println!(
        "  responsive: {}",
        if status.responsive { "yes" } else { "no" }
    );
    println!("  sound pack: {}", cfg.sound_pack);
    println!("  volume:     {:.1}", cfg.volume);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn key_playback_volume_keeps_zero_silent_for_all_key_categories() {
        for key_name in ["A", "NEXTSONG", "F1", "F12"] {
            assert_eq!(key_playback_volume(0.0, key_name), 0.0, "{key_name}");
        }
    }

    #[test]
    fn function_key_boost_respects_volume_boundaries() {
        assert_eq!(key_playback_volume(0.25, "F1"), 0.45);
        assert_eq!(key_playback_volume(0.8, "F12"), 1.0);
        assert_eq!(key_playback_volume(1.0, "F1"), 1.0);
        assert_eq!(key_playback_volume(0.25, "A"), 0.25);
        assert_eq!(key_playback_volume(0.25, "NEXTSONG"), 0.25);
    }

    #[test]
    fn failed_switch_persistence_does_not_change_runtime_or_config() -> Result<()> {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let roots = [manifest.join("sounds")];
        let mut config = config::Config {
            sound_pack: "eg-oreo".to_string(),
            volume: 0.5,
        };
        let mut pack = soundpack::SoundPack::load(&roots[0].join("eg-oreo"))?;
        let old_channels = pack.channels;
        let old_rate = pack.sample_rate;
        let old_keys = pack
            .samples
            .keys()
            .cloned()
            .collect::<std::collections::HashSet<_>>();

        let result = switch_pack(&mut config, &mut pack, "cherrymx-blue-abs", &roots, |_| {
            anyhow::bail!("simulated read-only config")
        });

        assert!(result.is_err());
        assert_eq!(config.sound_pack, "eg-oreo");
        assert_eq!(pack.channels, old_channels);
        assert_eq!(pack.sample_rate, old_rate);
        assert_eq!(
            pack.samples
                .keys()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
            old_keys
        );
        Ok(())
    }

    #[test]
    fn successful_switch_persists_before_changing_runtime_state() -> Result<()> {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let roots = [manifest.join("sounds")];
        let mut config = config::Config {
            sound_pack: "eg-oreo".to_string(),
            volume: 0.5,
        };
        let mut pack = soundpack::SoundPack::load(&roots[0].join("eg-oreo"))?;
        let mut persisted_name = None;

        switch_pack(
            &mut config,
            &mut pack,
            "cherrymx-blue-abs",
            &roots,
            |next| {
                persisted_name = Some(next.sound_pack.clone());
                Ok(())
            },
        )?;

        assert_eq!(persisted_name.as_deref(), Some("cherrymx-blue-abs"));
        assert_eq!(config.sound_pack, "cherrymx-blue-abs");
        Ok(())
    }

    #[test]
    fn failed_volume_persistence_does_not_change_config() {
        let mut config = config::Config {
            sound_pack: "eg-oreo".to_string(),
            volume: 0.5,
        };
        let result = set_config_volume(&mut config, 0.8, |_| {
            anyhow::bail!("simulated read-only config")
        });
        assert!(result.is_err());
        assert_eq!(config.volume, 0.5);
    }

    #[test]
    fn volume_update_validates_finite_range_and_persists_boundaries() -> Result<()> {
        for invalid in [-0.1, 1.1, f32::NAN, f32::INFINITY] {
            let mut config = config::Config::default();
            assert!(set_config_volume(&mut config, invalid, |_| Ok(())).is_err());
            assert_eq!(config.volume, 0.8);
        }
        for valid in [0.0, 0.5, 1.0] {
            let mut config = config::Config::default();
            let mut persisted = None;
            assert_eq!(
                set_config_volume(&mut config, valid, |next| {
                    persisted = Some(next.volume);
                    Ok(())
                })?,
                valid
            );
            assert_eq!(config.volume, valid);
            assert_eq!(persisted, Some(valid));
        }
        Ok(())
    }
}
