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
        Commands::Stop => cmd_stop()?,
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
            match request.command {
                ipc::Command::Stop => {
                    let _ = request.acknowledge();
                    if benchmark && !latencies.is_empty() {
                        print_latency_summary(&latencies);
                    }
                    return Ok(());
                }
                ipc::Command::Switch { pack: name } => {
                    let result = config::resolve_sound_pack(&name, &sound_roots)
                        .context("sound pack not found")
                        .and_then(|path| soundpack::SoundPack::load(&path));
                    match result {
                        Ok(new_pack) => {
                            pack = new_pack;
                            cfg.sound_pack = name.clone();
                            let _ = cfg.save();
                            eprintln!("Switched to '{}'", name);
                        }
                        Err(e) => eprintln!("Failed to load '{}': {}", name, e),
                    }
                }
                ipc::Command::Volume { level } => {
                    player.set_volume(level);
                    cfg.volume = player.volume();
                    let _ = cfg.save();
                }
                ipc::Command::Status => {
                    let _ = request.acknowledge();
                }
            }
        }

        // Play sounds for key events (non-blocking, drain all pending)
        while let Ok(key_event) = key_rx.try_recv() {
            if let Some(samples) = pack.samples.get(key_event.key_name) {
                let is_fkey = key_event.key_name.starts_with('F')
                    && key_event.key_name.len() <= 3
                    && key_event.key_name[1..].parse::<u32>().is_ok();
                if is_fkey {
                    // Boost F-key volume since their samples are quieter
                    player.play_with_volume(
                        samples,
                        pack.channels,
                        pack.sample_rate,
                        (cfg.volume + 0.2).min(1.0),
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

    if ipc::send_command(&ipc::Command::Switch { pack: name.clone() }).is_ok() {
        println!("Switched to '{}'", name);
    } else {
        let mut cfg = config::Config::load()?;
        cfg.sound_pack = name.clone();
        cfg.save()?;
        println!("Switched to '{}' (will apply on next start)", name);
    }
    Ok(())
}

fn cmd_volume(level: f32) -> Result<()> {
    if !(0.0..=1.0).contains(&level) {
        bail!("Volume must be between 0.0 and 1.0");
    }

    if ipc::send_command(&ipc::Command::Volume { level }).is_ok() {
        println!("Volume set to {:.1}", level);
    } else {
        let mut cfg = config::Config::load()?;
        cfg.volume = level;
        cfg.save()?;
        println!("Volume set to {:.1} (will apply on next start)", level);
    }
    Ok(())
}

pub(crate) fn is_running() -> bool {
    lifecycle::read_pid(&config::runtime_dir())
        .ok()
        .flatten()
        .is_some_and(lifecycle::process_exists)
}

fn cmd_status() -> Result<()> {
    let cfg = config::Config::load()?;
    let running = is_running();

    println!("klicky status:");
    println!("  running:    {}", if running { "yes" } else { "no" });
    println!(
        "  responsive: {}",
        if lifecycle::responsive(&config::runtime_dir()) {
            "yes"
        } else {
            "no"
        }
    );
    println!("  sound pack: {}", cfg.sound_pack);
    println!("  volume:     {:.1}", cfg.volume);
    Ok(())
}
