//! Real-time call monitoring via ADB.
//! 
//! Monitors incoming and outgoing calls on a connected Android device
//! by polling the telephony registry and parsing logcat events.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// A call event detected on the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallEvent {
    pub timestamp: i64,          // Unix timestamp in ms
    pub event_type: CallType,    // incoming, outgoing, missed, ended
    pub phone_number: Option<String>,
    pub contact_name: Option<String>,
    pub duration: Option<i64>,   // seconds (for ended calls)
    pub sim_slot: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CallType {
    Incoming,
    Outgoing,
    Missed,
    Ended,
    Rejected,
    Active,
    Idle,
    Unknown,
}

impl std::fmt::Display for CallType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallType::Incoming => write!(f, "Incoming"),
            CallType::Outgoing => write!(f, "Outgoing"),
            CallType::Missed => write!(f, "Missed"),
            CallType::Ended => write!(f, "Ended"),
            CallType::Rejected => write!(f, "Rejected"),
            CallType::Active => write!(f, "Active"),
            CallType::Idle => write!(f, "Idle"),
            CallType::Unknown => write!(f, "Unknown"),
        }
    }
}

/// Shared state for call monitoring.
#[derive(Debug, Default, Clone)]
pub struct CallMonitorState {
    pub events: Vec<CallEvent>,
    pub is_monitoring: bool,
    pub last_call_state: Option<CallType>,
}

/// Poll the device for current call state using call log (works without root).
pub fn poll_call_state(adb: &AdbClient, serial: &str) -> Result<Option<CallEvent>> {
    // Use call log content provider - this works without special permissions
    // Get the most recent call log entry
    let cmd = "content query --uri content://call_log/calls --projection _id:number:name:type:duration:date --sort \"date DESC LIMIT 1\"";
    let output = adb.exec_out(serial, &["sh", "-c", cmd])?;
    let text = String::from_utf8_lossy(&output);
    
    let mut number: Option<String> = None;
    let mut name: Option<String> = None;
    let mut call_type: Option<i32> = None;
    let mut duration: Option<i64> = None;
    let mut timestamp: Option<i64> = None;
    let mut id: Option<i64> = None;
    
    for line in text.lines() {
        if !line.contains("Row") { continue; }
        
        for part in line.split(',') {
            let part = part.trim();
            if part.starts_with("number=") {
                number = Some(part[7..].trim_matches(|c| c == '"' || c == ' ').to_string());
            } else if part.starts_with("name=") {
                let n = part[5..].trim_matches(|c| c == '"' || c == ' ').to_string();
                if !n.is_empty() && n != "NULL" && n != "null" {
                    name = Some(n);
                }
            } else if part.starts_with("type=") {
                call_type = part[5..].trim().parse().ok();
            } else if part.starts_with("duration=") {
                duration = part[9..].trim().parse().ok();
            } else if part.starts_with("date=") {
                timestamp = part[5..].trim().parse().ok();
            } else if part.starts_with("_id=") {
                id = part[4..].trim().parse().ok();
            }
        }
    }
    
    // Only return if we have a valid recent call (within last 10 seconds for "live" detection)
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    
    if let Some(ts) = timestamp {
        // Check if this call was very recent (within 30 seconds) - indicates active/recent call
        let age_seconds = (now - ts) / 1000;
        
        if age_seconds < 30 {
            let event_type = match call_type {
                Some(1) => CallType::Incoming,
                Some(2) => CallType::Outgoing,
                Some(3) => CallType::Missed,
                Some(4) => CallType::Rejected,
                Some(5) => CallType::Ended,
                _ => CallType::Unknown,
            };
            
            if event_type != CallType::Unknown {
                return Ok(Some(CallEvent {
                    timestamp: ts,
                    event_type,
                    phone_number: number,
                    contact_name: name,
                    duration,
                    sim_slot: None,
                }));
            }
        }
    }
    
    Ok(None)
}

/// Check for recent call log entries (missed calls, ended calls).
pub fn check_recent_calls(adb: &AdbClient, serial: &str, since_ms: i64) -> Result<Vec<CallEvent>> {
    let cmd = format!(
        "content query --uri content://call_log/calls --projection _id:number:name:type:duration:date --where \"date > {}\" --sort \"date DESC\"",
        since_ms
    );
    let output = adb.exec_out(serial, &["sh", "-c", &cmd])?;
    let text = String::from_utf8_lossy(&output);
    
    let mut events = Vec::new();
    
    for line in text.lines() {
        if !line.contains("Row ") { continue; }
        
        let mut number: Option<String> = None;
        let mut name: Option<String> = None;
        let mut call_type: Option<i32> = None;
        let mut duration: Option<i64> = None;
        let mut timestamp: Option<i64> = None;
        
        // Parse fields from the row
        for part in line.split(',') {
            let part = part.trim();
            if part.starts_with("number=") {
                number = Some(part[7..].trim_matches(|c| c == '"' || c == ' ').to_string());
            } else if part.starts_with("name=") {
                let n = part[5..].trim_matches(|c| c == '"' || c == ' ').to_string();
                if !n.is_empty() && n != "NULL" {
                    name = Some(n);
                }
            } else if part.starts_with("type=") {
                call_type = part[5..].trim().parse().ok();
            } else if part.starts_with("duration=") {
                duration = part[9..].trim().parse().ok();
            } else if part.starts_with("date=") {
                timestamp = part[5..].trim().parse().ok();
            }
        }
        
        let event_type = match call_type {
            Some(1) => CallType::Incoming,
            Some(2) => CallType::Outgoing,
            Some(3) => CallType::Missed,
            Some(4) => CallType::Rejected,
            Some(5) => CallType::Ended,
            _ => CallType::Unknown,
        };
        
        if let Some(ts) = timestamp {
            events.push(CallEvent {
                timestamp: ts,
                event_type,
                phone_number: number,
                contact_name: name,
                duration,
                sim_slot: None,
            });
        }
    }
    
    Ok(events)
}

/// Get notification about current call state with formatted message.
pub fn get_call_notification(adb: &AdbClient, serial: &str) -> Result<Option<String>> {
    match poll_live_call(adb, serial)? {
        Some(event) => {
            let msg = match &event.event_type {
                CallType::Incoming => {
                    format!(
                        "📞 INCOMING CALL{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" from {}", n))
                            .or_else(|| event.contact_name.as_ref().map(|n| format!(" from {}", n)))
                            .unwrap_or_default()
                    )
                }
                CallType::Outgoing => {
                    format!(
                        "📱 OUTGOING CALL{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" to {}", n))
                            .or_else(|| event.contact_name.as_ref().map(|n| format!(" to {}", n)))
                            .unwrap_or_default()
                    )
                }
                CallType::Missed => {
                    format!(
                        "❌ MISSED CALL{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" from {}", n))
                            .or_else(|| event.contact_name.as_ref().map(|n| format!(" from {}", n)))
                            .unwrap_or_default()
                    )
                }
                CallType::Ended => {
                    format!(
                        "📞 CALL ENDED{}{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" with {}", n))
                            .or_else(|| event.contact_name.as_ref().map(|n| format!(" with {}", n)))
                            .unwrap_or_default(),
                        event.duration.map(|d| format!(" ({}s)", d)).unwrap_or_default()
                    )
                }
                CallType::Active => {
                    format!(
                        "🟢 CALL ACTIVE{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" with {}", n))
                            .or_else(|| event.contact_name.as_ref().map(|n| format!(" with {}", n)))
                            .unwrap_or_default()
                    )
                }
                _ => return Ok(None),
            };
            Ok(Some(msg))
        }
        None => Ok(None),
    }
}

/// Real-time call state combining `dumpsys telecom` (RINGING / DIALING / ACTIVE,
/// readable by the shell UID without READ_PHONE_STATE) with the call-log content
/// provider. Telecom catches the ringing/dialing *moment*; the call log fills in
/// ENDED / MISSED entries that only exist after the call is over.
pub fn poll_live_call(adb: &AdbClient, serial: &str) -> Result<Option<CallEvent>> {
    // 1) Live state from telecom (ringing / dialing / active) — the earliest signal.
    if let Some(event) = poll_telecom_state(adb, serial)? {
        return Ok(Some(event));
    }
    // 2) Fall back to the call log for very recent ENDED / MISSED entries.
    poll_call_state(adb, serial)
}

/// Parse `dumpsys telecom` for the current live call state + number.
fn poll_telecom_state(adb: &AdbClient, serial: &str) -> Result<Option<CallEvent>> {
    let output = adb.exec_out(serial, &["sh", "-c", "dumpsys telecom"])?;
    let text = String::from_utf8_lossy(&output);

    let Some(event_type) = parse_telecom_call_state(&text) else {
        return Ok(None);
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    Ok(Some(CallEvent {
        timestamp: now,
        event_type,
        phone_number: extract_tel_number(&text),
        contact_name: None,
        duration: None,
        sim_slot: None,
    }))
}

/// Map a telecom state *assignment* to a CallType. Only concrete in-call states
/// are returned; IDLE / DISCONNECTED / legend text yields None. The value token
/// is matched exactly (after `=`/`:`) so fields like `mIsActive` never leak in.
fn parse_telecom_call_state(text: &str) -> Option<CallType> {
    for line in text.lines() {
        if !line.to_lowercase().contains("state") {
            continue;
        }
        let Some(value) = extract_state_value(line) else { continue };
        match value.to_uppercase().as_str() {
            "RINGING" | "SIMULATED_RINGING" | "INCOMING" => return Some(CallType::Incoming),
            "DIALING" | "CONNECTING" | "OUTGOING" => return Some(CallType::Outgoing),
            "ACTIVE" | "HOLDING" | "AUDIO_PROCESSING" | "OFFHOOK" => return Some(CallType::Active),
            _ => {}
        }
    }
    None
}

/// Extract the value token after the first `=` or `:` on a state line.
fn extract_state_value(line: &str) -> Option<&str> {
    let idx = line.find(['=', ':'])?;
    let after = line[idx + 1..].trim();
    let token = after.split_whitespace().next()?;
    let token = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
    if token.is_empty() { None } else { Some(token) }
}

/// Extract a phone number from a `tel:` URI anywhere in the dump (handles the
/// common `%2B` URL-encoding of a leading `+`).
fn extract_tel_number(text: &str) -> Option<String> {
    let idx = text.find("tel:")?;
    let rest = &text[idx + 4..];
    let snippet: String = rest.chars().take(40).collect();
    let decoded = snippet.replace("%2B", "+").replace("%2b", "+");
    let num: String = decoded
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '+' | '-' | '(' | ')' | ' '))
        .collect();
    let num = num.trim().to_string();
    if num.chars().filter(|c| c.is_ascii_digit()).count() >= 3 {
        Some(num)
    } else {
        None
    }
}
