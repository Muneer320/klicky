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
    /// Query or control this app's audio output
    Audio {
        #[command(subcommand)]
        action: crate::audio::AudioCommand,
    },
    /// Check for a stable release or open the supported update route
    Update {
        /// Only check; do not open an installer or package manager
        #[arg(long)]
        check: bool,
        /// Accept the displayed update handoff without prompting
        #[arg(long)]
        yes: bool,
    },
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
    Status {
        /// Emit structured playback and service state for desktop helpers
        #[arg(long)]
        json: bool,
    },
    /// Silence playback without stopping the daemon
    Pause,
    /// Resume playback without replaying paused events
    Resume,
    /// Manage automatic startup and the background service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum ServiceAction {
    /// Change login startup without changing current playback
    Autostart {
        #[command(subcommand)]
        action: AutostartAction,
    },
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

#[derive(Subcommand)]
pub(crate) enum AutostartAction {
    Enable,
    Disable,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Audio { action } => cmd_audio(action)?,
        Commands::Update { check, yes } => crate::update::run(check, yes)?,
        Commands::Start { benchmark } => engine::run(benchmark)?,
        Commands::Stop => service::stop()?,
        Commands::List => cmd_list()?,
        Commands::Switch { name } => cmd_switch(name)?,
        Commands::Volume { level } => cmd_volume(level)?,
        Commands::Status { json: false } => cmd_status()?,
        Commands::Status { json: true } => cmd_json_status()?,
        Commands::Pause => {
            ipc::send_command(&ipc::Command::Pause)?;
        }
        Commands::Resume => {
            ipc::send_command(&ipc::Command::Resume)?;
        }
        Commands::Service { action } => service::run(action)?,
    }

    Ok(())
}

fn cmd_audio(action: crate::audio::AudioCommand) -> Result<()> {
    action.validate()?;
    match ipc::audio_command(action.clone()) {
        Ok(state) => println!("{}", serde_json::to_string_pretty(&state)?),
        #[cfg(target_os = "linux")]
        Err(error) if ipc::is_daemon_unavailable(&error)
            && matches!(action, crate::audio::AudioCommand::Outputs | crate::audio::AudioCommand::Select { .. }) => {
            if action == crate::audio::AudioCommand::Outputs {
                println!("{}", serde_json::to_string_pretty(&crate::linux_audio::outputs()?)?);
            } else {
                let directory = config::ensure_runtime_dir()?;
                let _owner = lifecycle::Owner::acquire(&directory)?
                    .context("Klicky may be starting; no offline change was saved")?;
                let preference = crate::linux_audio::validate_selection(&action)?
                    .context("no output preference was supplied")?;
                let mut cfg = config::Config::load()?;
                cfg.audio_output = preference;
                cfg.save()?;
                println!("Audio output saved for next start");
            }
        }
        Err(error)
            if ipc::is_daemon_unavailable(&error)
                && action == crate::audio::AudioCommand::FollowDefault =>
        {
            let directory = config::ensure_runtime_dir()?;
            let _owner = lifecycle::Owner::acquire(&directory)?
                .context("Klicky may be starting; no offline change was saved")?;
            let mut cfg = config::Config::load()?;
            cfg.audio_output = Default::default();
            cfg.save()?;
            println!("System-default output saved for next start");
        }
        Err(error) => {
            return Err(error)
                .context("Audio request not confirmed; no offline preference change was saved. Older engines need an upgrade for audio controls")
        }
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
        Ok(_) => println!("{}", volume_confirmation(level, false)),
        Err(error) if ipc::is_daemon_unavailable(&error) => {
            let mut cfg = config::Config::load()?;
            cfg.volume = level;
            cfg.save()?;
            println!("{}", volume_confirmation(level, true));
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
    println!("{}", status_volume_line(cfg.volume));
    Ok(())
}

/// Text for Linux/macOS volume confirmations and `status`.
///
/// `Display` keeps a quiet nonzero value visible. The endpoints stay `0.0` and
/// `1.0`, which `Display` would otherwise print as `0` and `1`.
fn format_cli_volume(level: f32) -> String {
    if level == 0.0 {
        "0.0".to_string()
    } else if level == 1.0 {
        "1.0".to_string()
    } else {
        format!("{level}")
    }
}

fn volume_confirmation(level: f32, deferred: bool) -> String {
    let shown = format_cli_volume(level);
    if deferred {
        format!("Volume set to {shown} (will apply on next start)")
    } else {
        format!("Volume set to {shown}")
    }
}

fn status_volume_line(level: f32) -> String {
    format!("  volume:     {}", format_cli_volume(level))
}

fn cmd_json_status() -> Result<()> {
    let cfg = config::Config::load()?;
    let health = lifecycle::status(&config::runtime_dir())?;
    // Never substitute saved preferences for a live daemon's confirmed state.
    let playback = if health.responsive {
        ipc::playback_status()?
    } else {
        ipc::PlaybackState {
            sound_pack: cfg.sound_pack,
            volume: cfg.volume,
            paused: false,
        }
    };
    println!(
        "{}",
        serde_json::to_string(&serde_json::json!({
            "schema_version": 1,
            "running": health.responsive,
            "process_exists": health.process_exists,
            "paused": playback.paused,
            "sound_pack": playback.sound_pack,
            "volume": playback.volume,
            "packs": config::available_sound_packs(&config::sound_roots())?,
            "autostart": service::autostart_enabled()?,
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_cli_volume, status_volume_line, volume_confirmation};

    #[test]
    fn cli_volume_text_keeps_quiet_settings_and_readable_endpoints() {
        assert_eq!(format_cli_volume(0.0), "0.0");
        assert_eq!(format_cli_volume(1.0), "1.0");
        assert_eq!(format_cli_volume(0.04), "0.04");
        assert_eq!(format_cli_volume(0.125), "0.125");
        assert_eq!(format_cli_volume(0.8), "0.8");
        assert_eq!(format_cli_volume(0.5), "0.5");
        assert_eq!(format_cli_volume(0.0004), "0.0004");
        assert_eq!(format_cli_volume(0.999), "0.999");
        assert_eq!(volume_confirmation(0.04, false), "Volume set to 0.04");
        assert_eq!(
            volume_confirmation(0.125, true),
            "Volume set to 0.125 (will apply on next start)"
        );
        assert_eq!(status_volume_line(0.04), "  volume:     0.04");
        assert_eq!(status_volume_line(0.0), "  volume:     0.0");
        assert_eq!(status_volume_line(1.0), "  volume:     1.0");
    }
}
