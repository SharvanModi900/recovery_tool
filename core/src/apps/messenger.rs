//! Facebook Messenger data extraction module.
//! 
//! Extracts:
//! - Cache files from /sdcard/Android/data/com.facebook.orca/
//! - Database parsing (requires root): threads.db, messages.db

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

/// Messenger media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessengerMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String, // "image", "video", "audio"
    pub source: String,     // "cache", "media"
}

/// Messenger extraction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessengerData {
    pub media: Vec<MessengerMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
}

/// Extract Messenger data from device.
pub fn extract_messenger(adb: &AdbClient, serial: &str) -> MessengerData {
    let mut media = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Extract from cache (no root required)
    let cache_paths = [
        "/sdcard/Android/data/com.facebook.orca/cache",
        "/sdcard/Android/data/com.facebook.orca/files",
    ];

    for path in cache_paths {
        if let Ok(files) = scan_media_dir(adb, serial, path, "cache") {
            for file in files {
                if !seen_paths.contains(&file.path) {
                    seen_paths.insert(file.path.clone());
                    media.push(file);
                }
            }
        }
    }

    // 2. Check for database access (root required)
    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls -la /data/data/com.facebook.orca/databases/' 2>/dev/null"])
        .unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("threads.db") || db_text.contains("messages.db");
    let database_accessible = has_database;

    MessengerData {
        media,
        has_database,
        database_accessible,
    }
}

/// Scan a directory for media files.
fn scan_media_dir(adb: &AdbClient, serial: &str, path: &str, source: &str) -> Result<Vec<MessengerMedia>, String> {
    let cmd = format!(
        r"find {} -type f \( -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.png' -o -iname '*.mp4' -o -iname '*.mp3' -o -iname '*.aac' -o -iname '*.opus' \) -exec ls -la {{}} + 2>/dev/null",
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
        } else if lower.ends_with(".mp3") || lower.ends_with(".aac") || lower.ends_with(".opus") {
            "audio"
        } else {
            "image"
        };
        
        files.push(MessengerMedia {
            path: file_path,
            filename,
            size,
            media_type: media_type.to_string(),
            source: source.to_string(),
        });
    }
    
    Ok(files)
}
