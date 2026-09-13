//! Clipboard + notification log forensics.
//!
//! Reads recent clipboard contents and notification history.
//! Clipboard may be empty on modern Android (security restriction).

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationEntry {
    pub package: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub timestamp: Option<i64>,
    pub channel: Option<String>,
}

/// Collect recent notification entries from the device.
pub fn collect_notifications(adb: &AdbClient, serial: &str) -> Result<Vec<NotificationEntry>> {
    let out = adb.exec_out(serial, &["dumpsys", "notification"])?;
    let text = String::from_utf8_lossy(&out);
    let mut notifications = Vec::new();

    let mut current_pkg = String::new();
    let mut current_title: Option<String> = None;
    let mut current_text: Option<String> = None;
    let mut current_time: Option<i64> = None;
    let mut current_channel: Option<String> = None;
    let mut in_notification = false;

    for line in text.lines() {
        let trimmed = line.trim();

        // Notification record start
        if trimmed.starts_with("NotificationRecord(") || trimmed.contains("NotificationRecord") {
            // Save previous
            if !current_pkg.is_empty() {
                notifications.push(NotificationEntry {
                    package: current_pkg.clone(),
                    title: current_title.take(),
                    text: current_text.take(),
                    timestamp: current_time.take(),
                    channel: current_channel.take(),
                });
            }
            in_notification = true;
            current_pkg = String::new();
            current_title = None;
            current_text = None;
            current_time = None;
            current_channel = None;

            // Extract package from "NotificationRecord(0x...: pkg/com.pkg)"
            if let Some(start) = trimmed.find("pkg=").or_else(|| trimmed.find(':')) {
                let rest = &trimmed[start..];
                if let Some(pkg_start) = rest.find(|c: char| c.is_alphanumeric() && c != 'p' && c != 'k' && c != 'g' && c != '=') {
                    let pkg_rest = &rest[pkg_start..];
                    let pkg_end = pkg_rest.find(|c: char| c == '/' || c == ')' || c == ' ' || c == ',').unwrap_or(pkg_rest.len());
                    current_pkg = pkg_rest[..pkg_end].to_string();
                }
            }
            continue;
        }

        if !in_notification {
            continue;
        }

        // Extract fields
        if trimmed.starts_with("android.title=") || trimmed.starts_with("Title:") {
            current_title = Some(trimmed.split('=').nth(1).or_else(|| trimmed.split(':').nth(1)).unwrap_or("").trim().trim_matches('"').to_string());
        }
        if trimmed.starts_with("android.text=") || trimmed.starts_with("Text:") {
            current_text = Some(trimmed.split('=').nth(1).or_else(|| trimmed.split(':').nth(1)).unwrap_or("").trim().trim_matches('"').to_string());
        }
        if trimmed.starts_with("time=") || trimmed.starts_with("postTime=") {
            if let Some(val) = trimmed.split('=').nth(1) {
                current_time = val.trim().split_whitespace().next().and_then(|v| v.parse().ok());
            }
        }
        if trimmed.starts_with("android.channelId=") || trimmed.starts_with("channel=") {
            current_channel = Some(trimmed.split('=').nth(1).unwrap_or("").trim().trim_matches('"').to_string());
        }
        if trimmed.starts_with("pkg=") {
            let val = trimmed.split('=').nth(1).unwrap_or("").trim();
            if !val.is_empty() && current_pkg.is_empty() {
                current_pkg = val.split(|c: char| c == '/' || c == ' ' || c == ')').next().unwrap_or(val).to_string();
            }
        }
    }

    // Save last notification
    if !current_pkg.is_empty() {
        notifications.push(NotificationEntry {
            package: current_pkg,
            title: current_title,
            text: current_text,
            timestamp: current_time,
            channel: current_channel,
        });
    }

    // Limit to most recent 100
    notifications.truncate(100);

    Ok(notifications)
}
