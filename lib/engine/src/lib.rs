//! Core of Workbench: domain types, ports and use cases.
//!
//! Does no I/O. Adapters in the services implement the ports defined here.

mod domain;
mod error;
mod init;
mod ports;

pub use domain::Platform;
pub use error::Error;
pub use init::init;
pub use ports::ConfigStore;
