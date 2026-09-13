//! Power/battery event forensics — extracts charge/discharge timeline.
//!
//! Parses `dumpsys batterystats --history` for plug/unplug events,
//! charge levels, and power state changes. Timeline evidence.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerEvent {
    pub timestamp_ms: i64,
    pub event_type: String,
    pub battery_level: i32,
    pub detail: Option<String>,
}

/// Collect power/battery events from the device.
pub fn collect_power_events(adb: &AdbClient, serial: &str) -> Result<Vec<PowerEvent>> {
    let out = adb.exec_out(serial, &["dumpsys", "batterystats", "--history"])?;
    let text = String::from_utf8_lossy(&out);
    let mut events = Vec::new();

    // Parse battery history lines like:
    // "Battery History:"
    // "+1ms line 1: 100 Discharging PLUGGED:AC ..."
    // "+5s: 95 Charging PLUGGED:USB ..."
    let mut base_time: Option<i64> = None;
    let mut elapsed_ms: i64 = 0;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Battery History:") || trimmed.starts_with("Statistics since:") {
            continue;
        }

        // Time offset line: "+1234ms ..." or "+5s ..."
        if trimmed.starts_with('+') {
            let rest = &trimmed[1..];
            if let Some(ms_str) = rest.split(|c: char| c == ' ' || c == ':').next() {
                let ms = if ms_str.ends_with("ms") {
                    ms_str.trim_end_matches("ms").parse::<i64>().ok()
                } else if ms_str.ends_with('s') {
                    ms_str.trim_end_matches('s').parse::<i64>().ok().map(|s| s * 1000)
                } else if ms_str.ends_with("m") {
                    ms_str.trim_end_matches('m').parse::<i64>().ok().map(|m| m * 60000)
                } else if ms_str.ends_with("h") {
                    ms_str.trim_end_matches('h').parse::<i64>().ok().map(|h| h * 3600000)
                } else {
                    ms_str.parse::<i64>().ok()
                };

                if let Some(delta) = ms {
                    elapsed_ms += delta;
                }
            }

            // Parse battery level and state from the rest of the line
            let level = extract_battery_level(trimmed);
            let (event_type, detail) = classify_power_event(trimmed);

            if !event_type.is_empty() {
                events.push(PowerEvent {
                    timestamp_ms: elapsed_ms, // relative to history start
                    event_type,
                    battery_level: level.unwrap_or(0),
                    detail,
                });
            }
        }

        // Also look for absolute timestamps in "Time on battery:" sections
        if trimmed.starts_with("Time on battery:") {
            if let Some(val) = trimmed.split(':').nth(1) {
                base_time = Some(0); // marker
                events.push(PowerEvent {
                    timestamp_ms: 0,
                    event_type: "history_start".into(),
                    battery_level: 0,
                    detail: Some(val.trim().to_string()),
                });
            }
        }
    }

    // Also get current battery state for context
    if let Ok(bat_out) = adb.exec_out(serial, &["dumpsys", "battery"]) {
        let bat_text = String::from_utf8_lossy(&bat_out);
        let level = bat_text.lines()
            .find(|l| l.trim().starts_with("level:"))
            .and_then(|l| l.split(':').nth(1))
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(-1);
        let plugged = bat_text.lines()
            .find(|l| l.trim().starts_with("USB powered:") || l.trim().starts_with("AC powered:") || l.trim().starts_with("Wireless powered:"))
            .map(|l| l.trim().to_string());

        if level >= 0 {
            events.push(PowerEvent {
                timestamp_ms: elapsed_ms + 1, // just after last history event
                event_type: "current_state".into(),
                battery_level: level,
                detail: plugged,
            });
        }
    }

    Ok(events)
}

fn extract_battery_level(line: &str) -> Option<i32> {
    // Look for a number followed by a space and a state keyword
    for part in line.split_whitespace() {
        if let Ok(n) = part.parse::<i32>() {
            if n >= 0 && n <= 100 {
                return Some(n);
            }
        }
    }
    None
}

fn classify_power_event(line: &str) -> (String, Option<String>) {
    let lower = line.to_lowercase();
    if lower.contains("plugged:ac") || lower.contains("plugged:usb") || lower.contains("plugged:wireless") {
        let plug_type = if lower.contains("ac") { "AC" } else if lower.contains("usb") { "USB" } else { "wireless" };
        ("plug".into(), Some(format!("plugged:{}", plug_type)))
    } else if lower.contains("unplugged") || lower.contains("discharging") {
        ("unplug".into(), None)
    } else if lower.contains("charging") {
        ("charge_start".into(), None)
    } else if lower.contains("full") && lower.contains("charge") {
        ("charge_full".into(), None)
    } else if lower.contains("shutdown") {
        ("shutdown".into(), None)
    } else {
        (String::new(), None)
    }
}
