//! LINE data extraction module.
//! Extracts media from /sdcard/LINE/ and checks for naver_line database.

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineData {
    pub media: Vec<LineMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
}

pub fn extract_line(adb: &AdbClient, serial: &str) -> LineData {
    let mut media = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (path, source) in &[
        ("/sdcard/LINE", "shared"),
        ("/sdcard/LINE/Images", "images"),
        ("/sdcard/LINE/Movies", "movies"),
        ("/sdcard/LINE/Voice", "voice"),
        ("/sdcard/Android/data/jp.naver.line.android", "cache"),
    ] {
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.mp4' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.mp3' -o -iname '*.m4a' \) -exec ls -la {{}} + 2>/dev/null",
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
                let media_type = if file_path.to_lowercase().ends_with(".mp4") { "video" }
                    else if file_path.to_lowercase().ends_with(".mp3") || file_path.to_lowercase().ends_with(".m4a") { "audio" }
                    else { "image" };
                media.push(LineMedia { path: file_path, filename, size, media_type: media_type.to_string(), source: source.to_string() });
            }
        }
    }

    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls /data/data/jp.naver.line.android/databases/' 2>/dev/null"]).unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("naver_line");

    LineData { media, has_database, database_accessible: false }
}
