//! WiFi network forensics — extracts saved/joined WiFi networks from the device.
//!
//! Parses `dumpsys wifi` output for configured networks, extracting SSID, BSSID,
//! security type, and connection metadata. Proves physical location history.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: Option<String>,
    pub security: String,
    pub freq_mhz: Option<i32>,
    pub is_saved: bool,
    pub last_connected: Option<i64>,
    pub network_id: Option<i32>,
}

/// Collect WiFi network history from the device.
pub fn collect_wifi_networks(adb: &AdbClient, serial: &str) -> Result<Vec<WifiNetwork>> {
    let out = adb.exec_out(serial, &["dumpsys", "wifi"])?;
    let text = String::from_utf8_lossy(&out);
    let mut networks = Vec::new();

    // Parse "mLastConfiguredNetworks" and "Networks" sections
    let mut in_networks = false;
    let mut current: Option<WifiNetwork> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        // Detect start of configured networks section
        if trimmed.contains("mConfiguredNetworks") || trimmed.contains("Networks:") {
            in_networks = true;
            continue;
        }

        if !in_networks {
            continue;
        }

        // New network entry: "Network id=<N> ..."
        if trimmed.starts_with("Network id=") || (trimmed.starts_with('<') && trimmed.contains("SSID:")) {
            if let Some(net) = current.take() {
                if !net.ssid.is_empty() {
                    networks.push(net);
                }
            }
            current = Some(WifiNetwork {
                ssid: String::new(),
                bssid: None,
                security: String::new(),
                freq_mhz: None,
                is_saved: true,
                last_connected: None,
                network_id: extract_int(trimmed, "id="),
            });
        }

        if let Some(ref mut net) = current {
            // SSID
            if trimmed.starts_with("SSID:") || trimmed.starts_with("mSSID:") {
                let val = trimmed.split(':').nth(1).unwrap_or("").trim();
                net.ssid = val.trim_matches('"').to_string();
            }
            // BSSID
            if trimmed.starts_with("BSSID:") || trimmed.starts_with("mBSSID:") {
                let val = trimmed.split(':').nth(1).unwrap_or("").trim();
                if !val.is_empty() {
                    net.bssid = Some(val.to_string());
                }
            }
            // Security / key management
            if trimmed.contains("KeyMgmt=") || trimmed.contains("KeyMgmt:") {
                let val = trimmed.split('=').nth(1).or_else(|| trimmed.split(':').nth(1)).unwrap_or("").trim();
                net.security = val.to_string();
            }
            // Frequency
            if trimmed.starts_with("Frequency:") || trimmed.starts_with("freq=") {
                if let Some(v) = extract_int(trimmed, ":").or_else(|| extract_int(trimmed, "=")) {
                    net.freq_mhz = Some(v);
                }
            }
            // Last connected timestamp
            if trimmed.contains("lastConnected=") || trimmed.contains("last_connected=") {
                if let Some(v) = extract_int(trimmed, "=") {
                    net.last_connected = Some(v as i64);
                }
            }
        }

        // End of networks section
        if trimmed.is_empty() && in_networks && current.is_none() {
            // Could be end of section, but keep parsing
        }
    }

    // Don't forget the last network
    if let Some(net) = current {
        if !net.ssid.is_empty() {
            networks.push(net);
        }
    }

    // If dumpsys parsing found nothing, try content provider fallback
    if networks.is_empty() {
        if let Ok(cp_out) = adb.exec_out(serial, &[
            "content", "query",
            "--uri", "content://settings/global",
            "--where", "name='wifi_saved_access_points'",
        ]) {
            let cp_text = String::from_utf8_lossy(&cp_out);
            for line in cp_text.lines() {
                if line.contains("value=") {
                    if let Some(val) = line.split_once("value=").map(|(_, v)| v.trim()) {
                        if !val.is_empty() && val != "null" {
                            // Just note that WiFi data exists
                            networks.push(WifiNetwork {
                                ssid: format!("(saved access points: {})", val),
                                bssid: None,
                                security: "unknown".into(),
                                freq_mhz: None,
                                is_saved: true,
                                last_connected: None,
                                network_id: None,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(networks)
}

fn extract_int(s: &str, sep: &str) -> Option<i32> {
    s.split(sep).nth(1)?.trim().split(|c: char| !c.is_ascii_digit() && c != '-').next()?.parse().ok()
}
