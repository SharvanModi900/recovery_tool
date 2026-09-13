//! WeChat data extraction module.
//! Extracts media from /sdcard/tencent/MicroMsg/ and checks for EnMicroMsg.db.

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeChatData {
    pub media: Vec<WeChatMedia>,
    pub has_database: bool,
    pub database_accessible: bool,
    pub image_count: i64,
    pub video_count: i64,
    pub voice_count: i64,
}

pub fn extract_wechat(adb: &AdbClient, serial: &str) -> WeChatData {
    let mut media = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut image_count = 0i64;
    let mut video_count = 0i64;
    let mut voice_count = 0i64;

    for (path, source) in &[
        ("/sdcard/tencent/MicroMsg", "micromsg"),
        ("/sdcard/tencent/MicroMsg/WeiXin", "saved"),
        ("/sdcard/Android/data/com.tencent.mm", "cache"),
    ] {
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.mp4' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.amr' -o -iname '*.silk' \) -exec ls -la {{}} + 2>/dev/null",
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
                let lower = file_path.to_lowercase();
                let media_type = if lower.ends_with(".mp4") { "video" }
                    else if lower.ends_with(".amr") || lower.ends_with(".silk") { "voice" }
                    else { "image" };
                match media_type {
                    "video" => video_count += 1,
                    "voice" => voice_count += 1,
                    _ => image_count += 1,
                }
                media.push(WeChatMedia { path: file_path, filename, size, media_type: media_type.to_string(), source: source.to_string() });
            }
        }
    }

    let db_check = adb.exec_out(serial, &["sh", "-c", "su -c 'ls /data/data/com.tencent.mm/MicroMsg/' 2>/dev/null"]).unwrap_or_default();
    let db_text = String::from_utf8_lossy(&db_check);
    let has_database = db_text.contains("EnMicroMsg.db");

    WeChatData { media, has_database, database_accessible: false, image_count, video_count, voice_count }
}
