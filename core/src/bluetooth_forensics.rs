//! Bluetooth pairing forensics — extracts paired/bonded devices.
//!
//! Parses `dumpsys bluetooth_manager` for bonded devices, proving
//! associations with headphones, cars, speakers, etc.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BluetoothDevice {
    pub name: String,
    pub mac_address: String,
    pub device_type: String,
    pub bond_state: String,
    pub alias: Option<String>,
}

/// Collect Bluetooth paired devices from the device.
pub fn collect_bluetooth_devices(adb: &AdbClient, serial: &str) -> Result<Vec<BluetoothDevice>> {
    let out = adb.exec_out(serial, &["dumpsys", "bluetooth_manager"])?;
    let text = String::from_utf8_lossy(&out);
    let mut devices = Vec::new();

    let mut in_bonded = false;
    let mut current_addr: Option<String> = None;
    let mut current_name = String::new();
    let mut current_type = String::new();
    let mut current_bond = String::new();
    let mut current_alias: Option<String> = None;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.contains("BondedDevices") || trimmed.contains("bondedDevices") {
            in_bonded = true;
            continue;
        }

        if !in_bonded {
            continue;
        }

        // Device address line: "  Device: XX:XX:XX:XX:XX:XX"
        if trimmed.starts_with("Device:") || trimmed.starts_with("Address:") {
            // Save previous device
            if let Some(addr) = current_addr.take() {
                if addr.contains(':') {
                    devices.push(BluetoothDevice {
                        name: std::mem::take(&mut current_name),
                        mac_address: addr,
                        device_type: std::mem::take(&mut current_type),
                        bond_state: std::mem::take(&mut current_bond),
                        alias: current_alias.take(),
                    });
                }
            }
            let addr = trimmed.split(':').nth(1).unwrap_or("").trim().to_string();
            // If split by ':' lost the MAC, try the whole value
            let addr = if addr.contains(':') || addr.len() == 17 {
                addr
            } else {
                trimmed.split_whitespace().last().unwrap_or("").to_string()
            };
            current_addr = Some(addr);
            current_bond = "bonded".to_string();
            continue;
        }

        if let Some(ref _addr) = current_addr {
            if trimmed.starts_with("Name:") || trimmed.starts_with("mName:") {
                current_name = trimmed.split(':').nth(1).unwrap_or("").trim().to_string();
            }
            if trimmed.starts_with("Alias:") || trimmed.starts_with("mAlias:") {
                current_alias = Some(trimmed.split(':').nth(1).unwrap_or("").trim().to_string());
            }
            if trimmed.starts_with("DeviceType:") || trimmed.contains("type=") {
                let val = trimmed.split('=').nth(1).or_else(|| trimmed.split(':').nth(1)).unwrap_or("").trim();
                current_type = val.to_string();
            }
            if trimmed.starts_with("BondState:") || trimmed.contains("bondState=") {
                let val = trimmed.split('=').nth(1).or_else(|| trimmed.split(':').nth(1)).unwrap_or("").trim();
                current_bond = val.to_string();
            }
        }

        // End of bonded section
        if trimmed.starts_with("AvailableDevices") || trimmed.starts_with("DiscoveredDevices") {
            in_bonded = false;
        }
    }

    // Save last device
    if let Some(addr) = current_addr {
        if addr.contains(':') {
            devices.push(BluetoothDevice {
                name: current_name,
                mac_address: addr,
                device_type: current_type,
                bond_state: current_bond,
                alias: current_alias,
            });
        }
    }

    // Fallback: try `settings get global bluetooth_disabled_subsystems` or list
    if devices.is_empty() {
        if let Ok(bt_out) = adb.exec_out(serial, &["settings", "get", "global", "bluetooth_on"]) {
            let bt_text = String::from_utf8_lossy(&bt_out).trim().to_string();
            if !bt_text.is_empty() && bt_text != "0" {
                // Bluetooth is on but no bonded devices found in dumpsys
            }
        }
    }

    Ok(devices)
}
