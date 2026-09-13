//! Generic forensic extraction interface for all apps.
//! 
//! This module provides a unified interface for extracting data from
//! messaging apps, including deleted record recovery via SQLite carving.

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;
use crate::sqlite::{self, carve};
use super::{instagram, messenger, telegram, snapchat, signal, viber, line, wechat};

/// Unified extraction result from any app.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicExtraction {
    pub app_id: String,
    pub app_name: String,
    pub media_files: Vec<ExtractedMedia>,
    pub carved_messages: Vec<sqlite::Message>,
    pub databases_found: Vec<String>,
    pub databases_pulled: Vec<String>,
    pub extraction_notes: Vec<String>,
}

/// An extracted media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String,
    pub source: String,
    pub local_path: Option<String>,
}

/// Extract forensic data from a specific app.
pub fn extract_app(
    adb: &AdbClient,
    serial: &str,
    app_id: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    match app_id {
        "instagram" => extract_instagram_forensic(adb, serial, temp_dir),
        "messenger" => extract_messenger_forensic(adb, serial, temp_dir),
        "telegram" => extract_telegram_forensic(adb, serial, temp_dir),
        "snapchat" => extract_snapchat_forensic(adb, serial, temp_dir),
        "signal" => extract_signal_forensic(adb, serial, temp_dir),
        "viber" => extract_viber_forensic(adb, serial, temp_dir),
        "line" => extract_line_forensic(adb, serial, temp_dir),
        "wechat" => extract_wechat_forensic(adb, serial, temp_dir),
        _ => extract_generic_forensic(adb, serial, app_id, temp_dir),
    }
}

/// Extract Instagram data with forensic carving.
fn extract_instagram_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let ig_data = instagram::extract_instagram(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "instagram".to_string(),
        app_name: "Instagram".to_string(),
        media_files: ig_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path,
            filename: m.filename,
            size: m.size,
            media_type: m.media_type,
            source: m.source,
            local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };
    
    // Try to pull and carve databases if root is available
    if ig_data.database_accessible {
        let db_paths = [
            "/data/data/com.instagram.android/databases/direct.db",
            "/data/data/com.instagram.android/databases/main.db",
        ];
        
        for db_path in db_paths {
            let local_db = temp_dir.join(format!("ig_{}", db_path.rsplit('/').next().unwrap_or("db")));
            if adb.pull(serial, db_path, &local_db).is_ok() {
                result.databases_pulled.push(local_db.display().to_string());
                
                // Carve deleted records
                match carve::carve_db(&local_db, sqlite::Source::Freelist) {
                    Ok(messages) => {
                        result.carved_messages.extend(messages);
                    }
                    Err(e) => {
                        result.extraction_notes.push(format!("Carve error for {}: {}", db_path, e));
                    }
                }
                
                // Try WAL carving
                let wal_path = local_db.with_extension("db-wal");
                if wal_path.exists() {
                    if let Ok(wal_messages) = carve::carve_wal(&wal_path, sqlite::Source::Wal) {
                        result.carved_messages.extend(wal_messages);
                    }
                }
            }
        }
    }
    
    result
}

/// Extract Messenger data with forensic carving.
fn extract_messenger_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let msg_data = messenger::extract_messenger(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "messenger".to_string(),
        app_name: "Facebook Messenger".to_string(),
        media_files: msg_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path,
            filename: m.filename,
            size: m.size,
            media_type: m.media_type,
            source: m.source,
            local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };
    
    // Try to pull and carve databases if root is available
    if msg_data.database_accessible {
        let db_paths = [
            "/data/data/com.facebook.orca/databases/threads.db",
            "/data/data/com.facebook.orca/databases/messages.db",
        ];
        
        for db_path in db_paths {
            let local_db = temp_dir.join(format!("msg_{}", db_path.rsplit('/').next().unwrap_or("db")));
            if adb.pull(serial, db_path, &local_db).is_ok() {
                result.databases_pulled.push(local_db.display().to_string());
                
                // Carve deleted records
                match carve::carve_db(&local_db, sqlite::Source::Freelist) {
                    Ok(messages) => {
                        result.carved_messages.extend(messages);
                    }
                    Err(e) => {
                        result.extraction_notes.push(format!("Carve error for {}: {}", db_path, e));
                    }
                }
                
                // Try WAL carving
                let wal_path = local_db.with_extension("db-wal");
                if wal_path.exists() {
                    if let Ok(wal_messages) = carve::carve_wal(&wal_path, sqlite::Source::Wal) {
                        result.carved_messages.extend(wal_messages);
                    }
                }
            }
        }
    }
    
    result
}

/// Extract Telegram data with forensic carving.
fn extract_telegram_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let tg_data = telegram::extract_telegram(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "telegram".to_string(),
        app_name: "Telegram".to_string(),
        media_files: tg_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path,
            filename: m.filename,
            size: m.size,
            media_type: m.media_type,
            source: m.source,
            local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };
    
    // Try to pull and carve databases if root is available
    if tg_data.database_accessible {
        let db_path = "/data/data/org.telegram.messenger/files/cache4.db";
        let local_db = temp_dir.join("tg_cache4.db");
        
        if adb.pull(serial, db_path, &local_db).is_ok() {
            result.databases_pulled.push(local_db.display().to_string());
            
            // Carve deleted records
            match carve::carve_db(&local_db, sqlite::Source::Freelist) {
                Ok(messages) => {
                    result.carved_messages.extend(messages);
                }
                Err(e) => {
                    result.extraction_notes.push(format!("Carve error for {}: {}", db_path, e));
                }
            }
            
            // Try WAL carving
            let wal_path = local_db.with_extension("db-wal");
            if wal_path.exists() {
                if let Ok(wal_messages) = carve::carve_wal(&wal_path, sqlite::Source::Wal) {
                    result.carved_messages.extend(wal_messages);
                }
            }
        }
    }
    
    result
}

/// Extract Snapchat data with forensic scanning.
fn extract_snapchat_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let sc_data = snapchat::extract_snapchat(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "snapchat".to_string(),
        app_name: "Snapchat".to_string(),
        media_files: sc_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path, filename: m.filename, size: m.size,
            media_type: m.media_type, source: m.source, local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };

    if sc_data.has_database {
        result.databases_found.push("arroyo.db".to_string());
        for db_path in ["/data/data/com.snapchat.android/databases/arroyo.db",
                        "/data/data/com.snapchat.android/databases/main.db"] {
            let local_db = temp_dir.join(format!("sc_{}", db_path.rsplit('/').next().unwrap_or("db")));
            if adb.pull(serial, db_path, &local_db).is_ok() {
                result.databases_pulled.push(local_db.display().to_string());
                if let Ok(msgs) = carve::carve_db(&local_db, sqlite::Source::Freelist) {
                    result.carved_messages.extend(msgs);
                }
            }
        }
    } else {
        result.extraction_notes.push("Snapchat database requires root access".to_string());
    }
    result
}

/// Extract Signal data with forensic scanning.
fn extract_signal_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let sig_data = signal::extract_signal(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "signal".to_string(),
        app_name: "Signal".to_string(),
        media_files: sig_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path, filename: m.filename, size: m.size,
            media_type: m.media_type, source: m.source, local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };

    if sig_data.has_database {
        result.databases_found.push("signal.db".to_string());
        let db_path = "/data/data/org.thoughtcrime.securesms/databases/signal.db";
        let local_db = temp_dir.join("signal.db");
        if adb.pull(serial, db_path, &local_db).is_ok() {
            result.databases_pulled.push(local_db.display().to_string());
            if let Ok(msgs) = carve::carve_db(&local_db, sqlite::Source::Freelist) {
                result.carved_messages.extend(msgs);
            }
        }
    } else {
        result.extraction_notes.push("Signal is privacy-focused — all data requires root".to_string());
    }
    if sig_data.backup_found {
        result.extraction_notes.push("Signal backup folder found on SD card".to_string());
    }
    result
}

/// Extract Viber data with forensic scanning.
fn extract_viber_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let vib_data = viber::extract_viber(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "viber".to_string(),
        app_name: "Viber".to_string(),
        media_files: vib_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path, filename: m.filename, size: m.size,
            media_type: m.media_type, source: m.source, local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };

    if vib_data.has_database {
        result.databases_found.push("viber_messages".to_string());
        for db_path in ["/data/data/com.viber.voip/databases/viber_messages",
                        "/data/data/com.viber.voip/databases/viber_data"] {
            let local_db = temp_dir.join(format!("viber_{}", db_path.rsplit('/').next().unwrap_or("db")));
            if adb.pull(serial, db_path, &local_db).is_ok() {
                result.databases_pulled.push(local_db.display().to_string());
                if let Ok(msgs) = carve::carve_db(&local_db, sqlite::Source::Freelist) {
                    result.carved_messages.extend(msgs);
                }
            }
        }
    } else {
        result.extraction_notes.push("Viber database requires root access".to_string());
    }
    result
}

/// Extract LINE data with forensic scanning.
fn extract_line_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let line_data = line::extract_line(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "line".to_string(),
        app_name: "LINE".to_string(),
        media_files: line_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path, filename: m.filename, size: m.size,
            media_type: m.media_type, source: m.source, local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };

    if line_data.has_database {
        result.databases_found.push("naver_line".to_string());
        let db_path = "/data/data/jp.naver.line.android/databases/naver_line";
        let local_db = temp_dir.join("line_naver.db");
        if adb.pull(serial, db_path, &local_db).is_ok() {
            result.databases_pulled.push(local_db.display().to_string());
            if let Ok(msgs) = carve::carve_db(&local_db, sqlite::Source::Freelist) {
                result.carved_messages.extend(msgs);
            }
        }
    } else {
        result.extraction_notes.push("LINE database requires root access".to_string());
    }
    result
}

/// Extract WeChat data with forensic scanning.
fn extract_wechat_forensic(
    adb: &AdbClient,
    serial: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let wc_data = wechat::extract_wechat(adb, serial);
    
    let mut result = ForensicExtraction {
        app_id: "wechat".to_string(),
        app_name: "WeChat".to_string(),
        media_files: wc_data.media.into_iter().map(|m| ExtractedMedia {
            path: m.path, filename: m.filename, size: m.size,
            media_type: m.media_type, source: m.source, local_path: None,
        }).collect(),
        carved_messages: Vec::new(),
        databases_found: Vec::new(),
        databases_pulled: Vec::new(),
        extraction_notes: Vec::new(),
    };

    if wc_data.has_database {
        result.databases_found.push("EnMicroMsg.db".to_string());
        result.extraction_notes.push("WeChat DB is SQLCipher encrypted — requires IMEI+UIN to decrypt".to_string());
    } else {
        result.extraction_notes.push("WeChat database requires root access".to_string());
    }
    result.extraction_notes.push(format!("WeChat media: {} images, {} videos, {} voice messages",
        wc_data.image_count, wc_data.video_count, wc_data.voice_count));
    result
}

/// Generic extraction for apps without dedicated extractors (Skype, Discord, Twitter, etc.).
fn extract_generic_forensic(
    adb: &AdbClient,
    serial: &str,
    app_id: &str,
    temp_dir: &std::path::Path,
) -> ForensicExtraction {
    let app_def = super::SUPPORTED_APPS.iter().find(|a| a.id == app_id);
    let Some(app_def) = app_def else {
        return ForensicExtraction {
            app_id: app_id.to_string(),
            app_name: app_id.to_string(),
            media_files: Vec::new(),
            carved_messages: Vec::new(),
            databases_found: Vec::new(),
            databases_pulled: Vec::new(),
            extraction_notes: vec!["Unknown app".to_string()],
        };
    };

    let mut media_files = Vec::new();
    let mut databases_found = Vec::new();
    let mut databases_pulled = Vec::new();
    let mut carved_messages = Vec::new();
    let mut notes = Vec::new();

    // Scan accessible paths for media
    for path in app_def.accessible_paths {
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.mp4' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.mp3' -o -iname '*.ogg' \) -exec ls -la {{}} + 2>/dev/null",
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
                if file_path.is_empty() { continue; }
                let filename = file_path.rsplit('/').next().unwrap_or(&file_path).to_string();
                let lower = file_path.to_lowercase();
                let media_type = if lower.ends_with(".mp4") { "video" }
                    else if lower.ends_with(".mp3") || lower.ends_with(".ogg") { "audio" }
                    else { "image" };
                media_files.push(ExtractedMedia {
                    path: file_path, filename, size,
                    media_type: media_type.to_string(),
                    source: "sdcard".to_string(),
                    local_path: None,
                });
            }
        }
    }

    // Try to pull databases via root
    for db in app_def.databases {
        databases_found.push(db.to_string());
        for root_path in app_def.root_paths {
            let full_db = format!("{}/{}", root_path, db);
            let local_db = temp_dir.join(format!("{}_{}", app_id, db));
            if adb.pull(serial, &full_db, &local_db).is_ok() {
                databases_pulled.push(local_db.display().to_string());
                if let Ok(msgs) = carve::carve_db(&local_db, sqlite::Source::Freelist) {
                    carved_messages.extend(msgs);
                }
            }
        }
    }

    if databases_pulled.is_empty() && !databases_found.is_empty() {
        notes.push(format!("{} databases found but require root access", app_def.name));
    }

    ForensicExtraction {
        app_id: app_id.to_string(),
        app_name: app_def.name.to_string(),
        media_files,
        carved_messages,
        databases_found,
        databases_pulled,
        extraction_notes: notes,
    }
}
