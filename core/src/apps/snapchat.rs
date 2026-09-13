//! Snapchat data extraction module.
//! Extracts media from SD card cache and checks for arroyo.db (messages).

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapchatMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapchatData {
    pub media: Vec<SnapchatMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
}

pub fn extract_snapchat(adb: &AdbClient, serial: &str) -> SnapchatData {
    let mut media = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (path, source) in &[
        ("/sdcard/Android/data/com.snapchat.android", "cache"),
    ] {
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.mp4' -o -iname '*.png' -o -iname '*.gif' \) -exec ls -la {{}} + 2>/dev/null",
            path
        );
        if let Ok(output) = adb.exec_out(serial, &["sh", "-c", &cmd]) {
            let text = String::from_utf8_lossy(&output);
            for line in text.lines() {
                if !line.starts_with('-') { continue; }
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 8 { continue; }
                let size = parts[4].parse::<i64>().unwrap_or(0);
                let file_path = parts[7..].join(" ");
                if file_path.is_empty() || seen.contains(&file_path) { continue; }
                seen.insert(file_path.clone());
                let filename = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
                let media_type = if file_path.to_lowercase().ends_with(".mp4") { "video" } else { "image" };
                media.push(SnapchatMedia { path: file_path, filename, size, media_type: media_type.to_string(), source: source.to_string() });
            }
        }
    }

    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls /data/data/com.snapchat.android/databases/' 2>/dev/null"]).unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("arroyo.db") || db_text.contains("main.db");

    SnapchatData { media, has_database, database_accessible: false }
}
