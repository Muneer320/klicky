//! Windows control-plane components. The application host is integrated separately.
mod config;
mod files;
mod lifecycle;
mod paths;
pub mod protocol;

pub use config::ConfigStore;
pub use lifecycle::InstanceOwner;
pub use paths::Paths;
