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

/// Poll the device for current call state.
pub fn poll_call_state(adb: &AdbClient, serial: &str) -> Result<Option<CallEvent>> {
    // Query telephony registry for call state
    let cmd = "dumpsys telephony.registry | grep -E 'mCallState|mCallIncomingNumber'";
    let output = adb.exec_out(serial, &["sh", "-c", cmd])?;
    let text = String::from_utf8_lossy(&output);
    
    let mut call_state: Option<i32> = None;
    let mut phone_number: Option<String> = None;
    
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("mCallState=") {
            if let Some(val) = line.strip_prefix("mCallState=") {
                call_state = val.trim().parse().ok();
            }
        } else if line.contains("mCallIncomingNumber=") {
            if let Some(idx) = line.find("mCallIncomingNumber=") {
                let val = &line[idx + 20..];
                let num = val.trim().trim_matches('"').to_string();
                if !num.is_empty() && num != "null" {
                    phone_number = Some(num);
                }
            }
        }
    }
    
    let event_type = match call_state {
        Some(0) => CallType::Idle,
        Some(1) => CallType::Incoming,  // RINGING
        Some(2) => CallType::Outgoing,  // OFFHOOK
        _ => CallType::Unknown,
    };
    
    if event_type != CallType::Unknown && event_type != CallType::Idle {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        
        return Ok(Some(CallEvent {
            timestamp: now,
            event_type,
            phone_number,
            contact_name: None,
            duration: None,
            sim_slot: None,
        }));
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
    match poll_call_state(adb, serial)? {
        Some(event) => {
            let msg = match &event.event_type {
                CallType::Incoming => {
                    format!(
                        "📞 INCOMING CALL{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" from {}", n))
                            .unwrap_or_default()
                    )
                }
                CallType::Outgoing => {
                    format!(
                        "📱 OUTGOING CALL{}",
                        event.phone_number.as_ref()
                            .map(|n| format!(" to {}", n))
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
