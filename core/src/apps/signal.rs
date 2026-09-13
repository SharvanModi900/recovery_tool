//! Signal data extraction module.
//! Signal is privacy-focused — no SD card data. All data in app sandbox.
//! Extracts: database check (root), media from shared storage if any.

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalData {
    pub media: Vec<SignalMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
    pub backup_found: bool,
}

pub fn extract_signal(adb: &AdbClient, serial: &str) -> SignalData {
    let mut media = Vec::new();

    // Signal stores media in /sdcard/Signal/ on some versions
    let cmd = r"find /sdcard/Signal -type f \( -iname '*.jpg' -o -iname '*.mp4' -o -iname '*.png' -o -iname '*.mp3' -o -iname '*.ogg' \) -exec ls -la {} + 2>/dev/null";
    if let Ok(output) = adb.exec_out(serial, &["sh", "-c", cmd]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines() {
            if !line.starts_with('-') { continue; }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 8 { continue; }
            let size = parts[4].parse::<i64>().unwrap_or(0);
            let file_path = parts[7..].join(" ");
            if file_path.is_empty() { continue; }
            let filename = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
            let media_type = if file_path.to_lowercase().ends_with(".mp4") { "video" }
                else if file_path.to_lowercase().ends_with(".mp3") || file_path.to_lowercase().ends_with(".ogg") { "audio" }
                else { "image" };
            media.push(SignalMedia { path: file_path, filename, size, media_type: media_type.to_string(), source: "shared".to_string() });
        }
    }

    // Check for database access (root required)
    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls /data/data/org.thoughtcrime.securesms/databases/' 2>/dev/null"]).unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("signal.db");

    // Check for Signal backup files
    let backup_check = adb.exec_out(serial, &["sh", "-c", "ls /sdcard/Signal/ 2>/dev/null"]).unwrap_or_default();
    let backup_text = String::from_utf8_lossy(&backup_check);
    let backup_found = backup_text.contains("signal") || backup_text.contains("backup");

    SignalData { media, has_database, database_accessible: false, backup_found }
}
