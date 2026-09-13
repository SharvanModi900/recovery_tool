//! Multi-app detection and data extraction for forensic analysis.
//! 
//! Supports detection and extraction from:
//! - WhatsApp
//! - Instagram
//! - Snapchat
//! - Facebook Messenger
//! - Telegram
//! - Signal
//! - Viber
//! - Line
//! - WeChat

pub mod instagram;
pub mod messenger;
pub mod telegram;
pub mod snapchat;
pub mod signal;
pub mod viber;
pub mod line;
pub mod wechat;
pub mod extraction;

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;

/// Supported messaging apps with their package names and data paths.
#[derive(Debug, Clone)]
pub struct AppDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub package: &'static str,
    pub icon: &'static str,
    pub requires_root: bool,
    pub accessible_paths: &'static [&'static str],
    pub root_paths: &'static [&'static str],
    pub databases: &'static [&'static str],
}

/// All supported messaging apps.
pub const SUPPORTED_APPS: &[AppDefinition] = &[
    AppDefinition {
        id: "whatsapp",
        name: "WhatsApp",
        package: "com.whatsapp",
        icon: "💬",
        requires_root: false,
        accessible_paths: &[
            "/sdcard/Android/media/com.whatsapp/WhatsApp",
            "/sdcard/WhatsApp",
        ],
        root_paths: &[
            "/data/data/com.whatsapp/databases",
        ],
        databases: &[
            "msgstore.db",
            "wa.db",
            "axolotl.db",
        ],
    },
    AppDefinition {
        id: "instagram",
        name: "Instagram",
        package: "com.instagram.android",
        icon: "📷",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/com.instagram.android",
            "/sdcard/DCIM/Instagram",
        ],
        root_paths: &[
            "/data/data/com.instagram.android/databases",
        ],
        databases: &[
            "direct.db",
            "main.db",
        ],
    },
    AppDefinition {
        id: "snapchat",
        name: "Snapchat",
        package: "com.snapchat.android",
        icon: "👻",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/com.snapchat.android",
        ],
        root_paths: &[
            "/data/data/com.snapchat.android/databases",
        ],
        databases: &[
            "arroyo.db",
            "main.db",
        ],
    },
    AppDefinition {
        id: "messenger",
        name: "Facebook Messenger",
        package: "com.facebook.orca",
        icon: "💭",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/com.facebook.orca",
        ],
        root_paths: &[
            "/data/data/com.facebook.orca/databases",
        ],
        databases: &[
            "threads.db",
            "messages.db",
        ],
    },
    AppDefinition {
        id: "telegram",
        name: "Telegram",
        package: "org.telegram.messenger",
        icon: "✈️",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Telegram",
        ],
        root_paths: &[
            "/data/data/org.telegram.messenger/files",
        ],
        databases: &[
            "cache4.db",
        ],
    },
    AppDefinition {
        id: "signal",
        name: "Signal",
        package: "org.thoughtcrime.securesms",
        icon: "🔒",
        requires_root: true,
        accessible_paths: &[],
        root_paths: &[
            "/data/data/org.thoughtcrime.securesms/databases",
        ],
        databases: &[
            "signal.db",
        ],
    },
    AppDefinition {
        id: "viber",
        name: "Viber",
        package: "com.viber.voip",
        icon: "📞",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/com.viber.voip",
        ],
        root_paths: &[
            "/data/data/com.viber.voip/databases",
        ],
        databases: &[
            "viber_messages",
            "viber_data",
        ],
    },
    AppDefinition {
        id: "line",
        name: "LINE",
        package: "jp.naver.line.android",
        icon: "🟢",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/jp.naver.line.android",
            "/sdcard/LINE",
        ],
        root_paths: &[
            "/data/data/jp.naver.line.android/databases",
        ],
        databases: &[
            "naver_line",
        ],
    },
    AppDefinition {
        id: "wechat",
        name: "WeChat",
        package: "com.tencent.mm",
        icon: "💚",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Android/data/com.tencent.mm",
            "/sdcard/tencent/MicroMsg",
        ],
        root_paths: &[
            "/data/data/com.tencent.mm/MicroMsg",
        ],
        databases: &[
            "EnMicroMsg.db",
        ],
    },
    // --- Non-messaging apps ---
    AppDefinition {
        id: "skype",
        name: "Skype",
        package: "com.skype.raider",
        icon: "🔵",
        requires_root: false,
        accessible_paths: &[
            "/sdcard/Android/data/com.skype.raider",
        ],
        root_paths: &[
            "/data/data/com.skype.raider/databases",
        ],
        databases: &[
            "main.db",
        ],
    },
    AppDefinition {
        id: "discord",
        name: "Discord",
        package: "com.discord",
        icon: "🎮",
        requires_root: false,
        accessible_paths: &[
            "/sdcard/Android/data/com.discord",
        ],
        root_paths: &[
            "/data/data/com.discord/databases",
        ],
        databases: &[
            "discord.db",
        ],
    },
    AppDefinition {
        id: "twitter",
        name: "X (Twitter)",
        package: "com.twitter.android",
        icon: "🐦",
        requires_root: false,
        accessible_paths: &[
            "/sdcard/Android/data/com.twitter.android",
        ],
        root_paths: &[
            "/data/data/com.twitter.android/databases",
        ],
        databases: &[
            "twitter.db",
        ],
    },
    AppDefinition {
        id: "tiktok",
        name: "TikTok",
        package: "com.zhiliaoapp.musically",
        icon: "🎵",
        requires_root: false,
        accessible_paths: &[
            "/sdcard/Android/data/com.zhiliaoapp.musically",
        ],
        root_paths: &[
            "/data/data/com.zhiliaoapp.musically/databases",
        ],
        databases: &[
            "aweme.db",
        ],
    },
    AppDefinition {
        id: "threema",
        name: "Threema",
        package: "ch.threema.app",
        icon: "🟩",
        requires_root: true,
        accessible_paths: &[
            "/sdcard/Threema",
        ],
        root_paths: &[
            "/data/data/ch.threema.app/databases",
        ],
        databases: &[
            "threema.db",
        ],
    },
];

/// Detection result for a single app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDetection {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub package: String,
    pub is_installed: bool,
    pub is_accessible: bool,
    pub requires_root: bool,
    pub accessible_paths_found: Vec<String>,
    pub root_paths_found: Vec<String>,
    pub databases_found: Vec<String>,
    pub media_count: i64,
    pub has_data: bool,
}

/// Summary of all detected apps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppInventory {
    pub apps: Vec<AppDetection>,
    pub total_installed: usize,
    pub total_with_data: usize,
    pub device_has_root: bool,
}

/// Detect all supported messaging apps on the device.
pub fn detect_apps(adb: &AdbClient, serial: &str) -> AppInventory {
    // Get list of installed packages
    let packages_output = adb.exec_out(serial, &["sh", "-c", "pm list packages 2>/dev/null"])
        .unwrap_or_default();
    let packages_text = String::from_utf8_lossy(&packages_output);
    let installed_packages: Vec<&str> = packages_text
        .lines()
        .filter_map(|l| l.strip_prefix("package:"))
        .collect();

    // Check for root access
    let root_check = adb.exec_out(serial, &["sh", "-c", "su -c 'id' 2>/dev/null"])
        .unwrap_or_default();
    let device_has_root = String::from_utf8_lossy(&root_check).contains("uid=0");

    let mut apps = Vec::new();
    let mut total_installed = 0;
    let mut total_with_data = 0;

    for app_def in SUPPORTED_APPS {
        let is_installed = installed_packages.iter().any(|p| p.starts_with(app_def.package));
        
        if is_installed {
            total_installed += 1;
        }

        // Check accessible paths
        let mut accessible_paths_found = Vec::new();
        let mut media_count = 0i64;
        
        for path in app_def.accessible_paths {
            let check_cmd = format!("ls -la {} 2>/dev/null", path);
            let output = adb.exec_out(serial, &["sh", "-c", &check_cmd]).unwrap_or_default();
            let text = String::from_utf8_lossy(&output);
            
            if !text.is_empty() && !text.contains("No such file") {
                accessible_paths_found.push(path.to_string());
                
                // Count media files if path exists
                if path.contains("WhatsApp") || path.contains("Instagram") || path.contains("Telegram") {
                    let count_cmd = format!("find {} -type f 2>/dev/null | wc -l", path);
                    let count_output = adb.exec_out(serial, &["sh", "-c", &count_cmd]).unwrap_or_default();
                    let count_text = String::from_utf8_lossy(&count_output);
                    if let Ok(count) = count_text.trim().parse::<i64>() {
                        media_count += count;
                    }
                }
            }
        }

        // Check root paths (if root is available)
        let mut root_paths_found = Vec::new();
        let mut databases_found = Vec::new();
        
        if device_has_root {
            for path in app_def.root_paths {
                let check_cmd = format!("su -c 'ls -la {}' 2>/dev/null", path);
                let output = adb.exec_out(serial, &["sh", "-c", &check_cmd]).unwrap_or_default();
                let text = String::from_utf8_lossy(&output);
                
                if !text.is_empty() && !text.contains("No such file") {
                    root_paths_found.push(path.to_string());
                    
                    // Check for specific databases
                    for db in app_def.databases {
                        if text.contains(db) {
                            databases_found.push(db.to_string());
                        }
                    }
                }
            }
        }

        let is_accessible = !accessible_paths_found.is_empty() || (!root_paths_found.is_empty() && device_has_root);
        let has_data = is_accessible && (media_count > 0 || !databases_found.is_empty());
        
        if has_data {
            total_with_data += 1;
        }

        apps.push(AppDetection {
            id: app_def.id.to_string(),
            name: app_def.name.to_string(),
            icon: app_def.icon.to_string(),
            package: app_def.package.to_string(),
            is_installed,
            is_accessible,
            requires_root: app_def.requires_root,
            accessible_paths_found,
            root_paths_found,
            databases_found,
            media_count,
            has_data,
        });
    }

    AppInventory {
        apps,
        total_installed,
        total_with_data,
        device_has_root,
    }
}

/// Check if a specific app is installed.
pub fn is_app_installed(adb: &AdbClient, serial: &str, package: &str) -> bool {
    let output = adb.exec_out(serial, &["sh", "-c", &format!("pm list packages {} 2>/dev/null", package)])
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&output);
    text.contains(&format!("package:{}", package))
}

/// Get accessible media files from an app.
pub fn get_app_media(adb: &AdbClient, serial: &str, app_id: &str) -> Vec<(String, i64)> {
    let app_def = SUPPORTED_APPS.iter().find(|a| a.id == app_id);
    let Some(app_def) = app_def else {
        return Vec::new();
    };

    let mut files = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for path in app_def.accessible_paths {
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.mp4' -o -iname '*.mp3' -o -iname '*.ogg' -o -iname '*.opus' \) -exec ls -la {{}} + 2>/dev/null",
            path
        );
        let output = adb.exec_out(serial, &["sh", "-c", &cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);

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
            
            if !file_path.is_empty() && !seen.contains(&file_path) {
                seen.insert(file_path.clone());
                files.push((file_path, size));
            }
        }
    }

    files
}
