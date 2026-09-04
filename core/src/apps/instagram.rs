//! Instagram data extraction module.
//! 
//! Extracts:
//! - Saved media from /sdcard/DCIM/Instagram/
//! - Cache files from /sdcard/Android/data/com.instagram.android/
//! - Database parsing (requires root): direct.db for DMs, main.db

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

/// Instagram media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String, // "image", "video"
    pub source: String,     // "dcim", "cache"
}

/// Instagram extraction result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstagramData {
    pub media: Vec<InstagramMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
}

/// Extract Instagram data from device.
pub fn extract_instagram(adb: &AdbClient, serial: &str) -> InstagramData {
    let mut media = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    // 1. Extract from DCIM/Instagram (saved media - no root required)
    let dcim_paths = [
        "/sdcard/DCIM/Instagram",
        "/sdcard/Pictures/Instagram",
    ];

    for path in dcim_paths {
        if let Ok(files) = scan_media_dir(adb, serial, path, "dcim") {
            for file in files {
                if !seen_paths.contains(&file.path) {
                    seen_paths.insert(file.path.clone());
                    media.push(file);
                }
            }
        }
    }

    // 2. Extract from Android/data cache (no root required)
    let cache_paths = [
        "/sdcard/Android/data/com.instagram.android/cache",
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

    // 3. Check for database access (root required)
    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls -la /data/data/com.instagram.android/databases/' 2>/dev/null"])
        .unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("direct.db") || db_text.contains("main.db");
    let database_accessible = has_database && db_text.contains("direct.db");

    InstagramData {
        media,
        has_database,
        database_accessible,
    }
}

/// Scan a directory for media files.
fn scan_media_dir(adb: &AdbClient, serial: &str, path: &str, source: &str) -> Result<Vec<InstagramMedia>, String> {
    let cmd = format!(
        r"find {} -type f \( -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.png' -o -iname '*.mp4' -o -iname '*.webp' \) -exec ls -la {{}} + 2>/dev/null",
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
        } else {
            "image"
        };
        
        files.push(InstagramMedia {
            path: file_path,
            filename,
            size,
            media_type: media_type.to_string(),
            source: source.to_string(),
        });
    }
    
    Ok(files)
}
