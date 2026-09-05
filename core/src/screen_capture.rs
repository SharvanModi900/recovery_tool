//! Screen capture history — automatic periodic screenshots saved locally.
//!
//! Captures the Android device screen at regular intervals and stores
//! both the image files and metadata in a local SQLite database.

use crate::adb::AdbClient;
use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A captured screenshot entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenCapture {
    pub id: i64,
    pub timestamp: i64,          // Unix timestamp in ms
    pub file_path: String,       // Local path to the screenshot
    pub device_serial: String,
    pub device_model: Option<String>,
    pub note: Option<String>,    // Optional note/label
    pub file_size: i64,
    pub dark_frame: bool,        // True if the screen was locked/off
}

/// Screen capture database manager.
pub struct ScreenCaptureDB {
    db_path: PathBuf,
    capture_dir: PathBuf,
}

impl ScreenCaptureDB {
    /// Create a new screen capture database.
    pub fn new(base_dir: &Path) -> Result<Self> {
        let db_path = base_dir.join("screen_captures.db");
        let capture_dir = base_dir.join("screenshots");
        std::fs::create_dir_all(&capture_dir)?;
        
        let db = Connection::open(&db_path)?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS captures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                file_path TEXT NOT NULL,
                device_serial TEXT NOT NULL,
                device_model TEXT,
                note TEXT,
                file_size INTEGER DEFAULT 0,
                dark_frame INTEGER DEFAULT 0
            )",
            [],
        )?;
        
        Ok(Self { db_path, capture_dir })
    }
    
    /// Save a new screenshot capture.
    pub fn save_capture(
        &self,
        serial: &str,
        model: Option<&str>,
        image_data: &[u8],
        note: Option<&str>,
    ) -> Result<ScreenCapture> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        
        // Generate filename: serial_timestamp.png
        let safe_serial = serial.replace(|c: char| !c.is_alphanumeric(), "_");
        let filename = format!("{}_{}.png", safe_serial, now);
        let file_path = self.capture_dir.join(&filename);
        
        // Write the image
        std::fs::write(&file_path, image_data)?;
        let file_size = image_data.len() as i64;
        let dark_frame = image_data.len() < 60_000;
        
        // Save to database
        let db = Connection::open(&self.db_path)?;
        db.execute(
            "INSERT INTO captures (timestamp, file_path, device_serial, device_model, note, file_size, dark_frame)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![now, file_path.display().to_string(), serial, model, note, file_size, dark_frame as i32],
        )?;
        
        let id = db.last_insert_rowid();
        
        Ok(ScreenCapture {
            id,
            timestamp: now,
            file_path: file_path.display().to_string(),
            device_serial: serial.to_string(),
            device_model: model.map(|s| s.to_string()),
            note: note.map(|s| s.to_string()),
            file_size,
            dark_frame,
        })
    }
    
    /// Get all captures, optionally filtered by device serial.
    pub fn get_captures(&self, serial: Option<&str>, limit: usize) -> Result<Vec<ScreenCapture>> {
        let db = Connection::open(&self.db_path)?;
        
        let captures = if let Some(s) = serial {
            let mut stmt = db.prepare(
                "SELECT id, timestamp, file_path, device_serial, device_model, note, file_size, dark_frame
                 FROM captures WHERE device_serial = ?1 ORDER BY timestamp DESC LIMIT ?2"
            )?;
            let rows = stmt.query_map(params![s, limit as i64], |row| {
                Ok(ScreenCapture {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    file_path: row.get(2)?,
                    device_serial: row.get(3)?,
                    device_model: row.get(4)?,
                    note: row.get(5)?,
                    file_size: row.get(6)?,
                    dark_frame: row.get::<_, i32>(7).unwrap_or(0) != 0,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let mut stmt = db.prepare(
                "SELECT id, timestamp, file_path, device_serial, device_model, note, file_size, dark_frame
                 FROM captures ORDER BY timestamp DESC LIMIT ?1"
            )?;
            let rows = stmt.query_map(params![limit as i64], |row| {
                Ok(ScreenCapture {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    file_path: row.get(2)?,
                    device_serial: row.get(3)?,
                    device_model: row.get(4)?,
                    note: row.get(5)?,
                    file_size: row.get(6)?,
                    dark_frame: row.get::<_, i32>(7).unwrap_or(0) != 0,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        
        Ok(captures)
    }
    
    /// Delete a capture by ID (also removes the file).
    pub fn delete_capture(&self, id: i64) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        
        // Get file path first
        let mut stmt = db.prepare("SELECT file_path FROM captures WHERE id = ?1")?;
        let file_path: Option<String> = stmt
            .query_row(params![id], |row| row.get(0))
            .ok();
        
        // Delete from database
        db.execute("DELETE FROM captures WHERE id = ?1", params![id])?;
        
        // Delete file if it exists
        if let Some(path) = file_path {
            let _ = std::fs::remove_file(&path);
        }
        
        Ok(())
    }
    
    /// Get total count of captures.
    pub fn count(&self, serial: Option<&str>) -> Result<i64> {
        let db = Connection::open(&self.db_path)?;
        let count = if let Some(s) = serial {
            db.query_row(
                "SELECT COUNT(*) FROM captures WHERE device_serial = ?1",
                params![s],
                |row| row.get(0),
            )?
        } else {
            db.query_row("SELECT COUNT(*) FROM captures", [], |row| row.get(0))?
        };
        Ok(count)
    }
}

/// Capture a screenshot from the device and save it.
/// Wakes the display first (keyevent 224) so locked devices don't produce black frames.
pub fn capture_and_save(
    adb: &AdbClient,
    serial: &str,
    model: Option<&str>,
    db: &ScreenCaptureDB,
    note: Option<&str>,
) -> Result<ScreenCapture> {
    // Wake the display so we don't capture a black/locked frame
    let _ = adb.exec_out(serial, &["sh", "-c", "input keyevent 224"]);
    std::thread::sleep(std::time::Duration::from_millis(450));
    
    // Capture screenshot via ADB
    let output = adb.exec_out(serial, &["sh", "-c", "screencap -p"])?;
    
    if output.is_empty() {
        return Err(crate::error::CoreError::Adb("Screenshot capture returned empty data".into()));
    }
    
    // Save to database
    db.save_capture(serial, model, &output, note)
}

/// State carried between auto-capture cycles so the gate can tell whether
/// the screen changed in a meaningful way.
#[derive(Default)]
pub struct CaptureState {
    last_hash: Option<[u8; 32]>,
    last_activity: Option<String>,
}

impl CaptureState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Read the current foreground app/activity (read-only `dumpsys`, no root).
/// Returns a component string like `com.whatsapp/.HomeActivity`.
pub fn get_foreground_activity(adb: &AdbClient, serial: &str) -> Option<String> {
    let cmds = [
        "dumpsys window | grep -E 'mCurrentFocus|mFocusedApp'",
        "dumpsys activity activities | grep -E 'mResumedActivity|topResumedActivity|ResumedActivity'",
    ];
    for cmd in cmds {
        if let Ok(out) = adb.exec_out(serial, &["sh", "-c", cmd]) {
            let text = String::from_utf8_lossy(&out);
            if let Some(comp) = parse_focus_component(&text) {
                return Some(comp);
            }
        }
    }
    None
}

/// Extract the `package/activity` token from a dumpsys focus line.
fn parse_focus_component(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.contains("mCurrentFocus=")
            || line.contains("mFocusedApp=")
            || line.contains("mResumedActivity:")
            || line.contains("topResumedActivity=")
            || line.contains("ResumedActivity:")
        {
            for tok in line.split_whitespace() {
                let t = tok.trim_end_matches('}').trim_end_matches(',');
                // The component token contains '/' and is not a Window{...}/ActivityRecord{...} handle
                if t.contains('/') && !t.contains('{') && !t.contains("u0") {
                    return Some(t.to_string());
                }
            }
        }
    }
    None
}

/// Package portion of a component string (`com.whatsapp/.Home` -> `com.whatsapp`).
fn package_of(component: &str) -> &str {
    component.split('/').next().unwrap_or(component)
}

/// Smart capture gate — saves only *meaningful* screens.
///
/// Combines three strategies:
/// - (#4) skip dark/locked frames (PNG < 60 KB) so black screens are never stored;
/// - (#2) read the foreground app/activity (cheap `dumpsys`) and label the shot;
/// - (#1) SHA-256 the PNG bytes for exact-frame dedup (`screencap -p` is deterministic).
///
/// A new capture is stored only when the foreground app changed **or** the pixels
/// changed. Identical static screens (same app + same bytes) are skipped.
/// Returns `Ok(Some(capture))` when saved, `Ok(None)` when skipped.
pub fn capture_smart(
    adb: &AdbClient,
    serial: &str,
    model: Option<&str>,
    db: &ScreenCaptureDB,
    state: &mut CaptureState,
) -> Result<Option<ScreenCapture>> {
    // #2 — cheap foreground check first
    let activity = get_foreground_activity(adb, serial);

    // Wake the display, then capture the frame
    let _ = adb.exec_out(serial, &["sh", "-c", "input keyevent 224"]);
    std::thread::sleep(std::time::Duration::from_millis(450));
    let png = adb.exec_out(serial, &["sh", "-c", "screencap -p"])?;
    if png.is_empty() {
        return Err(crate::error::CoreError::Adb("Screenshot capture returned empty data".into()));
    }

    // #4 — never store dark/locked frames
    if png.len() < 60_000 {
        return Ok(None);
    }

    // #1 — exact-frame hash
    let mut hasher = Sha256::new();
    hasher.update(&png);
    let hash: [u8; 32] = hasher.finalize().into();

    let activity_changed = activity != state.last_activity;
    let frame_changed = Some(hash) != state.last_hash;

    // Skip only when nothing meaningful changed (same app AND identical pixels)
    if !activity_changed && !frame_changed {
        return Ok(None);
    }

    // Save, labeled with the foreground app so each shot is meaningful
    let label = activity
        .as_ref()
        .map(|a| format!("auto · {}", package_of(a)))
        .unwrap_or_else(|| "auto-capture".to_string());
    let capture = db.save_capture(serial, model, &png, Some(&label))?;

    state.last_hash = Some(hash);
    state.last_activity = activity;

    Ok(Some(capture))
}
