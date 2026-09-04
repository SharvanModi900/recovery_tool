//! Generic forensic extraction interface for all apps.
//! 
//! This module provides a unified interface for extracting data from
//! messaging apps, including deleted record recovery via SQLite carving.

use serde::{Deserialize, Serialize};
use crate::adb::AdbClient;
use crate::sqlite::{self, carve};
use super::{instagram, messenger, telegram};

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
        _ => ForensicExtraction {
            app_id: app_id.to_string(),
            app_name: app_id.to_string(),
            media_files: Vec::new(),
            carved_messages: Vec::new(),
            databases_found: Vec::new(),
            databases_pulled: Vec::new(),
            extraction_notes: vec!["App not supported for deep extraction".to_string()],
        },
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
