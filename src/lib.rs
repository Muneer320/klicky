//! Callable command-line and foreground-engine entrypoints for Klicky.

mod cli;
mod config;
mod engine;
mod ipc;
mod lifecycle;
mod listener;
#[cfg(target_os = "macos")]
mod media_keys;
mod player;
mod service;
mod soundpack;

pub use cli::run as run_cli;
pub use engine::run as run_engine;
