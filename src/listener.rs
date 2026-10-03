use std::sync::mpsc::Sender;
use std::time::Instant;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "linux")]
pub use linux::start_listening;
#[cfg(target_os = "macos")]
pub use macos::start_listening;

pub struct KeyEvent {
    pub key_name: &'static str,
    pub timestamp: Instant,
}

pub type KeySender = Sender<KeyEvent>;
