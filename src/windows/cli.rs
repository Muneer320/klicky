use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

use super::control::{Client, ControlError, COMMAND_TIMEOUT};
use super::protocol::{Command, Outcome, Snapshot};
use super::runtime::Options;
use super::{ConfigStore, InstanceOwner, Paths};

#[derive(Parser)]
#[command(
    name = "klicky",
    version,
    about = "Mechanical keyboard sounds for your desktop"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Run the Windows tray app in this process; Ctrl+C stops it
    Start {
        /// Collect aggregate internal dispatch diagnostics (not audible latency)
        #[arg(long)]
        benchmark: bool,
        /// Start without key sounds
        #[arg(long)]
        paused: bool,
        /// End this test session automatically (1-3600 seconds)
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=3600))]
        seconds: Option<u64>,
    },
    /// Stop and confirm the verified instance has actually exited
    Stop,
    /// List installed and user sound packs
    List,
    /// Activate a pack, or save it for next start when no instance exists
    Switch { name: String },
    /// Set volume between 0.0 and 1.0
    Volume { level: f32 },
    /// Show responsive live state, without creating configuration
    Status,
    /// Read aggregate input/playback counters from the running Windows app
    Diagnostics,
    /// Pause key sounds for this session
    Pause,
    /// Resume key sounds when the desktop and audio are available
    Resume,
    /// Reopen the default multimedia audio output
    RetryAudio,
    /// Manage per-user login startup and the background desktop app
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Register login startup and start now; Windows may independently disable startup
    Enable,
    /// Disable this bundle's login startup and stop its running instance
    Disable,
    /// Start this bundle's desktop app for the current session
    Start {
        /// Apply only when launching a new desktop process
        #[arg(long)]
        paused: bool,
    },
    /// Stop this bundle's verified running instance
    Stop,
    /// Show login startup registration and live session state
    Status,
    /// Installer hook; other bundles are preserved and every matching exit is verified
    #[command(hide = true)]
    PrepareUpdate,
    /// Installer hook; caller holds the exclusive bundle marker
    #[command(hide = true)]
    UnregisterStartup,
    /// Installer hook; register without launching after the bundle is unlocked
    #[command(hide = true)]
    RegisterStartup,
}

pub fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Start {
            benchmark,
            paused,
            seconds,
        } => super::desktop::run(Options {
            benchmark,
            paused,
            duration: seconds.map(Duration::from_secs),
        }),
        Commands::Stop => match Client::current()?.stop(deadline()) {
            Ok(()) => {
                println!("Klicky stopped; process exit confirmed");
                Ok(())
            }
            Err(ControlError::Absent) => {
                let paths = Paths::discover()?;
                let _owner = InstanceOwner::try_acquire(&paths)?
                    .context("Klicky owns the session but its control pipe is unavailable")?;
                println!("Klicky is not running");
                Ok(())
            }
            Err(error) => Err(error.into()),
        },
        Commands::List => {
            for name in Paths::discover()?.available_sound_packs()? {
                println!("{name}");
            }
            Ok(())
        }
        Commands::Status => match Client::current()?.command(Command::Status {}, deadline()) {
            Ok(reply) => print_outcome(reply.response.outcome),
            Err(ControlError::Absent) => {
                let paths = Paths::discover()?;
                let _owner = InstanceOwner::try_acquire(&paths)?
                    .context("Klicky owns the session but is not responsive")?;
                println!("Klicky is not running");
                Ok(())
            }
            Err(error) => Err(error.into()),
        },
        Commands::Diagnostics => {
            let reply = Client::current()?
                .command(Command::Diagnostics {}, deadline())
                .context("Diagnostics require a running app that supports this command; older previews do not")?;
            match reply.response.outcome {
                Outcome::Diagnostics { snapshot, counters } => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "process_id": reply.response.instance.pid,
                            "process_creation_time": reply.response.instance.creation_time,
                            "snapshot": snapshot,
                            "counters": counters,
                        }))?
                    );
                    Ok(())
                }
                outcome => print_outcome(outcome),
            }
        }
        Commands::Switch { name } => change(Command::Switch { pack: name }),
        Commands::Volume { level } => change(Command::Volume { level }),
        Commands::Pause => live(Command::Pause { paused: true }),
        Commands::Resume => live(Command::Pause { paused: false }),
        Commands::RetryAudio => live(Command::RetryAudio {}),
        Commands::Service { action } => service(action),
    }
}

fn service(action: ServiceAction) -> Result<()> {
    use super::service as background;
    let paths = Paths::discover()?;
    match action {
        ServiceAction::Enable => {
            background::startup(paths.clone(), true)?;
            println!(
                "Login startup is registered; Windows Startup settings controls whether it runs"
            );
            let snapshot = background::start(&paths)
                .context("Startup is enabled, but desktop launch was not confirmed")?;
            print_snapshot(&snapshot);
        }
        ServiceAction::Disable => {
            background::startup(paths.clone(), false)?;
            println!("Login startup disabled (or already disabled)");
            if background::stop_for_update(&paths)? {
                println!("Klicky stopped; process exit confirmed");
            } else {
                println!("No matching running bundle was stopped");
            }
        }
        ServiceAction::Start { paused } => {
            print_snapshot(&background::start_with_pause(&paths, paused)?)
        }
        ServiceAction::Stop => {
            if background::stop_for_update(&paths)? {
                println!("Matching Klicky process exit confirmed");
            } else {
                println!("No matching running bundle; other instances were preserved");
            }
        }
        ServiceAction::PrepareUpdate => {
            let state = background::prepare_update(&paths)?;
            println!(
                "KLICKY_UPDATE_V1 {} {}",
                u8::from(state.restart),
                u8::from(state.paused)
            );
        }
        ServiceAction::Status => {
            println!(
                "Login startup: {:?}; Windows Startup settings controls approval",
                background::startup_status(paths)?
            );
            match Client::current()?.command(Command::Status {}, deadline()) {
                Ok(reply) => print_outcome(reply.response.outcome)?,
                Err(ControlError::Absent) => println!("No responsive session endpoint"),
                Err(error) => return Err(error.into()),
            }
        }
        ServiceAction::UnregisterStartup => {
            let status = background::unregister_for_removal(paths)?;
            println!(
                "KLICKY_STARTUP_V1 {}",
                if status == super::startup::Status::Foreign {
                    "FOREIGN"
                } else {
                    "ABSENT"
                }
            );
        }
        ServiceAction::RegisterStartup => {
            background::startup(paths, true)?;
            println!("KLICKY_STARTUP_V1 REGISTERED");
        }
    }
    Ok(())
}
fn deadline() -> Instant {
    Instant::now() + COMMAND_TIMEOUT
}
fn live(command: Command) -> Result<()> {
    print_outcome(
        Client::current()?
            .command(command, deadline())?
            .response
            .outcome,
    )
}
fn change(command: Command) -> Result<()> {
    // Validate before contacting a running process or changing offline configuration.
    super::protocol::Request::new(1, command.clone()).encode()?;
    match Client::current()?.command(command.clone(), deadline()) {
        Ok(reply) => print_outcome(reply.response.outcome),
        Err(ControlError::Absent) => {
            let paths = Paths::discover()?;
            let _owner = InstanceOwner::try_acquire(&paths)?
                .context("Klicky may be starting; no offline change was saved")?;
            let store = ConfigStore::new(paths.clone());
            let saved = match command {
                Command::Switch { pack } => {
                    paths
                        .resolve_sound_pack(&pack)?
                        .context("Sound pack not found")?;
                    store.update(|config| {
                        config.sound_pack = pack;
                        Ok(())
                    })?
                }
                Command::Volume { level } => store.update(|config| {
                    config.volume = level;
                    Ok(())
                })?,
                _ => bail!("Command requires a running instance"),
            };
            println!(
                "Saved for next start: {} | volume {}",
                saved.sound_pack, saved.volume
            );
            Ok(())
        }
        Err(error) => {
            Err(error).context("Klicky did not confirm the change; no offline change was saved")
        }
    }
}
fn print_outcome(outcome: Outcome) -> Result<()> {
    match outcome {
        Outcome::Success { snapshot } => {
            print_snapshot(&snapshot);
            Ok(())
        }
        Outcome::Failure { code, message } => bail!("{code:?}: {message}"),
        Outcome::UpdatePrepared { .. } => bail!("Unexpected installer preparation response"),
        Outcome::Diagnostics { .. } => bail!("Unexpected diagnostics response"),
    }
}
fn print_snapshot(snapshot: &Snapshot) {
    println!(
        "State: {:?}\nSound pack: {}\nVolume: {}",
        snapshot.state, snapshot.sound_pack, snapshot.volume
    );
}
