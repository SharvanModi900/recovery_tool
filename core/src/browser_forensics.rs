//! Browser forensics: Chrome history, bookmarks, downloads, cookies.
//!
//! Pulls Chrome's `History` SQLite database from the device via ADB
//! `exec-out cat` (requires root), then parses it through rusqlite.

use crate::adb::AdbClient;
use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

/// Browser history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserHistoryEntry {
    pub url: String,
    pub title: Option<String>,
    pub visit_count: i64,
    pub last_visit: Option<i64>,
    pub typed_count: i64,
}

/// Browser bookmark entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookmarkEntry {
    pub url: String,
    pub title: Option<String>,
    pub folder: Option<String>,
    pub created: Option<i64>,
}

/// Browser download entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadEntry {
    pub url: String,
    pub filename: Option<String>,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub start_time: Option<i64>,
}

/// Browser cookie entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub host_key: String,
    pub name: String,
    pub path: String,
    pub is_secure: bool,
    pub is_httponly: bool,
    pub expires_utc: Option<i64>,
    pub last_access: Option<i64>,
}

/// Full browser forensics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserForensicsResult {
    pub history: Vec<BrowserHistoryEntry>,
    pub bookmarks: Vec<BookmarkEntry>,
    pub downloads: Vec<DownloadEntry>,
    pub cookies: Vec<CookieEntry>,
    pub sha256: String,
    pub notes: Vec<String>,
}

/// Known Chrome History database paths on Android.
const CHROME_DB_PATHS: &[&str] = &[
    "/data/data/com.android.chrome/app_chrome/Default/History",
    "/data/user/0/com.android.chrome/app_chrome/Default/History",
];

/// Chrome Cookies database paths.
const CHROME_COOKIE_PATHS: &[&str] = &[
    "/data/data/com.android.chrome/app_chrome/Default/Cookies",
    "/data/user/0/com.android.chrome/app_chrome/Default/Cookies",
];

/// Collect browser forensics from the device.
pub fn collect_browser_data(adb: &AdbClient, serial: &str) -> Result<BrowserForensicsResult> {
    let mut notes = Vec::new();
    let mut history = Vec::new();
    let mut bookmarks = Vec::new();
    let mut downloads = Vec::new();
    let mut cookies = Vec::new();
    let mut hash = String::new();

    // Try to pull Chrome History database.
    let mut db_data: Option<Vec<u8>> = None;
    let mut used_path = "";
    for path in CHROME_DB_PATHS {
        match adb.exec_out(serial, &["cat", path]) {
            Ok(data) if data.len() > 100 => {
                db_data = Some(data);
                used_path = path;
                break;
            }
            Ok(_) => notes.push(format!("{path}: too small or empty")),
            Err(e) => notes.push(format!("{path}: {e}")),
        }
    }

    if let Some(data) = db_data {
        // Compute hash.
        hash = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(&data);
            hex::encode(h.finalize())
        };

        // Write to temp file.
        let tmp_dir = std::env::temp_dir().join("recoverkit_browser");
        let _ = std::fs::create_dir_all(&tmp_dir);
        let local_db = tmp_dir.join("chrome_history.db");
        std::fs::write(&local_db, &data).ok();

        // Try to pull WAL.
        let wal_remote = format!("{used_path}-wal");
        let local_wal = tmp_dir.join("chrome_history.db-wal");
        if let Ok(wal_data) = adb.exec_out(serial, &["cat", &wal_remote]) {
            if wal_data.len() > 32 {
                std::fs::write(&local_wal, &wal_data).ok();
            }
        }

        // Parse the database.
        if let Ok(conn) = Connection::open_with_flags(
            &local_db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            // History
            if let Ok(mut stmt) = conn.prepare(
                "SELECT url, title, visit_count, last_visit_time, typed_count \
                 FROM urls ORDER BY last_visit_time DESC LIMIT 10000"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok(BrowserHistoryEntry {
                        url: r.get::<_, String>(0).unwrap_or_default(),
                        title: r.get::<_, String>(1).ok(),
                        visit_count: r.get::<_, i64>(2).unwrap_or(0),
                        last_visit: r.get::<_, i64>(3).ok(),
                        typed_count: r.get::<_, i64>(4).unwrap_or(0),
                    })
                }) {
                    for r in rows.flatten() {
                        history.push(r);
                    }
                }
            }

            // Bookmarks
            if let Ok(mut stmt) = conn.prepare(
                "SELECT url, title, parent, date_added \
                 FROM urls WHERE id IN (SELECT url_id FROM bookmarks WHERE url_id IS NOT NULL) \
                 LIMIT 5000"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok(BookmarkEntry {
                        url: r.get::<_, String>(0).unwrap_or_default(),
                        title: r.get::<_, String>(1).ok(),
                        folder: r.get::<_, i64>(2).ok().map(|p| format!("folder_{p}")),
                        created: r.get::<_, i64>(3).ok(),
                    })
                }) {
                    for r in rows.flatten() {
                        bookmarks.push(r);
                    }
                }
            } else {
                // Fallback: try the bookmarks table directly.
                if let Ok(mut stmt) = conn.prepare(
                    "SELECT b.url, b.title, \
                     (SELECT title FROM bookmarks WHERE id = b.parent) as folder, \
                     b.date_added \
                     FROM bookmarks b WHERE b.type = 0 AND b.url IS NOT NULL \
                     LIMIT 5000"
                ) {
                    if let Ok(rows) = stmt.query_map([], |r| {
                        Ok(BookmarkEntry {
                            url: r.get::<_, String>(0).unwrap_or_default(),
                            title: r.get::<_, String>(1).ok(),
                            folder: r.get::<_, String>(2).ok(),
                            created: r.get::<_, i64>(3).ok(),
                        })
                    }) {
                        for r in rows.flatten() {
                            bookmarks.push(r);
                        }
                    }
                }
            }

            // Downloads
            if let Ok(mut stmt) = conn.prepare(
                "SELECT tab_url, target_path, total_bytes, received_bytes, start_time \
                 FROM downloads ORDER BY start_time DESC LIMIT 5000"
            ) {
                if let Ok(rows) = stmt.query_map([], |r| {
                    Ok(DownloadEntry {
                        url: r.get::<_, String>(0).unwrap_or_default(),
                        filename: r.get::<_, String>(1).ok(),
                        total_bytes: r.get::<_, i64>(2).unwrap_or(0),
                        received_bytes: r.get::<_, i64>(3).unwrap_or(0),
                        start_time: r.get::<_, i64>(4).ok(),
                    })
                }) {
                    for r in rows.flatten() {
                        downloads.push(r);
                    }
                }
            }

            // Autofill data (form fills)
            // Note: autofill is in a separate table, skip for now.
        }

        // Clean up.
        let _ = std::fs::remove_file(&local_db);
        let _ = std::fs::remove_file(&local_wal);

        notes.push(format!(
            "Pulled {} bytes from {}, {} history, {} bookmarks, {} downloads",
            data.len(), used_path, history.len(), bookmarks.len(), downloads.len()
        ));
    } else {
        notes.push("Chrome History database not accessible — root may be required".into());
    }

    // Try to pull Chrome Cookies database.
    for path in CHROME_COOKIE_PATHS {
        match adb.exec_out(serial, &["cat", path]) {
            Ok(data) if data.len() > 100 => {
                let tmp_dir = std::env::temp_dir().join("recoverkit_browser");
                let local_cookies = tmp_dir.join("chrome_cookies.db");
                std::fs::write(&local_cookies, &data).ok();

                if let Ok(conn) = Connection::open_with_flags(
                    &local_cookies,
                    OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                ) {
                    if let Ok(mut stmt) = conn.prepare(
                        "SELECT host_key, name, path, is_secure, is_httponly, expires_utc, last_access_utc \
                         FROM cookies LIMIT 10000"
                    ) {
                        if let Ok(rows) = stmt.query_map([], |r| {
                            Ok(CookieEntry {
                                host_key: r.get::<_, String>(0).unwrap_or_default(),
                                name: r.get::<_, String>(1).unwrap_or_default(),
                                path: r.get::<_, String>(2).unwrap_or_default(),
                                is_secure: r.get::<_, i64>(3).unwrap_or(0) != 0,
                                is_httponly: r.get::<_, i64>(4).unwrap_or(0) != 0,
                                expires_utc: r.get::<_, i64>(5).ok(),
                                last_access: r.get::<_, i64>(6).ok(),
                            })
                        }) {
                            for r in rows.flatten() {
                                cookies.push(r);
                            }
                        }
                    }
                }

                let _ = std::fs::remove_file(&local_cookies);
                notes.push(format!("{} cookies from {}", cookies.len(), path));
                break;
            }
            Ok(_) => {}
            Err(_) => {}
        }
    }

    Ok(BrowserForensicsResult {
        history,
        bookmarks,
        downloads,
        cookies,
        sha256: hash,
        notes,
    })
}
