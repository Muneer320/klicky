//! Shared sound/input types and platform application entrypoints for Klicky.

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod cli;
mod config;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod engine;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod ipc;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod lifecycle;
mod listener;
#[cfg(target_os = "macos")]
mod media_keys;
mod player;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod service;
mod soundpack;
#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use cli::run as run_cli;
pub use config::Config;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub use engine::run as run_engine;
pub use listener::{KeyEvent, KeySender};
pub use player::Player;
pub use soundpack::SoundPack;

/// Report an unavailable application runtime without opening input or audio.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn run_cli() -> anyhow::Result<()> {
    anyhow::bail!(
        "Klicky's application runtime is not yet available on {}. Windows qualification uses the separate tools/windows-qualification executable.",
        std::env::consts::OS
    )
}
