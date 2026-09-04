//! recovery-core CLI.
//!
//! Examples:
//!   recovery-core devices
//!   recovery-core scan-device <serial> --dest workspace --pull
//!   recovery-core scan-folder D:\backups\whatsapp
//!   recovery-core decrypt msgstore.db.crypt14 key msgstore.db
//!   recovery-core recover msgstore.db -o recovered.json
//!
//! All JSON goes to stdout (or `-o <file>`); human notes go to stderr.

use clap::{Parser, Subcommand};
use recovery_core::{adb, crypto, scanner, sqlite, VERSION};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "recovery-core",
    version = VERSION,
    about = "Read-only WhatsApp/Android data recovery engine"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List connected devices (read-only).
    Devices,
    /// Scan a connected device for WhatsApp artifacts (read-only listing).
    ScanDevice {
        serial: String,
        /// Also pull discovered artifacts into --dest.
        #[arg(long)]
        pull: bool,
        /// Directory to pull artifacts into (default: ./workspace/<serial>).
        #[arg(long)]
        dest: Option<PathBuf>,
        /// Pull media files too (databases/backups are pulled by default).
        #[arg(long)]
        include_media: bool,
    },
    /// Scan a locally imported backup folder.
    ScanFolder { path: PathBuf },
    /// Parse live messages from a msgstore database.
    Parse {
        db: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Parse live messages AND carve deleted records (freeblocks/freelist/WAL).
    Recover {
        db: PathBuf,
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Decrypt a crypt12/14/15 backup with the device key file.
    Decrypt {
        input: PathBuf,
        key: PathBuf,
        output: PathBuf,
    },
}

fn print_json<T: serde::Serialize>(value: &T, output: Option<PathBuf>) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(value)?;
    match output {
        Some(path) => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&path, &json)?;
            eprintln!("written to {}", path.display());
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Devices => {
            let adb = adb::AdbClient::new();
            print_json(&adb.list_devices()?, None)?;
        }
        Cmd::ScanDevice {
            serial,
            pull,
            dest,
            include_media,
        } => {
            let adb = adb::AdbClient::new();
            let report = scanner::scan_device(&adb, &serial)?;
            print_json(&report, None)?;
            if pull {
                if report.artifacts.is_empty() {
                    eprintln!("nothing to pull");
                } else {
                    let dest = dest.unwrap_or_else(|| PathBuf::from("workspace").join(&serial));
                    eprintln!("pulling {} artifacts into {}", report.artifacts.len(), dest.display());
                    let pulled = scanner::pull_artifacts(
                        &adb,
                        &serial,
                        &report.artifacts,
                        &dest,
                        include_media,
                        |i, total, path| eprintln!("[{i}/{total}] {path}"),
                    )?;
                    eprintln!("pulled {} files", pulled.len());
                }
            }
        }
        Cmd::ScanFolder { path } => {
            if !path.exists() {
                anyhow::bail!("folder not found: {}", path.display());
            }
            print_json(&scanner::scan_folder(&path)?, None)?;
        }
        Cmd::Parse { db, output } => {
            print_json(&sqlite::parse_msgstore(&db)?, output)?;
        }
        Cmd::Recover { db, output } => {
            print_json(&sqlite::recover(&db)?, output)?;
        }
        Cmd::Decrypt {
            input,
            key,
            output,
        } => {
            print_json(&crypto::decrypt_backup(&input, &key, &output)?, None)?;
        }
    }
    Ok(())
}
