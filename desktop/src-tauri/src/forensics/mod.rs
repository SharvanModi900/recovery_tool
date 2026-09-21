//! Forensic analysis commands — split into focused modules.

#[macro_use]
pub mod device_info;
#[macro_use]
pub mod data_extraction;
#[macro_use]
pub mod network;
#[macro_use]
pub mod advanced;
#[macro_use]
pub mod security;
#[macro_use]
pub mod thermal;

// Re-export all commands so lib.rs can reference them directly.
pub use device_info::*;
pub use data_extraction::*;
pub use network::*;
pub use advanced::*;
pub use security::*;
pub use thermal::*;
