//! Location/GPS timeline aggregation.
//!
//! Collects location data from multiple sources:
//! - `dumpsys location` for last known locations
//! - Cell tower history from `dumpsys telephony.registry`
//! - WiFi scan history (already in wifi_forensics)
//! - Photo EXIF GPS (integrated via exif_parser)

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// A single location point from any source.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationPoint {
    /// Unix timestamp in milliseconds.
    pub timestamp: Option<i64>,
    pub latitude: f64,
    pub longitude: f64,
    /// Accuracy in meters (if available).
    pub accuracy_m: Option<f64>,
    /// Source of the location data.
    pub source: String,
    /// Human-readable address hint (if available).
    pub address_hint: Option<String>,
}

/// Location forensics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocationForensicsResult {
    pub points: Vec<LocationPoint>,
    pub notes: Vec<String>,
}

/// Collect location data from the device.
pub fn collect_location_data(adb: &AdbClient, serial: &str) -> Result<LocationForensicsResult> {
    let mut notes = Vec::new();
    let mut points = Vec::new();

    // 1) Last known location from dumpsys location.
    if let Ok(output) = adb.exec_out(serial, &["dumpsys", "location"]) {
        let text = String::from_utf8_lossy(&output);
        let mut in_last = false;
        let mut lat = None;
        let mut lon = None;
        let mut acc = None;
        let mut ts = None;

        for line in text.lines() {
            let line = line.trim();
            if line.contains("Last Known Location") || line.contains("lastLocation") {
                in_last = true;
                continue;
            }
            if in_last {
                if line.starts_with("Latitude=") || line.contains("lat=") {
                    lat = extract_float(line);
                } else if line.starts_with("Longitude=") || line.contains("lon=") {
                    lon = extract_float(line);
                } else if line.starts_with("Accuracy=") || line.contains("accuracy=") {
                    acc = extract_float(line);
                } else if line.contains("Time=") || line.contains("timestamp=") {
                    ts = extract_int(line);
                }
                if lat.is_some() && lon.is_some() {
                    points.push(LocationPoint {
                        timestamp: ts,
                        latitude: lat.unwrap(),
                        longitude: lon.unwrap(),
                        accuracy_m: acc,
                        source: "gps_last".into(),
                        address_hint: None,
                    });
                    in_last = false;
                    lat = None;
                    lon = None;
                    acc = None;
                    ts = None;
                }
                // Stop after a reasonable number of lines.
                if line.is_empty() {
                    in_last = false;
                }
            }
        }
    }

    // 2) Cell tower info from dumpsys telephony.registry.
    if let Ok(output) = adb.exec_out(serial, &["dumpsys", "telephony.registry"]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines() {
            let line = line.trim();
            if line.contains("CellLocation") || line.contains("cellLocation") {
                // Try to extract CID/LAC as a rough location indicator.
                if let Some(cid) = extract_field(line, "cid=") {
                    if let Some(lac) = extract_field(line, "lac=") {
                        notes.push(format!("Cell: CID={cid}, LAC={lac}"));
                    }
                }
            }
        }
    }

    // 3) Location settings from settings secure/global.
    if let Ok(output) = adb.exec_out(serial, &["settings", "get", "secure", "location_providers_allowed"]) {
        let text = String::from_utf8_lossy(&output).trim().to_string();
        notes.push(format!("Location providers: {text}"));
    }

    // 4) Try to get location from Google Maps cache (requires root).
    if let Ok(output) = adb.exec_out(serial, &["dumpsys", "location", "last"]) {
        let text = String::from_utf8_lossy(&output);
        let mut lat = None;
        let mut lon = None;
        let mut acc = None;

        for line in text.lines() {
            let line = line.trim();
            if line.contains("latitude") {
                lat = extract_float(line);
            } else if line.contains("longitude") {
                lon = extract_float(line);
            } else if line.contains("accuracy") && acc.is_none() {
                acc = extract_float(line);
            }
        }

        if let (Some(lat), Some(lon)) = (lat, lon) {
            if lat != 0.0 && lon != 0.0 {
                points.push(LocationPoint {
                    timestamp: None,
                    latitude: lat,
                    longitude: lon,
                    accuracy_m: acc,
                    source: "location_last".into(),
                    address_hint: None,
                });
            }
        }
    }

    if points.is_empty() {
        notes.push("No location data found — device may have location services disabled".into());
    } else {
        notes.push(format!("{} location points collected", points.len()));
    }

    // Sort by timestamp.
    points.sort_by_key(|p| p.timestamp.unwrap_or(0));

    Ok(LocationForensicsResult { points, notes })
}

fn extract_float(line: &str) -> Option<f64> {
    // Look for a floating point number in the line.
    for part in line.split(|c: char| !c.is_ascii_digit() && c != '.' && c != '-') {
        if let Ok(f) = part.parse::<f64>() {
            if f.abs() < 180.0 && f.abs() > 0.001 {
                return Some(f);
            }
        }
    }
    None
}

fn extract_int(line: &str) -> Option<i64> {
    for part in line.split(|c: char| !c.is_ascii_digit() && c != '-') {
        if let Ok(i) = part.parse::<i64>() {
            if i > 1_000_000_000_000 {
                return Some(i);
            }
        }
    }
    None
}

fn extract_field(line: &str, prefix: &str) -> Option<String> {
    if let Some(idx) = line.find(prefix) {
        let rest = &line[idx + prefix.len()..];
        let end = rest.find(|c: char| !c.is_ascii_digit() && c != 'x' && c != 'X')
            .unwrap_or(rest.len());
        let val = &rest[..end];
        if !val.is_empty() {
            return Some(val.to_string());
        }
    }
    None
}
