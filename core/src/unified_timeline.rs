//! Unified forensic timeline: merge ALL data sources into one chronological stream.
//!
//! Takes events from calls, SMS, WhatsApp, WiFi, Bluetooth, power, notifications,
//! browser history, location, and deleted data, then produces a single sorted timeline.

use serde::{Deserialize, Serialize};

/// Truncate a string to at most `max` bytes, never splitting a UTF-8 character.
fn truncate_safe(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    // Find the first character whose end would exceed max, truncate before it
    for (idx, ch) in s.char_indices() {
        if idx + ch.len_utf8() > max {
            return &s[..idx];
        }
    }
    s
}

/// Category of a unified timeline event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum EventCategory {
    Call,
    Sms,
    Whatsapp,
    Wifi,
    Bluetooth,
    Power,
    Notification,
    Browser,
    Location,
    Account,
    DeletedSms,
    System,
}

/// A single event in the unified timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedEvent {
    /// Unix timestamp in milliseconds.
    pub timestamp: i64,
    pub category: EventCategory,
    pub title: String,
    pub detail: Option<String>,
    pub source_app: Option<String>,
    pub device_serial: Option<String>,
    pub is_deleted: bool,
}

/// Input data for building the unified timeline.
#[derive(Debug, Default)]
pub struct TimelineInput {
    pub calls: Vec<(i64, String, String)>,          // (ts, type, number)
    pub sms: Vec<(i64, String, String, bool)>,       // (ts, address, body, deleted)
    pub whatsapp: Vec<(i64, String, String, bool)>,  // (ts, chat_jid, text, from_me)
    pub wifi_events: Vec<(i64, String)>,             // (ts, ssid)
    pub bluetooth_events: Vec<(i64, String)>,        // (ts, device_name)
    pub power_events: Vec<(i64, String, u8)>,        // (ts, event_type, battery_level)
    pub notifications: Vec<(i64, String, String)>,   // (ts, package, text)
    pub browser_history: Vec<(i64, String, String)>, // (ts, url, title)
    pub location_events: Vec<(i64, f64, f64, String)>, // (ts, lat, lon, source)
    pub accounts: Vec<String>,                        // account names
}

/// Build a unified timeline from all input sources.
pub fn build_unified_timeline(input: &TimelineInput, serial: Option<&str>) -> Vec<UnifiedEvent> {
    let mut events = Vec::new();
    let dev = serial.map(|s| s.to_string());

    // Calls
    for (ts, call_type, number) in &input.calls {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Call,
            title: format!("{call_type} call"),
            detail: Some(number.clone()),
            source_app: Some("Phone".into()),
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // SMS (live + deleted)
    for (ts, address, body, deleted) in &input.sms {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Sms,
            title: if *deleted { "Deleted SMS".into() } else { "SMS".into() },
            detail: Some(format!("{address}: {}", truncate_safe(body, 80))),
            source_app: Some("Messages".into()),
            device_serial: dev.clone(),
            is_deleted: *deleted,
        });
    }

    // WhatsApp
    for (ts, jid, text, from_me) in &input.whatsapp {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Whatsapp,
            title: if *from_me { "WA sent".into() } else { "WA received".into() },
            detail: Some(format!("{jid}: {}", truncate_safe(text, 80))),
            source_app: Some("WhatsApp".into()),
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // WiFi
    for (ts, ssid) in &input.wifi_events {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Wifi,
            title: "WiFi connected".into(),
            detail: Some(ssid.clone()),
            source_app: None,
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Bluetooth
    for (ts, name) in &input.bluetooth_events {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Bluetooth,
            title: "BT paired".into(),
            detail: Some(name.clone()),
            source_app: None,
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Power
    for (ts, event_type, level) in &input.power_events {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Power,
            title: format!("Power: {event_type}"),
            detail: Some(format!("{level}%")),
            source_app: None,
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Notifications
    for (ts, package, text) in &input.notifications {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Notification,
            title: format!("Notification: {}", package.split('.').last().unwrap_or(package)),
            detail: Some(text.clone()),
            source_app: Some(package.clone()),
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Browser
    for (ts, url, title) in &input.browser_history {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Browser,
            title: title.clone(),
            detail: Some(url.clone()),
            source_app: Some("Chrome".into()),
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Location
    for (ts, lat, lon, source) in &input.location_events {
        events.push(UnifiedEvent {
            timestamp: *ts,
            category: EventCategory::Location,
            title: format!("Location ({source})"),
            detail: Some(format!("{lat:.6}, {lon:.6}")),
            source_app: None,
            device_serial: dev.clone(),
            is_deleted: false,
        });
    }

    // Sort by timestamp.
    events.sort_by_key(|e| e.timestamp);
    events
}
