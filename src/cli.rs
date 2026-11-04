use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use crate::{config, engine, ipc, lifecycle, service};

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
pub(crate) enum ServiceAction {
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

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start { benchmark } => engine::run(benchmark)?,
        Commands::Stop => service::stop()?,
        Commands::List => cmd_list()?,
        Commands::Switch { name } => cmd_switch(name)?,
        Commands::Volume { level } => cmd_volume(level)?,
        Commands::Status => cmd_status()?,
        Commands::Service { action } => service::run(action)?,
    }

    Ok(())
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
