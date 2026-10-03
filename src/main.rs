mod config;
mod ipc;
mod listener;
mod media_keys;
mod player;
mod soundpack;

use std::fs;
use std::process;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "klicky", about = "Mechanical keyboard sounds for your Mac")]
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
    }

    Ok(())
}

fn cmd_start(benchmark: bool) -> Result<()> {
    let pid_path = config::pid_path();
    if pid_path.exists() {
        let pid_str = fs::read_to_string(&pid_path).unwrap_or_default();
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            unsafe {
                if libc::kill(pid, 0) == 0 {
                    println!("klicky is already running (pid {})", pid);
                    return Ok(());
                }
            }
        }
        fs::remove_file(&pid_path)?;
    }

    let mut cfg = config::Config::load()?;
    let sounds_dir = config::sounds_dir();

    if !sounds_dir.exists() {
        bail!(
            "No sound packs found. Copy sound packs to {}",
            sounds_dir.display()
        );
    }

    let pack_dir = sounds_dir.join(&cfg.sound_pack);
    if !pack_dir.exists() {
        eprintln!(
            "Sound pack '{}' not found, trying first available...",
            cfg.sound_pack
        );
        let first = fs::read_dir(&sounds_dir)?
            .filter_map(|e| e.ok())
            .find(|e| e.path().is_dir())
            .context("No sound packs found")?;
        cfg.sound_pack = first.file_name().to_string_lossy().to_string();
        cfg.save()?;
    }

    // Write PID
    fs::create_dir_all(pid_path.parent().unwrap())?;
    fs::write(&pid_path, process::id().to_string())?;

    println!(
        "klicky started with '{}' (volume: {})",
        cfg.sound_pack, cfg.volume
    );

    run_daemon(cfg, benchmark)?;

    Ok(())
}

fn run_daemon(mut cfg: config::Config, benchmark: bool) -> Result<()> {
    let sounds_dir = config::sounds_dir();

    // Load sound pack
    let mut pack = soundpack::SoundPack::load(&sounds_dir.join(&cfg.sound_pack))?;
    let mut player = player::Player::new()?;
    player.set_volume(cfg.volume);

    // IPC channel
    let (ipc_tx, ipc_rx) = mpsc::channel();
    ipc::start_server(ipc_tx)?;

    // Key listener channel
    let (key_tx, key_rx) = mpsc::channel();
    let media_key_tx = key_tx.clone();
    thread::spawn(move || {
        listener::start_listening(key_tx);
    });

    // Media/function key listener (F1-F12 without Fn on MacBooks)
    media_keys::start_media_key_listener(media_key_tx);

    // Latency tracking
    let mut latencies: Vec<f64> = Vec::new();

    // Main event loop
    loop {
        // Check for IPC commands (non-blocking)
        while let Ok(cmd) = ipc_rx.try_recv() {
            match cmd {
                ipc::Command::Stop => {
                    if benchmark && !latencies.is_empty() {
                        print_latency_summary(&latencies);
                    }
                    cleanup();
                    process::exit(0);
                }
                ipc::Command::Switch { pack: name } => {
                    let pack_path = sounds_dir.join(&name);
                    match soundpack::SoundPack::load(&pack_path) {
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
                ipc::Command::Status => {}
            }
        }

        // Play sounds for key events (non-blocking, drain all pending)
        while let Ok(key_event) = key_rx.try_recv() {
            if let Some(samples) = pack.samples.get(&key_event.key_name) {
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

fn cmd_stop() -> Result<()> {
    match ipc::send_command(&ipc::Command::Stop) {
        Ok(_) => println!("klicky stopped"),
        Err(_) => println!("klicky is not running"),
    }
    cleanup();
    Ok(())
}

fn cmd_list() -> Result<()> {
    let sounds_dir = config::sounds_dir();
    if !sounds_dir.exists() {
        println!("No sound packs found at {}", sounds_dir.display());
        return Ok(());
    }

    let cfg = config::Config::load()?;
    for entry in fs::read_dir(&sounds_dir)? {
        let entry = entry?;
        if entry.path().is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            let marker = if name == cfg.sound_pack {
                " (active)"
            } else {
                ""
            };
            println!("  {}{}", name, marker);
        }
    }
    Ok(())
}

fn cmd_switch(name: String) -> Result<()> {
    let sounds_dir = config::sounds_dir();
    if !sounds_dir.join(&name).exists() {
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

fn cmd_status() -> Result<()> {
    let cfg = config::Config::load()?;
    let pid_path = config::pid_path();

    let running = if pid_path.exists() {
        let pid_str = fs::read_to_string(&pid_path).unwrap_or_default();
        pid_str
            .trim()
            .parse::<i32>()
            .map(|pid| unsafe { libc::kill(pid, 0) == 0 })
            .unwrap_or(false)
    } else {
        false
    };

    println!("klicky status:");
    println!("  running:    {}", if running { "yes" } else { "no" });
    println!("  sound pack: {}", cfg.sound_pack);
    println!("  volume:     {:.1}", cfg.volume);
    Ok(())
}

fn cleanup() {
    let _ = fs::remove_file(config::pid_path());
    let _ = fs::remove_file(config::socket_path());
}
