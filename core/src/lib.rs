//! recovery-core: read-only WhatsApp/Android data recovery engine.
//!
//! Modules:
//! - `adb`: read-only ADB acquisition layer (strict command allowlist)
//! - `scanner`: artifact discovery on device or in a local backup folder
//! - `sqlite`: msgstore parsing + deleted-record carving (freeblocks, freelist, WAL)
//! - `crypto`: WhatsApp crypt12/14/15 backup decryption (AES-256-GCM)

pub mod acquisition;
pub mod adb;
pub mod crypto;
pub mod error;
pub mod scanner;
pub mod sms_xml;
pub mod sqlite;
pub mod txt_export;
pub mod vcf;

pub use error::{CoreError, Result};

/// Tool version, reported by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
