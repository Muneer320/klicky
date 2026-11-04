//! Windows control-plane components. The application host is integrated separately.
pub mod audio;
mod config;
pub mod control;
mod files;
pub mod input;
mod lifecycle;
mod paths;
pub mod pipe;
mod process;
pub mod protocol;
pub mod runtime;
pub mod transactions;

pub use config::ConfigStore;
pub use lifecycle::InstanceOwner;
pub use paths::Paths;
pub use process::ServerProcess;
