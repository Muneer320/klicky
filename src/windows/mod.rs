//! Windows control-plane components. The application host is integrated separately.
mod config;
pub mod control;
mod files;
mod lifecycle;
mod paths;
pub mod pipe;
mod process;
pub mod protocol;

pub use config::ConfigStore;
pub use lifecycle::InstanceOwner;
pub use paths::Paths;
pub use process::ServerProcess;
