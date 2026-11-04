//! Windows control-plane components. The application host is integrated separately.
mod config;
mod files;
mod paths;

pub use config::ConfigStore;
pub use paths::Paths;
