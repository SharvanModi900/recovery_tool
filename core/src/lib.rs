//! recovery-core: read-only WhatsApp/Android data recovery engine.
//!
//! Modules:
//! - `adb`: read-only ADB acquisition layer (strict command allowlist)
//! - `scanner`: artifact discovery on device or in a local backup folder
//! - `sqlite`: msgstore parsing + deleted-record carving (freeblocks, freelist, WAL)
//! - `crypto`: WhatsApp crypt12/14/15 backup decryption (AES-256-GCM)
//! - `apps`: multi-app detection and forensic extraction

pub mod apk_analyzer;
pub mod acquisition;
pub mod account_forensics;
pub mod adb;
pub mod apps;
pub mod bluetooth_forensics;
pub mod browser_forensics;
pub mod call_monitor;
pub mod case_file;
pub mod clipboard_forensics;
pub mod crypto;
pub mod db_carver;
pub mod deleted_sms;
pub mod error;
pub mod exif_parser;
pub mod fastboot;
pub mod forensic_report;
pub mod keyboard_forensics;
pub mod location_forensics;
pub mod network_forensics;
pub mod power_forensics;
pub mod remote_agent;
pub mod ringtone;
pub mod scanner;
pub mod screen_capture;
pub mod sim_forensics;
pub mod sms_xml;
pub mod sqlite;
pub mod thumbnail_recovery;
pub mod txt_export;
pub mod unified_timeline;
pub mod vcf;
pub mod wifi_forensics;

pub use error::{CoreError, Result};

/// Tool version, reported by the CLI.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
