//! Telegram data extraction module.
//! 
//! Extracts:
//! - Saved media from /sdcard/Telegram/
//! - Database parsing (requires root): cache4.db

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

/// Telegram media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String, // "image", "video", "audio", "document"
    pub source: String,     // "telegram", "cache"
}

/// Telegram extraction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramData {
    pub media: Vec<TelegramMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
}

/// Extract Telegram data from device.
pub fn extract_telegram(adb: &AdbClient, serial: &str) -> TelegramData {
    let mut media = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Extract from /sdcard/Telegram (saved media - no root required)
    let telegram_paths = [
        "/sdcard/Telegram",
        "/sdcard/Telegram/Telegram Images",
        "/sdcard/Telegram/Telegram Video",
        "/sdcard/Telegram/Telegram Audio",
        "/sdcard/Telegram/Telegram Documents",
    ];

    for path in telegram_paths {
        if let Ok(files) = scan_media_dir(adb, serial, path, "telegram") {
            for file in files {
                if !seen_paths.contains(&file.path) {
                    seen_paths.insert(file.path.clone());
                    media.push(file);
                }
            }
        }
    }

    // 2. Check for database access (root required)
    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls -la /data/data/org.telegram.messenger/files/' 2>/dev/null"])
        .unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("cache4.db");
    let database_accessible = has_database;

    TelegramData {
        media,
        has_database,
        database_accessible,
    }
}

/// Scan a directory for media files.
fn scan_media_dir(adb: &AdbClient, serial: &str, path: &str, source: &str) -> Result<Vec<TelegramMedia>, String> {
    let cmd = format!(
        r"find {} -type f \( -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.mp4' -o -iname '*.mp3' -o -iname '*.ogg' -o -iname '*.opus' -o -iname '*.pdf' -o -iname '*.doc' -o -iname '*.docx' \) -exec ls -la {{}} + 2>/dev/null",
        path
    );
    
    let output = adb.exec_out(serial, &["sh", "-c", &cmd])
        .map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&output);
    
    let mut files = Vec::new();
    
    for line in text.lines() {
        if !line.starts_with('-') {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 {
            continue;
        }
        
        let size = parts[4].parse::<i64>().unwrap_or(0);
        let file_path = parts[7..].join(" ");
        
        if file_path.is_empty() {
            continue;
        }
        
        let filename = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
        let lower = file_path.to_lowercase();
        
        let media_type = if lower.ends_with(".mp4") {
            "video"
        } else if lower.ends_with(".mp3") || lower.ends_with(".ogg") || lower.ends_with(".opus") {
            "audio"
        } else if lower.ends_with(".pdf") || lower.ends_with(".doc") || lower.ends_with(".docx") {
            "document"
        } else {
            "image"
        };
        
        files.push(TelegramMedia {
            path: file_path,
            filename,
            size,
            media_type: media_type.to_string(),
            source: source.to_string(),
        });
    }
    
    Ok(files)
}
