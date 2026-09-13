//! Image thumbnail cache recovery.
//!
//! Even if photos are deleted from the device, the `.thumbnails` folder
//! retains smaller versions. This module scans thumbnail directories
//! and attempts to match them to original file paths.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// A recovered thumbnail entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailEntry {
    /// Path on device where the thumbnail was found.
    pub path: String,
    /// Guessed original image path (if determinable).
    pub original_path: Option<String>,
    /// File size in bytes.
    pub file_size: u64,
    /// Image width (if parseable from filename or metadata).
    pub width: Option<u32>,
    /// Image height.
    pub height: Option<u32>,
    /// Last modified timestamp (Unix seconds).
    pub timestamp: Option<i64>,
}

/// Result of thumbnail recovery scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThumbnailRecoveryResult {
    pub thumbnails: Vec<ThumbnailEntry>,
    pub total_count: usize,
    pub total_size: u64,
    pub notes: Vec<String>,
}

/// Thumbnail directories to scan on Android devices.
const THUMB_DIRS: &[&str] = &[
    "/sdcard/DCIM/.thumbnails",
    "/sdcard/DCIM/Camera/.thumbnails",
    "/sdcard/Pictures/.thumbnails",
    "/sdcard/Android/data/com.android.providers.media/databases",
];

/// Scan the device for thumbnail caches and return recovered entries.
pub fn recover_thumbnails(adb: &AdbClient, serial: &str) -> Result<ThumbnailRecoveryResult> {
    let mut notes = Vec::new();
    let mut thumbnails = Vec::new();

    for dir in THUMB_DIRS {
        match adb.exec_out(serial, &["ls", "-la", dir]) {
            Ok(output) if !output.is_empty() => {
                let text = String::from_utf8_lossy(&output);
                let entries = parse_ls_output(&text, dir);
                notes.push(format!("{dir}: {} thumbnail files", entries.len()));
                thumbnails.extend(entries);
            }
            Ok(_) => {
                notes.push(format!("{dir}: empty or inaccessible"));
            }
            Err(e) => {
                notes.push(format!("{dir}: {e}"));
            }
        }
    }

    // Also scan app-specific thumbnail caches.
    let app_thumb_dirs = [
        "/sdcard/DCIM/Camera",
        "/sdcard/Pictures",
    ];
    for dir in &app_thumb_dirs {
        // Look for .thumbdata files (Android's thumbnail database).
        let search_cmd = format!("find {dir} -maxdepth 2 -name '.thumbdata*' -o -name '*.thumb' 2>/dev/null");
        match adb.exec_out(serial, &["sh", "-c", &search_cmd]) {
            Ok(output) if !output.is_empty() => {
                let text = String::from_utf8_lossy(&output);
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() { continue; }
                    // Get file details.
                    if let Ok(detail) = adb.exec_out(serial, &["stat", "-c", "%s %Y", line]) {
                        let detail_str = String::from_utf8_lossy(&detail);
                        let parts: Vec<&str> = detail_str.trim().split_whitespace().collect();
                        let size = parts.first().and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
                        let ts = parts.get(1).and_then(|s| s.parse::<i64>().ok());
                        thumbnails.push(ThumbnailEntry {
                            path: line.to_string(),
                            original_path: guess_original_path(line),
                            file_size: size,
                            width: None,
                            height: None,
                            timestamp: ts,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    let total_size = thumbnails.iter().map(|t| t.file_size).sum();
    let total_count = thumbnails.len();

    if total_count == 0 {
        notes.push("No thumbnail caches found on device".into());
    }

    Ok(ThumbnailRecoveryResult {
        thumbnails,
        total_count,
        total_size,
        notes,
    })
}

/// Parse `ls -la` output into thumbnail entries.
fn parse_ls_output(text: &str, base_dir: &str) -> Vec<ThumbnailEntry> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        // Skip directories, total line, and non-file entries.
        if line.starts_with("total") || line.starts_with('d') || line.is_empty() {
            continue;
        }
        // Typical ls -la format: -rw-r--r-- 1 root sdcard_rw 12345 2024-01-15 10:30 filename
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        // Check if it's a regular file.
        if !parts[0].starts_with('-') {
            continue;
        }

        let size = parts[4].parse::<u64>().unwrap_or(0);
        let filename = parts[8..].join(" ");

        // Skip non-image files.
        let lower = filename.to_lowercase();
        if !lower.ends_with(".jpg") && !lower.ends_with(".jpeg") && !lower.ends_with(".png")
            && !lower.ends_with(".thumbdata") && !lower.ends_with(".thumb")
            && !lower.contains("thumb")
        {
            continue;
        }

        let full_path = if base_dir.ends_with('/') {
            format!("{base_dir}{filename}")
        } else {
            format!("{base_dir}/{filename}")
        };

        entries.push(ThumbnailEntry {
            path: full_path.clone(),
            original_path: guess_original_path(&full_path),
            file_size: size,
            width: None,
            height: None,
            timestamp: None,
        });
    }
    entries
}

/// Try to guess the original image path from a thumbnail path.
///
/// Android thumbnails often have names derived from the original file's
/// hash or a modified version of the original filename.
fn guess_original_path(thumb_path: &str) -> Option<String> {
    // .thumbdata files often contain the original filename in their name.
    if thumb_path.contains(".thumbdata") {
        // Pattern: .thumbdata3--<hash> or .thumbdata*<original_name>
        if let Some(idx) = thumb_path.rfind("--") {
            let suffix = &thumb_path[idx + 2..];
            if !suffix.is_empty() && suffix.len() < 200 {
                return Some(format!("/sdcard/DCIM/Camera/{suffix}"));
            }
        }
    }

    // For thumbnail files in .thumbnails/, the original is usually in the parent Camera/ folder.
    if thumb_path.contains(".thumbnails/") {
        let original = thumb_path.replace(".thumbnails/", "Camera/");
        // Strip thumbnail-specific suffixes.
        let original = original.replace(".thumb", "");
        if original != thumb_path {
            return Some(original);
        }
    }

    None
}
