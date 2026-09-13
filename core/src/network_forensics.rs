//! Network artifact collection: TCP connections, ARP table, DNS cache.
//!
//! Reads from `/proc/net/tcp`, `/proc/net/arp`, and `dumpsys netstats`
//! via ADB `exec-out`.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Active TCP connection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TcpConnection {
    pub local_addr: String,
    pub local_port: u16,
    pub remote_addr: String,
    pub remote_port: u16,
    pub state: String,
}

/// ARP table entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArpEntry {
    pub ip: String,
    pub mac: String,
    pub device: String,
    pub state: String,
}

/// DNS cache entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DnsCacheEntry {
    pub domain: String,
    pub resolved_ip: String,
    pub ttl: Option<u32>,
}

/// Network forensics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkForensicsResult {
    pub tcp_connections: Vec<TcpConnection>,
    pub arp_entries: Vec<ArpEntry>,
    pub dns_cache: Vec<DnsCacheEntry>,
    pub notes: Vec<String>,
}

/// TCP state code to name mapping.
fn tcp_state_name(state: u8) -> String {
    match state {
        0x01 => "ESTABLISHED".into(),
        0x02 => "SYN_SENT".into(),
        0x03 => "SYN_RECV".into(),
        0x04 => "FIN_WAIT1".into(),
        0x05 => "FIN_WAIT2".into(),
        0x06 => "TIME_WAIT".into(),
        0x07 => "CLOSE".into(),
        0x08 => "CLOSE_WAIT".into(),
        0x09 => "LAST_ACK".into(),
        0x0A => "LISTEN".into(),
        0x0B => "CLOSING".into(),
        _ => format!("UNKNOWN({state})"),
    }
}

/// Parse a hex-encoded IPv4 address (little-endian) from /proc/net/tcp.
fn parse_hex_ip(hex: &str) -> String {
    if hex.len() != 8 {
        return hex.to_string();
    }
    match u32::from_str_radix(hex, 16) {
        Ok(n) => {
            // /proc/net/tcp stores IP in little-endian.
            format!("{}.{}.{}.{}", n & 0xFF, (n >> 8) & 0xFF, (n >> 16) & 0xFF, (n >> 24) & 0xFF)
        }
        Err(_) => hex.to_string(),
    }
}

/// Collect network artifacts from the device.
pub fn collect_network_artifacts(adb: &AdbClient, serial: &str) -> Result<NetworkForensicsResult> {
    let mut notes = Vec::new();
    let mut tcp_connections = Vec::new();
    let mut arp_entries = Vec::new();
    let mut dns_cache = Vec::new();

    // 1) TCP connections from /proc/net/tcp
    if let Ok(output) = adb.exec_out(serial, &["cat", "/proc/net/tcp"]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines().skip(1) {
            // Format: sl local_address:port rem_address:port st ...
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 {
                continue;
            }
            let local = parts[1];
            let remote = parts[2];
            let state_hex = parts[3];

            if let Some((lip, lport)) = local.rsplit_once(':') {
                if let Some((rip, rport)) = remote.rsplit_once(':') {
                    let local_addr = parse_hex_ip(lip);
                    let local_port = u16::from_str_radix(lport, 16).unwrap_or(0);
                    let remote_addr = parse_hex_ip(rip);
                    let remote_port = u16::from_str_radix(rport, 16).unwrap_or(0);
                    let state = u8::from_str_radix(state_hex, 16).unwrap_or(0);

                    // Skip TIME_WAIT and CLOSE to reduce noise.
                    if state == 0x06 || state == 0x07 {
                        continue;
                    }

                    tcp_connections.push(TcpConnection {
                        local_addr,
                        local_port,
                        remote_addr,
                        remote_port,
                        state: tcp_state_name(state),
                    });
                }
            }
        }
        notes.push(format!("{} active TCP connections", tcp_connections.len()));
    }

    // 2) ARP table from /proc/net/arp
    if let Ok(output) = adb.exec_out(serial, &["cat", "/proc/net/arp"]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 6 {
                continue;
            }
            arp_entries.push(ArpEntry {
                ip: parts[0].to_string(),
                mac: parts[3].to_string(),
                device: parts[5].to_string(),
                state: parts[2].to_string(), // 0x2 = complete
            });
        }
        notes.push(format!("{} ARP entries", arp_entries.len()));
    }

    // 3) DNS cache via dumpsys netstats or ndc resolver
    if let Ok(output) = adb.exec_out(serial, &["dumpsys", "netstats"]) {
        let text = String::from_utf8_lossy(&output);
        // Parse network stats for active interfaces.
        for line in text.lines() {
            if line.contains("NetworkStats") || line.contains("activeNetwork") {
                notes.push(format!("netstats: {}", line.trim()));
            }
        }
    }

    // Try ndc resolver for DNS cache (may not work on all devices).
    if let Ok(output) = adb.exec_out(serial, &["ndc", "resolver", "list"]) {
        let text = String::from_utf8_lossy(&output);
        for line in text.lines() {
            // Format varies, but typically: domain ip ttl
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                dns_cache.push(DnsCacheEntry {
                    domain: parts[0].to_string(),
                    resolved_ip: parts[1].to_string(),
                    ttl: parts.get(2).and_then(|s| s.parse().ok()),
                });
            }
        }
        if !dns_cache.is_empty() {
            notes.push(format!("{} DNS cache entries", dns_cache.len()));
        }
    }

    Ok(NetworkForensicsResult {
        tcp_connections,
        arp_entries,
        dns_cache,
        notes,
    })
}
