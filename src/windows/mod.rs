//! Windows control-plane components. The application host is integrated separately.
pub mod audio;
pub(crate) mod cli;
mod config;
pub mod control;
pub mod desktop;
mod files;
pub mod input;
mod lifecycle;
mod paths;
pub mod pipe;
mod process;
pub mod protocol;
pub mod runtime;
mod session;
pub mod transactions;

pub use config::ConfigStore;
pub use lifecycle::InstanceOwner;
pub use paths::Paths;
pub use process::ServerProcess;
