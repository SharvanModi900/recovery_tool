//! Direct acquisition of Android system data via content providers (read-only).
//! Uses `adb shell content query` to read call logs, SMS, contacts, browser history, calendar events, and call recordings without root.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// A single call log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallLog {
    pub id: i64,
    pub number: String,
    pub name: Option<String>,
    pub call_type: String, // "incoming", "outgoing", "missed"
    pub duration: i64,     // seconds
    pub timestamp: i64,    // Unix timestamp (ms)
    pub source: String,    // "live"
}

/// A call recording file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecording {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub timestamp: i64,
    pub source: String, // "live"
}

/// A single SMS/MMS message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsMessage {
    pub id: i64,
    pub address: String,   // phone number
    pub body: Option<String>,
    pub msg_type: String,  // "inbox", "sent", "draft", "outbox"
    pub timestamp: i64,    // Unix timestamp (ms)
    pub read: bool,
    pub source: String,    // "live"
}

/// A contact entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactEntry {
    pub id: i64,
    pub display_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub source: String, // "live"
}

/// A browser history entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserHistory {
    pub id: i64,
    pub url: String,
    pub title: Option<String>,
    pub timestamp: i64,
    pub source: String, // "live"
}

/// A calendar event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: i64,
    pub title: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    pub start_time: i64,
    pub end_time: i64,
    pub source: String, // "live"
}

/// Result of direct acquisition from Android content providers.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DirectAcquisition {
    pub call_logs: Vec<CallLog>,
    pub call_recordings: Vec<CallRecording>,
    pub sms_messages: Vec<SmsMessage>,
    pub contacts: Vec<ContactEntry>,
    pub browser_history: Vec<BrowserHistory>,
    pub calendar_events: Vec<CalendarEvent>,
    pub notes: Vec<String>,
}

/// Acquire call logs from the device (read-only).
pub fn acquire_call_logs(adb: &AdbClient, serial: &str) -> Result<Vec<CallLog>> {
    // Query content://call_log/calls
    let cmd = format!(
        "content query --uri content://call_log/calls --projection _id:number:name:type:duration:date"
    );
    let output = adb.exec_out(serial, &["sh", "-c", &cmd])?;
    let text = String::from_utf8_lossy(&output);

    let mut logs = Vec::new();
    for line in text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        // Parse: Row: 0 _id=1, number=+1234567890, name=John, type=1, duration=120, date=1234567890000
        let mut entry = CallLog {
            id: 0,
            number: String::new(),
            name: None,
            call_type: String::new(),
            duration: 0,
            timestamp: 0,
            source: "live".to_string(),
        };

        // Strip "Row: N " prefix from the first field
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);

        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "_id" => entry.id = val.parse().unwrap_or(0),
                    "number" => entry.number = val.to_string(),
                    "name" => entry.name = if val == "null" { None } else { Some(val.to_string()) },
                    "type" => {
                        entry.call_type = match val {
                            "1" => "incoming",
                            "2" => "outgoing",
                            "3" => "missed",
                            _ => "unknown",
                        }.to_string();
                    }
                    "duration" => entry.duration = val.parse().unwrap_or(0),
                    "date" => entry.timestamp = val.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if !entry.number.is_empty() {
            logs.push(entry);
        }
    }

    Ok(logs)
}

/// Acquire SMS/MMS messages from the device (read-only).
pub fn acquire_sms(adb: &AdbClient, serial: &str) -> Result<Vec<SmsMessage>> {
    // Query content://sms
    let cmd = format!(
        "content query --uri content://sms --projection _id:address:body:type:date:read"
    );
    let output = adb.exec_out(serial, &["sh", "-c", &cmd])?;
    let text = String::from_utf8_lossy(&output);

    let mut messages = Vec::new();
    for line in text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }

        let mut msg = SmsMessage {
            id: 0,
            address: String::new(),
            body: None,
            msg_type: String::new(),
            timestamp: 0,
            read: false,
            source: "live".to_string(),
        };

        // Strip "Row: N " prefix from the first field
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);

        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "_id" => msg.id = val.parse().unwrap_or(0),
                    "address" => msg.address = val.to_string(),
                    "body" => msg.body = if val == "null" { None } else { Some(val.to_string()) },
                    "type" => {
                        msg.msg_type = match val {
                            "1" => "inbox",
                            "2" => "sent",
                            "3" => "draft",
                            "4" => "outbox",
                            _ => "unknown",
                        }.to_string();
                    }
                    "date" => msg.timestamp = val.parse().unwrap_or(0),
                    "read" => msg.read = val == "1",
                    _ => {}
                }
            }
        }

        if !msg.address.is_empty() {
            messages.push(msg);
        }
    }

    Ok(messages)
}

/// Acquire contacts from the device (read-only).
/// Uses the correct Android content provider URIs for contacts + phone numbers.
pub fn acquire_contacts(adb: &AdbClient, serial: &str) -> Result<Vec<ContactEntry>> {
    // Step 1: Query all contacts from the correct URI
    // content://com.android.contacts/contacts is the standard URI on modern Android
    let cmd = "content query --uri content://com.android.contacts/contacts --projection _id:display_name";
    let output = adb.exec_out(serial, &["sh", "-c", cmd])?;
    let text = String::from_utf8_lossy(&output);

    // Parse contact IDs and names
    let mut contact_map: Vec<(i64, Option<String>)> = Vec::new();
    for line in text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        // Strip "Row: N " prefix — the row number is glued to the first key
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        // stripped is now like "0 _id=4278, display_name=vivo Customer Care"
        // Skip the row number (first token before space)
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);
        // after_num is now "_id=4278, display_name=vivo Customer Care"
        let mut id: i64 = 0;
        let mut name: Option<String> = None;
        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "_id" => id = val.parse().unwrap_or(0),
                    "display_name" => {
                        name = if val == "null" { None } else { Some(val.to_string()) }
                    }
                    _ => {}
                }
            }
        }
        if id > 0 {
            contact_map.push((id, name));
        }
    }

    // Step 2: Query ALL data rows and filter in Rust (avoids --where quoting issues on Android)
    let data_cmd = "content query --uri content://com.android.contacts/data --projection raw_contact_id:data1:mimetype";
    let data_output = adb.exec_out(serial, &["sh", "-c", data_cmd]).unwrap_or_default();
    let data_text = String::from_utf8_lossy(&data_output);

    // Build maps of raw_contact_id -> phone/email by filtering mimetype in Rust
    let mut phone_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut email_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for line in data_text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        // Strip "Row: N " prefix
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);
        let mut raw_id = String::new();
        let mut data1 = String::new();
        let mut mimetype = String::new();
        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "raw_contact_id" => raw_id = val.to_string(),
                    "data1" => {
                        if val != "null" && !val.is_empty() {
                            data1 = val.to_string();
                        }
                    }
                    "mimetype" => mimetype = val.to_string(),
                    _ => {}
                }
            }
        }
        if raw_id.is_empty() || data1.is_empty() {
            continue;
        }
        if mimetype == "vnd.android.cursor.item/phone_v2" {
            phone_map.entry(raw_id).or_insert(data1);
        } else if mimetype == "vnd.android.cursor.item/email_v2" {
            email_map.entry(raw_id).or_insert(data1);
        }
    }

    // Step 4: Build final contact list
    let mut contacts = Vec::new();
    for (id, display_name) in &contact_map {
        let id_str = id.to_string();
        let phone = phone_map.get(&id_str).cloned();
        let email = email_map.get(&id_str).cloned();

        if display_name.is_some() || phone.is_some() {
            contacts.push(ContactEntry {
                id: *id,
                display_name: display_name.clone(),
                phone,
                email,
                source: "live".to_string(),
            });
        }
    }

    Ok(contacts)
}

/// Acquire call recording files from common directories.
pub fn acquire_call_recordings(adb: &AdbClient, serial: &str) -> Result<Vec<CallRecording>> {
    // Common call recording directories on Android
    let search_dirs = vec![
        "/sdcard/CallRecorder",
        "/sdcard/recordings",
        "/sdcard/CallRecordings",
        "/sdcard/DCIM/CallRecorder",
        "/sdcard/Music/CallRecorder",
        "/sdcard/Android/data/com.android.dialer/files/CallRecorder",
    ];

    let mut recordings = Vec::new();

    for dir in search_dirs {
        // List files in directory with details
        let cmd = format!("ls -la {} 2>/dev/null | grep -E '\\.(mp3|m4a|wav|aac|ogg|amr)$'", dir);
        let output = adb.exec_out(serial, &["sh", "-c", &cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);

        for line in text.lines() {
            // Parse ls -la output: -rw-rw---- 1 root sdcard_rw 123456 2024-01-15 14:30 recording.mp3
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                let size = parts[4].parse::<i64>().unwrap_or(0);
                let date_str = parts[5]; // 2024-01-15
                let time_str = parts[6]; // 14:30
                let filename = parts[8..].join(" ");

                // Convert date/time to timestamp (simplified - just store as-is for now)
                let timestamp = 0i64; // TODO: Parse date properly

                recordings.push(CallRecording {
                    path: format!("{}/{}", dir, filename),
                    filename: filename.clone(),
                    size,
                    timestamp,
                    source: "live".to_string(),
                });
            }
        }
    }

    Ok(recordings)
}

/// Acquire browser history from Android content providers.
pub fn acquire_browser_history(adb: &AdbClient, serial: &str) -> Result<Vec<BrowserHistory>> {
    // Try Chrome browser history first
    let cmd = "content query --uri content://com.android.chrome/browser/history --projection _id:url:title:date";
    let output = adb.exec_out(serial, &["sh", "-c", cmd]).unwrap_or_default();
    let text = String::from_utf8_lossy(&output);

    let mut history = Vec::new();

    // If Chrome history is empty, try default browser
    if !text.lines().any(|l| l.starts_with("Row:")) {
        let cmd = "content query --uri content://browser/bookmarks --projection _id:url:title:date --where \"bookmark=0\"";
        let output = adb.exec_out(serial, &["sh", "-c", cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);

        for line in text.lines() {
            if !line.starts_with("Row:") {
                continue;
            }
            let stripped = line.strip_prefix("Row: ").unwrap_or(line);
            let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);

            let mut entry = BrowserHistory {
                id: 0,
                url: String::new(),
                title: None,
                timestamp: 0,
                source: "live".to_string(),
            };

            for part in after_num.split(", ") {
                if let Some((key, val)) = part.split_once('=') {
                    let val = val.trim_matches('"');
                    match key.trim() {
                        "_id" => entry.id = val.parse().unwrap_or(0),
                        "url" => entry.url = val.to_string(),
                        "title" => entry.title = if val == "null" { None } else { Some(val.to_string()) },
                        "date" => entry.timestamp = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            if !entry.url.is_empty() {
                history.push(entry);
            }
        }
    } else {
        // Parse Chrome history
        for line in text.lines() {
            if !line.starts_with("Row:") {
                continue;
            }
            let stripped = line.strip_prefix("Row: ").unwrap_or(line);
            let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);

            let mut entry = BrowserHistory {
                id: 0,
                url: String::new(),
                title: None,
                timestamp: 0,
                source: "live".to_string(),
            };

            for part in after_num.split(", ") {
                if let Some((key, val)) = part.split_once('=') {
                    let val = val.trim_matches('"');
                    match key.trim() {
                        "_id" => entry.id = val.parse().unwrap_or(0),
                        "url" => entry.url = val.to_string(),
                        "title" => entry.title = if val == "null" { None } else { Some(val.to_string()) },
                        "date" => entry.timestamp = val.parse().unwrap_or(0),
                        _ => {}
                    }
                }
            }

            if !entry.url.is_empty() {
                history.push(entry);
            }
        }
    }

    Ok(history)
}

/// Acquire calendar events from Android content providers.
pub fn acquire_calendar_events(adb: &AdbClient, serial: &str) -> Result<Vec<CalendarEvent>> {
    let cmd = "content query --uri content://com.android.calendar/events --projection _id:title:description:eventLocation:dtstart:dtend";
    let output = adb.exec_out(serial, &["sh", "-c", cmd]).unwrap_or_default();
    let text = String::from_utf8_lossy(&output);

    let mut events = Vec::new();
    for line in text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);

        let mut event = CalendarEvent {
            id: 0,
            title: None,
            description: None,
            location: None,
            start_time: 0,
            end_time: 0,
            source: "live".to_string(),
        };

        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "_id" => event.id = val.parse().unwrap_or(0),
                    "title" => event.title = if val == "null" { None } else { Some(val.to_string()) },
                    "description" => event.description = if val == "null" { None } else { Some(val.to_string()) },
                    "eventLocation" => event.location = if val == "null" { None } else { Some(val.to_string()) },
                    "dtstart" => event.start_time = val.parse().unwrap_or(0),
                    "dtend" => event.end_time = val.parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        if event.title.is_some() || event.id > 0 {
            events.push(event);
        }
    }

    Ok(events)
}

/// Acquire all available data from Android content providers.
pub fn acquire_all(adb: &AdbClient, serial: &str) -> Result<DirectAcquisition> {
    let mut result = DirectAcquisition::default();

    match acquire_call_logs(adb, serial) {
        Ok(logs) => result.call_logs = logs,
        Err(e) => result.notes.push(format!("Call logs acquisition failed: {}", e)),
    }

    match acquire_call_recordings(adb, serial) {
        Ok(recordings) => result.call_recordings = recordings,
        Err(e) => result.notes.push(format!("Call recordings acquisition failed: {}", e)),
    }

    match acquire_sms(adb, serial) {
        Ok(msgs) => result.sms_messages = msgs,
        Err(e) => result.notes.push(format!("SMS acquisition failed: {}", e)),
    }

    match acquire_contacts(adb, serial) {
        Ok(contacts) => result.contacts = contacts,
        Err(e) => result.notes.push(format!("Contacts acquisition failed: {}", e)),
    }

    match acquire_browser_history(adb, serial) {
        Ok(history) => result.browser_history = history,
        Err(e) => result.notes.push(format!("Browser history acquisition failed: {}", e)),
    }

    match acquire_calendar_events(adb, serial) {
        Ok(events) => result.calendar_events = events,
        Err(e) => result.notes.push(format!("Calendar events acquisition failed: {}", e)),
    }

    Ok(result)
}
