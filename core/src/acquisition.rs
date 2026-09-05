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
    /// Call-log entry this recording was matched to (by number in filename or
    /// by timestamp proximity to the call's end). Null when unmatched.
    pub matched_number: Option<String>,
    pub matched_name: Option<String>,
    pub matched_call_type: Option<String>,
    pub matched_timestamp: Option<i64>,
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

/// A WhatsApp media file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppMedia {
    pub path: String,
    pub filename: String,
    pub size: i64,
    pub media_type: String, // "image", "video", "audio", "document"
    pub source_folder: String, // e.g., ".Statuses", "WhatsApp Images"
    pub contact: Option<String>, // WhatsApp number/contact if available
    pub source: String,     // "live"
}

/// A WhatsApp chat entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppChat {
    pub jid: String,
    pub name: Option<String>,
    pub message_count: i64,
    pub last_message: Option<String>,
    pub last_timestamp: i64,
    pub source: String, // "live"
}

/// A WhatsApp call record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppCall {
    pub jid: String,
    pub name: Option<String>,
    pub call_type: String, // "incoming", "outgoing", "missed"
    pub duration: i64,
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
    pub whatsapp_media: Vec<WhatsAppMedia>,
    pub whatsapp_chats: Vec<WhatsAppChat>,
    pub whatsapp_calls: Vec<WhatsAppCall>,
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
    // Step 1: Query contacts with phone numbers and emails using the data URI
    // Use contact_id which matches contacts._id (not raw_contact_id)
    let cmd = "content query --uri content://com.android.contacts/data --projection contact_id:data1:data2:mimetype";
    let output = adb.exec_out(serial, &["sh", "-c", cmd])?;
    let text = String::from_utf8_lossy(&output);

    // Build maps: contact_id -> phone, contact_id -> email
    let mut phone_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut email_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    
    for line in text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);
        let mut contact_id = String::new();
        let mut data1 = String::new();
        let mut mimetype = String::new();
        
        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "contact_id" => contact_id = val.to_string(),
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
        
        if contact_id.is_empty() {
            continue;
        }
        
        if mimetype == "vnd.android.cursor.item/phone_v2" && !data1.is_empty() {
            phone_map.entry(contact_id.clone()).or_insert(data1.clone());
        } else if mimetype == "vnd.android.cursor.item/email_v2" && !data1.is_empty() {
            email_map.entry(contact_id.clone()).or_insert_with(Vec::new).push(data1);
        } else if mimetype == "vnd.android.cursor.item/name" && !data1.is_empty() {
            name_map.entry(contact_id.clone()).or_insert(data1);
        }
    }

    // Step 2: Query display names from contacts table
    let name_cmd = "content query --uri content://com.android.contacts/contacts --projection _id:display_name";
    let name_output = adb.exec_out(serial, &["sh", "-c", name_cmd])?;
    let name_text = String::from_utf8_lossy(&name_output);
    
    let mut contact_list: Vec<(i64, Option<String>)> = Vec::new();
    for line in name_text.lines() {
        if !line.starts_with("Row:") {
            continue;
        }
        let stripped = line.strip_prefix("Row: ").unwrap_or(line);
        let after_num = stripped.split_once(' ').map(|(_, rest)| rest).unwrap_or(stripped);
        let mut id: i64 = 0;
        let mut display_name: Option<String> = None;
        
        for part in after_num.split(", ") {
            if let Some((key, val)) = part.split_once('=') {
                let val = val.trim_matches('"');
                match key.trim() {
                    "_id" => id = val.parse().unwrap_or(0),
                    "display_name" => {
                        display_name = if val == "null" { None } else { Some(val.to_string()) }
                    }
                    _ => {}
                }
            }
        }
        if id > 0 {
            contact_list.push((id, display_name));
        }
    }
    
    // Step 3: Try to get primary Google account email for contacts without email
    let primary_email = get_primary_account_email(adb, serial);

    // Step 4: Build final contact list with phone and email
    let mut contacts = Vec::new();
    for (id, display_name) in &contact_list {
        let id_str = id.to_string();
        let phone = phone_map.get(&id_str).cloned();
        
        // Get email from contact data, or use primary account email as fallback
        let email = email_map.get(&id_str)
            .and_then(|emails| emails.first().cloned())
            .or_else(|| primary_email.clone());
        
        // Use display_name from contacts table, or fallback to name from data table
        let final_name = display_name.clone().or_else(|| name_map.get(&id_str).cloned());

        if final_name.is_some() || phone.is_some() {
            contacts.push(ContactEntry {
                id: *id,
                display_name: final_name,
                phone,
                email,
                source: "live".to_string(),
            });
        }
    }

    Ok(contacts)
}

/// Get the primary Google account email from the device.
fn get_primary_account_email(adb: &AdbClient, serial: &str) -> Option<String> {
    // Method 1: Query accounts database
    let cmd = "content query --uri content://com.android.contacts/accounts --projection name";
    if let Ok(output) = adb.exec_out(serial, &["sh", "-c", cmd]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines() {
            if line.contains("@gmail.com") || line.contains("@google.com") {
                // Extract email from the row
                if let Some(idx) = line.find("name=") {
                    let val = &line[idx + 5..];
                    let email = val.split(',').next().unwrap_or(val).trim().trim_matches('"');
                    if email.contains('@') {
                        return Some(email.to_string());
                    }
                }
            }
        }
    }
    
    // Method 2: Use dumpsys to get accounts
    if let Ok(output) = adb.exec_out(serial, &["sh", "-c", "dumpsys account | grep -E 'Account{|name:' | head -20"]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines() {
            let line = line.trim();
            if (line.contains("@gmail.com") || line.contains("@google.com")) && !line.starts_with("Account {") {
                // This might be an account name line
                let email = line.trim().trim_matches('"').trim_matches(',');
                if email.contains('@') && !email.contains(' ') {
                    return Some(email.to_string());
                }
            }
        }
    }
    
    // Method 3: Get from settings
    if let Ok(output) = adb.exec_out(serial, &["sh", "-c", "settings get secure user_setup_complete"]) {
        // Device is set up, try to get the primary email
        if let Ok(output) = adb.exec_out(serial, &["sh", "-c", "pm list packages com.google.android.gsf"]) {
            let text = String::from_utf8_lossy(&output);
            if text.contains("gsf") {
                // Google Services Framework is installed, try to get account
                if let Ok(output) = adb.exec_out(serial, &["sh", "-c", "content query --uri content://com.google.android.gsf.gservices --projection name:value --where \"name='android_id'\""]) {
                    // We have GSF, but the email is in a different location
                    // Fall through to return None
                }
            }
        }
    }
    
    None
}

/// Acquire call recording files from a broad set of vendor directories plus a
/// bounded recursive `/sdcard` sweep, then match each file to a call-log entry.
///
/// `stat -c '%Y|%s|%n'` yields epoch mtime + size + path in one batch, so every
/// recording carries a real timestamp that can be tied to a number + date. A
/// fallback `ls -la` pass keeps older devices (no toybox `stat -c`) working.
pub fn acquire_call_recordings(adb: &AdbClient, serial: &str, logs: &[CallLog]) -> Result<Vec<CallRecording>> {
    // Curated call-recording roots across AOSP + major OEM skins.
    let search_dirs = [
        // Generic / AOSP dialer
        "/sdcard/CallRecorder",
        "/sdcard/CallRecordings",
        "/sdcard/PhoneRecordings",
        "/sdcard/recordings",
        "/sdcard/Recordings",
        "/sdcard/Record",
        "/sdcard/Record/call",
        "/sdcard/DCIM/CallRecorder",
        "/sdcard/Music/CallRecorder",
        "/sdcard/sounds",
        // Samsung
        "/sdcard/Android/data/com.samsung.android.dialer/files/CallRecorder",
        "/sdcard/Android/data/com.sec.android.app.voicenote/files",
        // Google / AOSP dialer
        "/sdcard/Android/data/com.android.dialer/files/CallRecorder",
        "/sdcard/Android/data/com.google.android.dialer/files/CallRecorder",
        // Xiaomi / MIUI
        "/sdcard/MIUI/sound_recorder",
        "/sdcard/MIUI/sound_recorder/call_rec",
        "/sdcard/MIUI/CallRecorder",
        "/sdcard/Android/data/com.android.soundrecorder/files",
        // Oppo / Realme (ColorOS)
        "/sdcard/Recordings/Call Recordings",
        "/sdcard/oppo_record",
        "/sdcard/Phone/CallRecordings",
        // Vivo (FuntouchOS)
        "/sdcard/vivo_record",
        "/sdcard/BBKRecord",
        // OnePlus / Huawei / Honor
        "/sdcard/oneplus_record",
        "/sdcard/Huawei/CallRecord",
        "/sdcard/Honor/CallRecord",
    ];

    let audio_filter = "-iname '*.mp3' -o -iname '*.m4a' -o -iname '*.wav' -o -iname '*.aac' -o -iname '*.ogg' -o -iname '*.amr' -o -iname '*.awb' -o -iname '*.3gp'";

    let mut recordings: Vec<CallRecording> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Primary: batched stat gives epoch mtime + size + path for every match.
    for dir in search_dirs {
        let cmd = format!(
            "find {} -type f \\( {} \\) -exec stat -c '%Y|%s|%n' {{}} + 2>/dev/null",
            dir, audio_filter
        );
        if let Ok(out) = adb.exec_out(serial, &["sh", "-c", &cmd]) {
            push_stat_lines(&String::from_utf8_lossy(&out), &mut recordings, &mut seen);
        }
    }

    // Recursive /sdcard sweep, restricted to call/recording-looking paths so
    // generic music is not swept in, but unknown OEM folders are still caught.
    let sweep = format!(
        "find /sdcard -type f \\( {} \\) \\( -ipath '*record*' -o -ipath '*call*' -o -ipath '*phone*' \\) -exec stat -c '%Y|%s|%n' {{}} + 2>/dev/null",
        audio_filter
    );
    if let Ok(out) = adb.exec_out(serial, &["sh", "-c", &sweep]) {
        push_stat_lines(&String::from_utf8_lossy(&out), &mut recordings, &mut seen);
    }

    // Fallback for devices without toybox `stat -c`: parse `ls -la` (no mtime).
    if recordings.is_empty() {
        for dir in search_dirs {
            let cmd = format!(
                "find {} -type f \\( {} \\) -exec ls -la {{}} + 2>/dev/null",
                dir, audio_filter
            );
            if let Ok(out) = adb.exec_out(serial, &["sh", "-c", &cmd]) {
                push_ls_lines(&String::from_utf8_lossy(&out), &mut recordings, &mut seen);
            }
        }
    }

    // Match each recording to a call-log entry (number in filename, else time).
    for rec in recordings.iter_mut() {
        if let Some(log) = match_recording_to_call(rec, logs) {
            rec.matched_number = Some(log.number.clone());
            rec.matched_name = log.name.clone();
            rec.matched_call_type = Some(log.call_type.clone());
            rec.matched_timestamp = Some(log.timestamp);
        }
    }

    recordings.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(recordings)
}

/// Parse `stat -c '%Y|%s|%n'` output lines into recordings (dedup by path).
fn push_stat_lines(text: &str, recordings: &mut Vec<CallRecording>, seen: &mut std::collections::HashSet<String>) {
    for line in text.lines() {
        if recordings.len() >= 1000 { break; }
        let mut it = line.splitn(3, '|');
        let (Some(m), Some(s), Some(p)) = (it.next(), it.next(), it.next()) else { continue };
        let mtime: i64 = m.trim().parse().unwrap_or(0);
        let size: i64 = s.trim().parse().unwrap_or(0);
        let path = p.trim().to_string();
        if path.is_empty() || !seen.insert(path.clone()) { continue; }
        let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
        recordings.push(CallRecording {
            path,
            filename,
            size,
            timestamp: mtime.saturating_mul(1000), // ms, aligned with CallLog
            source: "live".to_string(),
            matched_number: None,
            matched_name: None,
            matched_call_type: None,
            matched_timestamp: None,
        });
    }
}

/// Fallback parser for `ls -la` output lines (no reliable epoch timestamp).
fn push_ls_lines(text: &str, recordings: &mut Vec<CallRecording>, seen: &mut std::collections::HashSet<String>) {
    for line in text.lines() {
        if recordings.len() >= 1000 { break; }
        if !line.starts_with('-') { continue; }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 8 { continue; }
        let size = parts[4].parse::<i64>().unwrap_or(0);
        let path = parts[7..].join(" ");
        if path.is_empty() || !seen.insert(path.clone()) { continue; }
        let filename = path.rsplit('/').next().unwrap_or(&path).to_string();
        recordings.push(CallRecording {
            path,
            filename,
            size,
            timestamp: 0,
            source: "live".to_string(),
            matched_number: None,
            matched_name: None,
            matched_call_type: None,
            matched_timestamp: None,
        });
    }
}

/// Tie a recording to a call-log entry: first by a phone number embedded in the
/// filename, else by timestamp proximity to the call's end (±5 minutes).
fn match_recording_to_call<'a>(rec: &CallRecording, logs: &'a [CallLog]) -> Option<&'a CallLog> {
    let fname = rec.filename.to_lowercase();
    for log in logs {
        let digits: String = log.number.chars().filter(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 7 && fname.contains(&digits) {
            return Some(log);
        }
    }
    if rec.timestamp > 0 {
        let mut best: Option<(&CallLog, i64)> = None;
        for log in logs {
            if log.timestamp <= 0 { continue; }
            let call_end = log.timestamp + log.duration.saturating_mul(1000);
            let diff = (rec.timestamp - call_end).abs();
            if diff <= 300_000 && best.map_or(true, |(_, d)| diff < d) {
                best = Some((log, diff));
            }
        }
        if let Some((log, _)) = best {
            return Some(log);
        }
    }
    None
}

/// Acquire WhatsApp media files from device storage.
pub fn acquire_whatsapp_media(adb: &AdbClient, serial: &str) -> Result<Vec<WhatsAppMedia>> {
    let media_roots = vec![
        "/sdcard/Android/media/com.whatsapp/WhatsApp/Media",
        "/sdcard/WhatsApp/Media",
    ];

    // Try to pull and parse WhatsApp database for contact mapping
    let mut media_contacts: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut media_contacts_by_filename: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let db_paths = vec![
        "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db",
        "/sdcard/WhatsApp/Databases/msgstore.db",
    ];
    
    for db_path in db_paths {
        let check_cmd = format!("ls -la {} 2>/dev/null", db_path);
        let output = adb.exec_out(serial, &["sh", "-c", &check_cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);
        
        if text.contains("msgstore.db") {
            // Pull the database
            let temp_dir = std::path::PathBuf::from("D:\\RecoverKit_Temp");
            std::fs::create_dir_all(&temp_dir).ok();
            let local_db = temp_dir.join("msgstore.db");
            
            if adb.pull(serial, db_path, &local_db).is_ok() {
                // Parse the database to get media-to-contact mappings
                if let Ok(parse_result) = crate::sqlite::parse_msgstore(&local_db) {
                    for media_ref in &parse_result.media {
                        if let (Some(file_path), Some(contact_jid)) = (&media_ref.file_path, &media_ref.contact_jid) {
                            if !contact_jid.is_empty() {
                                // Store by full path (normalized)
                                let normalized = file_path.replace('\\', "/");
                                media_contacts.insert(normalized.clone(), contact_jid.clone());
                                
                                // Also store by filename for fallback matching
                                if let Some(filename) = file_path.rsplit('/').next().or_else(|| file_path.rsplit('\\').next()) {
                                    media_contacts_by_filename.insert(filename.to_string(), contact_jid.clone());
                                }
                                
                                // Also store by partial path (last 2 segments)
                                let parts: Vec<&str> = normalized.split('/').collect();
                                if parts.len() >= 2 {
                                    let partial = parts[parts.len()-2..].join("/");
                                    media_contacts.insert(partial, contact_jid.clone());
                                }
                            }
                        }
                    }
                }
            }
            break;
        }
    }

    let mut media_files = Vec::new();
    let mut seen_paths = std::collections::HashSet::new();

    for root in media_roots {
        // Use find + ls combo to get paths and sizes in one shot (no per-file stat)
        let cmd = format!(
            r"find {} -type f \( -iname '*.jpg' -o -iname '*.jpeg' -o -iname '*.png' -o -iname '*.gif' -o -iname '*.mp4' -o -iname '*.3gp' -o -iname '*.opus' -o -iname '*.ogg' -o -iname '*.pdf' -o -iname '*.doc' -o -iname '*.docx' \) -exec ls -la {{}} + 2>/dev/null",
            root
        );
        let output = adb.exec_out(serial, &["sh", "-c", &cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);

        for line in text.lines() {
            if !line.starts_with('-') {
                continue;
            }
            // Parse: -rw-rw---- 1 u0_a0 media_rw 227788 2025-06-17 11:44 filename
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 8 {
                continue;
            }
            let size = parts[4].parse::<i64>().unwrap_or(0);
            let full_path = parts[7..].join(" ");

            if full_path.is_empty() || seen_paths.contains(&full_path) {
                continue;
            }
            seen_paths.insert(full_path.clone());

            // Determine media type from path
            let lower = full_path.to_lowercase();
            let media_type = if lower.contains("/whatsapp images/") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".png") || lower.ends_with(".gif") {
                "image"
            } else if lower.contains("/whatsapp video/") || lower.ends_with(".mp4") || lower.ends_with(".3gp") {
                "video"
            } else if lower.contains("/whatsapp audio/") || lower.ends_with(".opus") || lower.ends_with(".ogg") {
                "audio"
            } else if lower.contains("/whatsapp documents/") {
                "document"
            } else {
                "other"
            };

            let filename = full_path.rsplit('/').next().unwrap_or(&full_path).to_string();

            // Extract source folder (e.g., ".Statuses", "WhatsApp Images")
            let source_folder = full_path
                .rsplit('/')
                .nth(1)
                .unwrap_or("Unknown")
                .to_string();

            // Try to match with database contact using multiple strategies
            let contact = media_contacts.get(&full_path).cloned()
                .or_else(|| {
                    // Try partial path match (folder/filename)
                    let parts: Vec<&str> = full_path.split('/').collect();
                    if parts.len() >= 2 {
                        let partial = parts[parts.len()-2..].join("/");
                        media_contacts.get(&partial).cloned()
                    } else {
                        None
                    }
                })
                .or_else(|| {
                    // Try filename-only match
                    media_contacts_by_filename.get(&filename).cloned()
                });

            media_files.push(WhatsAppMedia {
                path: full_path,
                filename,
                size,
                media_type: media_type.to_string(),
                source_folder,
                contact,
                source: "live".to_string(),
            });
        }
    }

    Ok(media_files)
}

/// Acquire WhatsApp chat information from device.
pub fn acquire_whatsapp_chats(adb: &AdbClient, serial: &str) -> Result<Vec<WhatsAppChat>> {
    // Check if WhatsApp databases exist
    let db_paths = vec![
        "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db",
        "/sdcard/WhatsApp/Databases/msgstore.db",
    ];

    let mut chats = Vec::new();

    for db_path in db_paths {
        let check_cmd = format!("ls -la {} 2>/dev/null", db_path);
        let output = adb.exec_out(serial, &["sh", "-c", &check_cmd]).unwrap_or_default();
        let text = String::from_utf8_lossy(&output);

        if text.contains("msgstore.db") {
            // Database exists - try to query chat list via content provider or parse database
            // For now, we'll scan the WhatsApp media directories to infer active chats
            let media_cmd = "ls -la /sdcard/Android/media/com.whatsapp/WhatsApp/Media/ 2>/dev/null";
            let media_output = adb.exec_out(serial, &["sh", "-c", media_cmd]).unwrap_or_default();
            let media_text = String::from_utf8_lossy(&media_output);

            // Add a placeholder entry indicating WhatsApp is installed
            chats.push(WhatsAppChat {
                jid: "whatsapp_installed".to_string(),
                name: Some("WhatsApp Database Found".to_string()),
                message_count: 0,
                last_message: Some("Database available for deep analysis".to_string()),
                last_timestamp: 0,
                source: "live".to_string(),
            });
            break;
        }
    }

    Ok(chats)
}

/// Acquire WhatsApp call logs from device.
pub fn acquire_whatsapp_calls(adb: &AdbClient, serial: &str) -> Result<Vec<WhatsAppCall>> {
    // WhatsApp calls are stored in the main database - for now return empty
    // Deep analysis would require parsing msgstore.db
    Ok(Vec::new())
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

    let recs = acquire_call_recordings(adb, serial, &result.call_logs);
    match recs {
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

    match acquire_whatsapp_media(adb, serial) {
        Ok(media) => result.whatsapp_media = media,
        Err(e) => result.notes.push(format!("WhatsApp media acquisition failed: {}", e)),
    }

    match acquire_whatsapp_chats(adb, serial) {
        Ok(chats) => result.whatsapp_chats = chats,
        Err(e) => result.notes.push(format!("WhatsApp chats acquisition failed: {}", e)),
    }

    match acquire_whatsapp_calls(adb, serial) {
        Ok(calls) => result.whatsapp_calls = calls,
        Err(e) => result.notes.push(format!("WhatsApp calls acquisition failed: {}", e)),
    }

    match acquire_calendar_events(adb, serial) {
        Ok(events) => result.calendar_events = events,
        Err(e) => result.notes.push(format!("Calendar events acquisition failed: {}", e)),
    }

    Ok(result)
}

/// Result of automatic WhatsApp deep extraction.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct WhatsAppDeepResult {
    pub key_obtained: bool,
    pub key_method: Option<String>,
    pub database_decrypted: bool,
    pub decrypted_db_path: Option<String>,
    pub crypt_file_path: Option<String>,
    pub crypt_format: Option<String>,
    pub chats: Vec<WhatsAppChat>,
    pub media_with_contacts: Vec<WhatsAppMedia>,
    pub total_messages: i64,
    pub total_contacts: i64,
    pub notes: Vec<String>,
}

/// Automatic WhatsApp deep extraction - tries all methods to get the key and decrypt.
pub fn acquire_whatsapp_deep(adb: &AdbClient, serial: &str) -> Result<WhatsAppDeepResult> {
    let mut result = WhatsAppDeepResult::default();
    let temp_dir = std::path::PathBuf::from("D:\\RecoverKit_Temp");
    std::fs::create_dir_all(&temp_dir).ok();

    // Step 1: Try to obtain the key file using multiple methods
    let key_path = try_obtain_key(adb, serial, &temp_dir, &mut result);
    
    // Step 2: Pull the encrypted database (crypt14/crypt15)
    let crypt_path = pull_crypt_database(adb, serial, &temp_dir, &mut result);
    
    // Step 3: If we have both key and encrypted database, decrypt
    if let (Some(key), Some(crypt)) = (&key_path, &crypt_path) {
        let decrypted_path = temp_dir.join("msgstore_decrypted.db");
        match crate::crypto::decrypt_backup(crypt, key, &decrypted_path) {
            Ok(outcome) => {
                result.database_decrypted = true;
                result.decrypted_db_path = Some(decrypted_path.display().to_string());
                let format_str = outcome.format.clone();
                result.notes.push(format!(
                    "Decrypted {} using key_offset={}, iv_offset={}",
                    format_str, outcome.key_offset, outcome.iv_offset
                ));
                result.crypt_format = Some(outcome.format);
                
                // Step 4: Parse the decrypted database
                if let Ok(parse_result) = crate::sqlite::parse_msgstore(&decrypted_path) {
                    result.total_messages = parse_result.messages.len() as i64;
                    
                    // Extract chats from the parsed database
                    let mut chat_map: std::collections::HashMap<String, (i64, Option<String>, Option<String>, Option<i64>)> = std::collections::HashMap::new();
                    
                    for msg in &parse_result.messages {
                        let jid = msg.chat_jid.clone().unwrap_or_else(|| "unknown".to_string());
                        let entry = chat_map.entry(jid.clone()).or_insert_with(|| {
                            let chat_info = parse_result.chats.iter().find(|c| c.jid == jid);
                            (
                                0i64,
                                chat_info.map(|c| c.name.clone()).filter(|n| !n.is_empty()),
                                None,
                                None,
                            )
                        });
                        entry.0 += 1; // message count
                        let msg_ts = msg.timestamp.unwrap_or(0);
                        if msg_ts > entry.3.unwrap_or(0) {
                            entry.2 = msg.text.clone();
                            entry.3 = msg.timestamp;
                        }
                    }
                    
                    for (jid, (msg_count, name, last_msg, last_ts)) in chat_map {
                        result.chats.push(WhatsAppChat {
                            jid: jid.clone(),
                            name,
                            message_count: msg_count,
                            last_message: last_msg,
                            last_timestamp: last_ts.unwrap_or(0),
                            source: "decrypted".to_string(),
                        });
                    }
                    
                    // Sort chats by message count descending
                    result.chats.sort_by(|a, b| b.message_count.cmp(&a.message_count));
                    result.total_contacts = result.chats.len() as i64;
                    
                    // Build media-to-contact mapping
                    let mut media_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    let mut filename_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
                    
                    for media_ref in &parse_result.media {
                        if let (Some(file_path), Some(contact_jid)) = (&media_ref.file_path, &media_ref.contact_jid) {
                            if !contact_jid.is_empty() {
                                let normalized = file_path.replace('\\', "/");
                                media_map.insert(normalized.clone(), contact_jid.clone());
                                
                                if let Some(filename) = file_path.rsplit('/').next().or_else(|| file_path.rsplit('\\').next()) {
                                    filename_map.insert(filename.to_string(), contact_jid.clone());
                                }
                                
                                let parts: Vec<&str> = normalized.split('/').collect();
                                if parts.len() >= 2 {
                                    let partial = parts[parts.len()-2..].join("/");
                                    media_map.insert(partial, contact_jid.clone());
                                }
                            }
                        }
                    }
                    
                    // Now scan media files and match with contacts
                    let media_roots = vec![
                        "/sdcard/Android/media/com.whatsapp/WhatsApp/Media",
                        "/sdcard/WhatsApp/Media",
                    ];
                    
                    for root in media_roots {
                        let check_cmd = format!("ls -la {} 2>/dev/null", root);
                        if let Ok(output) = adb.exec_out(serial, &["sh", "-c", &check_cmd]) {
                            let text = String::from_utf8_lossy(&output);
                            if text.contains("WhatsApp") {
                                // Scan subdirectories
                                let subdirs = ["WhatsApp Images", "WhatsApp Video", "WhatsApp Audio", 
                                              "WhatsApp Documents", "WhatsApp Animated Gifs", ".Statuses"];
                                
                                for subdir in subdirs {
                                    let full_dir = format!("{}/{}", root, subdir);
                                    let scan_cmd = format!("find {} -type f 2>/dev/null", full_dir);
                                    if let Ok(files_output) = adb.exec_out(serial, &["sh", "-c", &scan_cmd]) {
                                        let files_text = String::from_utf8_lossy(&files_output);
                                        for line in files_text.lines() {
                                            let line = line.trim();
                                            if line.is_empty() { continue; }
                                            
                                            let filename = line.rsplit('/').next().unwrap_or(line).to_string();
                                            let media_type = match subdir {
                                                s if s.contains("Image") => "image",
                                                s if s.contains("Video") => "video",
                                                s if s.contains("Audio") => "audio",
                                                s if s.contains("Document") => "document",
                                                s if s.contains("Gif") => "gif",
                                                s if s.contains("Status") => "status",
                                                _ => "other",
                                            };
                                            
                                            // Try to match contact
                                            let contact = media_map.get(line).cloned()
                                                .or_else(|| {
                                                    let parts: Vec<&str> = line.split('/').collect();
                                                    if parts.len() >= 2 {
                                                        let partial = parts[parts.len()-2..].join("/");
                                                        media_map.get(&partial).cloned()
                                                    } else { None }
                                                })
                                                .or_else(|| filename_map.get(&filename).cloned());
                                            
                                            result.media_with_contacts.push(WhatsAppMedia {
                                                path: line.to_string(),
                                                filename,
                                                size: 0, // Would need additional stat call
                                                media_type: media_type.to_string(),
                                                source_folder: subdir.to_string(),
                                                contact,
                                                source: "decrypted".to_string(),
                                            });
                                        }
                                    }
                                }
                                break;
                            }
                        }
                    }
                    
                    result.notes.push(format!(
                        "Extracted {} messages, {} chats, {} media files",
                        result.total_messages, result.total_contacts, result.media_with_contacts.len()
                    ));
                }
            }
            Err(e) => {
                result.notes.push(format!("Decryption failed: {}", e));
            }
        }
    } else {
        if key_path.is_none() {
            result.notes.push("Key file not available - device requires root access to extract WhatsApp encryption key from /data/data/com.whatsapp/files/key".to_string());
        }
        if crypt_path.is_none() {
            result.notes.push("Encrypted database not found on device".to_string());
        }
    }
    
    Ok(result)
}

/// Try multiple methods to obtain the WhatsApp key file.
fn try_obtain_key(adb: &AdbClient, serial: &str, temp_dir: &std::path::Path, result: &mut WhatsAppDeepResult) -> Option<std::path::PathBuf> {
    let key_path = temp_dir.join("whatsapp.key");
    
    // Key file locations to try
    let key_locations = [
        "/data/data/com.whatsapp/files/key",
        "/data/data/com.whatsapp/database/key",
        "/data/user/0/com.whatsapp/files/key",
        "/data/user_de/0/com.whatsapp/files/key",
    ];
    
    // su command variants
    let su_prefixes: &[&[&str]] = &[
        &["su", "-c"],
        &["su", "0", "-c"],
        &["su", "root", "-c"],
    ];
    
    for loc in &key_locations {
        for prefix in su_prefixes {
            let cat_cmd = format!("cat {}", loc);
            let args: Vec<&str> = prefix.iter().copied().chain(std::iter::once(cat_cmd.as_str())).collect();
            
            if let Ok(bytes) = adb.exec_out(serial, &args) {
                if bytes.len() >= 128 {
                    std::fs::write(&key_path, &bytes).ok();
                    result.key_obtained = true;
                    result.key_method = Some(format!("root: {}", loc));
                    result.notes.push(format!("Key obtained from {} ({} bytes)", loc, bytes.len()));
                    return Some(key_path);
                }
            }
        }
    }
    
    // Method 3: Check if key already exists locally
    let local_key_paths = [
        temp_dir.join("whatsapp.key"),
        std::path::PathBuf::from("D:\\RecoverKit_Temp\\whatsapp.key"),
        std::path::PathBuf::from("evidence\\key"),
    ];
    
    for local in local_key_paths {
        if local.exists() {
            result.key_obtained = true;
            result.key_method = Some("local cache".to_string());
            result.notes.push("Using cached key file".to_string());
            return Some(local);
        }
    }
    
    None
}

/// Check if the device has root access and return detailed info.
pub fn check_root_access(adb: &AdbClient, serial: &str) -> RootCheckResult {
    let mut result = RootCheckResult::default();
    
    // Try different su variants
    let su_tests = [
        ("su -c id", "generic su"),
        ("su 0 id", "su 0"),
        ("su root id", "su root"),
    ];
    
    for (cmd, variant) in &su_tests {
        let args: Vec<&str> = cmd.split_whitespace().collect();
        if let Ok(output) = adb.exec_out(serial, &args) {
            let text = String::from_utf8_lossy(&output);
            if text.contains("uid=0") || text.contains("root") {
                result.has_root = true;
                result.root_method = Some(variant.to_string());
                result.details.push(format!("Root confirmed via {}", variant));
                
                // Check if we can access WhatsApp data
                let wa_check = "cat /data/data/com.whatsapp/files/key 2>/dev/null | wc -c";
                if let Ok(output) = adb.exec_out(serial, &["su", "-c", wa_check]) {
                    let text = String::from_utf8_lossy(&output).trim().to_string();
                    if let Ok(size) = text.parse::<i64>() {
                        result.can_read_whatsapp_key = size > 100;
                        if result.can_read_whatsapp_key {
                            result.details.push(format!("WhatsApp key accessible ({} bytes)", size));
                        }
                    }
                }
                break;
            }
        }
    }
    
    // Check for Magisk specifically
    if let Ok(output) = adb.exec_out(serial, &["sh", "-c", "which magisk 2>/dev/null || ls /sbin/.magisk 2>/dev/null"]) {
        let text = String::from_utf8_lossy(&output);
        if !text.is_empty() {
            result.details.push("Magisk detected".to_string());
            result.has_magisk = true;
        }
    }
    
    // Check Android version (affects available methods)
    if let Ok(output) = adb.exec_out(serial, &["shell", "getprop", "ro.build.version.sdk"]) {
        let text = String::from_utf8_lossy(&output).trim().to_string();
        result.android_sdk = text.parse().ok();
    }
    
    if !result.has_root {
        result.details.push("No root access detected".to_string());
        result.details.push("The WhatsApp encryption key requires root access to extract".to_string());
    }
    
    result
}

/// Root access check result.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RootCheckResult {
    pub has_root: bool,
    pub has_magisk: bool,
    pub root_method: Option<String>,
    pub can_read_whatsapp_key: bool,
    pub android_sdk: Option<i32>,
    pub details: Vec<String>,
}

/// Pull the encrypted WhatsApp database (crypt14/crypt15).
fn pull_crypt_database(adb: &AdbClient, serial: &str, temp_dir: &std::path::Path, result: &mut WhatsAppDeepResult) -> Option<std::path::PathBuf> {
    let crypt_paths = [
        "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db.crypt14",
        "/sdcard/WhatsApp/Databases/msgstore.db.crypt14",
        "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db.crypt15",
        "/sdcard/WhatsApp/Databases/msgstore.db.crypt15",
    ];
    
    for remote in crypt_paths {
        let check_cmd = format!("ls -la {} 2>/dev/null", remote);
        if let Ok(output) = adb.exec_out(serial, &["sh", "-c", &check_cmd]) {
            let text = String::from_utf8_lossy(&output);
            if text.contains("crypt") {
                let filename = remote.rsplit('/').next().unwrap_or("msgstore.db.crypt14");
                let local_path = temp_dir.join(filename);
                
                if adb.pull(serial, remote, &local_path).is_ok() {
                    result.crypt_file_path = Some(local_path.display().to_string());
                    result.notes.push(format!("Pulled {} from {}", filename, remote));
                    return Some(local_path);
                }
            }
        }
    }
    
    None
}
