//! Remote forensic agent receiver.
//!
//! Runs a lightweight HTTP server that accepts forensic data packages
//! from the RecoverKit Remote Agent Android app (or any compatible client).
//! Received packages are stored locally and fed into the existing
//! scan-folder → parse → carve → export pipeline.
//!
//! ## Protocol
//!
//! ```text
//! POST /upload  Content-Type: application/json
//!
//! {
//!   "agent_version": "1.0",
//!   "device_id": "...",
//!   "device_model": "...",
//!   "android_version": "...",
//!   "timestamp": 1700000000000,
//!   "device_info": { ... },
//!   "contacts": [ ... ],
//!   "call_logs": [ ... ],
//!   "sms_messages": [ ... ],
//!   "browser_bookmarks": [ ... ],
//!   "wifi_networks": [ ... ],
//!   "bluetooth_devices": [ ... ],
//!   "app_list": [ ... ],
//!   "media_manifest": [ ... ],
//!   "notes": [ ... ]
//! }
//! ```

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Default port for the remote agent receiver.
const DEFAULT_PORT: u16 = 7799;

/// A single contact from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteContact {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

/// A single call log entry from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteCallLog {
    pub number: String,
    pub name: Option<String>,
    pub call_type: String,
    pub duration: i64,
    pub timestamp: i64,
}

/// A single SMS message from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSms {
    pub address: String,
    pub body: Option<String>,
    pub msg_type: String,
    pub timestamp: i64,
}

/// A browser bookmark from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBookmark {
    pub url: String,
    pub title: Option<String>,
}

/// A WiFi network entry from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWifiNetwork {
    pub ssid: String,
    pub bssid: Option<String>,
    pub security: String,
}

/// A Bluetooth device from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteBluetoothDevice {
    pub name: String,
    pub mac_address: String,
    pub bond_state: String,
}

/// An installed app entry from the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteApp {
    pub package: String,
    pub version_name: Option<String>,
    pub is_system: bool,
}

/// A media file manifest entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteMediaEntry {
    pub path: String,
    pub size: i64,
    pub media_type: String,
}

/// Remote device identity info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteDeviceInfo {
    pub manufacturer: Option<String>,
    pub brand: Option<String>,
    pub model: Option<String>,
    pub android_version: Option<String>,
    pub sdk_level: Option<String>,
    pub security_patch: Option<String>,
    pub imei: Option<String>,
    pub serial: Option<String>,
    pub battery_level: Option<String>,
    pub storage_free: Option<String>,
    pub wifi_ip: Option<String>,
    pub mac_address: Option<String>,
}

/// The full forensic package sent by the remote agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicPackage {
    pub agent_version: String,
    pub device_id: String,
    pub device_model: Option<String>,
    pub android_version: Option<String>,
    pub timestamp: i64,
    #[serde(default)]
    pub device_info: Option<RemoteDeviceInfo>,
    #[serde(default)]
    pub contacts: Vec<RemoteContact>,
    #[serde(default)]
    pub call_logs: Vec<RemoteCallLog>,
    #[serde(default)]
    pub sms_messages: Vec<RemoteSms>,
    #[serde(default)]
    pub browser_bookmarks: Vec<RemoteBookmark>,
    #[serde(default)]
    pub wifi_networks: Vec<RemoteWifiNetwork>,
    #[serde(default)]
    pub bluetooth_devices: Vec<RemoteBluetoothDevice>,
    #[serde(default)]
    pub app_list: Vec<RemoteApp>,
    #[serde(default)]
    pub media_manifest: Vec<RemoteMediaEntry>,
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Status of the remote agent receiver server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Stopped,
    Running,
    Error(String),
}

/// Summary of a received forensic package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceivedPackage {
    pub id: String,
    pub device_id: String,
    pub device_model: Option<String>,
    pub android_version: Option<String>,
    pub timestamp: i64,
    pub received_at: i64,
    pub contacts_count: usize,
    pub calls_count: usize,
    pub sms_count: usize,
    pub bookmarks_count: usize,
    pub wifi_count: usize,
    pub bluetooth_count: usize,
    pub apps_count: usize,
    pub media_count: usize,
    pub local_path: String,
    pub notes: Vec<String>,
}

/// Shared state for the remote agent server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteAgentState {
    pub status: ServerStatus,
    pub port: u16,
    pub packages: Vec<ReceivedPackage>,
    pub storage_dir: String,
}

impl Default for RemoteAgentState {
    fn default() -> Self {
        Self {
            status: ServerStatus::Stopped,
            port: DEFAULT_PORT,
            packages: Vec::new(),
            storage_dir: String::new(),
        }
    }
}

/// Global shared state for packages received.
static PACKAGES: Mutex<Option<Arc<Mutex<Vec<ReceivedPackage>>>>> = Mutex::new(None);
/// Global stop flag.
static STOP_FLAG: Mutex<Option<Arc<Mutex<bool>>>> = Mutex::new(None);

/// Start the remote agent HTTP receiver server.
pub fn start_server(port: u16, storage_dir: &Path) -> Result<RemoteAgentState> {
    // Stop any existing server first
    stop_server();

    std::fs::create_dir_all(storage_dir).map_err(|e| {
        CoreError::Adb(format!("cannot create remote storage dir: {e}"))
    })?;

    let addr: SocketAddr = format!("0.0.0.0:{port}").parse().map_err(|e| {
        CoreError::Adb(format!("invalid port {port}: {e}"))
    })?;

    let listener = TcpListener::bind(addr).map_err(|e| {
        CoreError::Adb(format!("failed to bind HTTP server on port {port}: {e}"))
    })?;
    listener.set_nonblocking(false).ok();

    let stop_flag = Arc::new(Mutex::new(false));
    let packages: Arc<Mutex<Vec<ReceivedPackage>>> = Arc::new(Mutex::new(Vec::new()));

    // Load existing packages from disk
    let existing = load_package_index(storage_dir);
    if let Ok(mut pkgs) = packages.lock() {
        *pkgs = existing;
    }

    let stop_clone = stop_flag.clone();
    let packages_clone = packages.clone();
    let storage = storage_dir.to_path_buf();

    // Store globals
    {
        let mut sf = STOP_FLAG.lock().unwrap();
        *sf = Some(stop_flag.clone());
    }
    {
        let mut pk = PACKAGES.lock().unwrap();
        *pk = Some(packages.clone());
    }

    std::thread::spawn(move || {
        eprintln!("[remote-agent] Receiver listening on 0.0.0.0:{port}");
        listener.set_nonblocking(true).ok();

        loop {
            // Check stop flag
            if *stop_clone.lock().unwrap() {
                eprintln!("[remote-agent] Stop signal received, shutting down");
                break;
            }

            match listener.accept() {
                Ok((stream, _addr)) => {
                    let storage = storage.clone();
                    let packages = packages_clone.clone();
                    std::thread::spawn(move || {
                        handle_connection(stream, &storage, &packages);
                    });
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    continue;
                }
                Err(e) => {
                    eprintln!("[remote-agent] Accept error: {e}");
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
            }
        }
        eprintln!("[remote-agent] Server thread exiting");
    });

    let state = RemoteAgentState {
        status: ServerStatus::Running,
        port,
        packages: Vec::new(),
        storage_dir: storage_dir.display().to_string(),
    };

    Ok(state)
}

/// Stop the running server.
pub fn stop_server() -> RemoteAgentState {
    if let Some(flag) = STOP_FLAG.lock().unwrap().take() {
        let mut f = flag.lock().unwrap();
        *f = true;
    }
    RemoteAgentState {
        status: ServerStatus::Stopped,
        port: DEFAULT_PORT,
        packages: Vec::new(),
        storage_dir: String::new(),
    }
}

/// Get the current server state.
pub fn get_server_state(storage_dir: &Path) -> RemoteAgentState {
    let is_running = STOP_FLAG.lock().unwrap().as_ref().map_or(false, |f| !*f.lock().unwrap());

    let status = if is_running {
        ServerStatus::Running
    } else {
        ServerStatus::Stopped
    };

    let packages = if storage_dir.exists() {
        load_package_index(storage_dir)
    } else {
        Vec::new()
    };

    RemoteAgentState {
        status,
        port: DEFAULT_PORT,
        packages,
        storage_dir: storage_dir.display().to_string(),
    }
}

/// Handle a single HTTP connection.
fn handle_connection(
    mut stream: TcpStream,
    storage_dir: &Path,
    packages: &Arc<Mutex<Vec<ReceivedPackage>>>,
) {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(30))).ok();

    let mut reader = BufReader::new(stream.try_clone().unwrap());

    // Read request line
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).is_err() {
        return;
    }

    // Parse method and path
    let parts: Vec<&str> = request_line.trim().split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    // Read headers to find Content-Length
    let mut content_length: usize = 0;
    loop {
        let mut header_line = String::new();
        if reader.read_line(&mut header_line).is_err() {
            break;
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((_key, val)) = trimmed.split_once(':') {
            let key = _key.trim().to_lowercase();
            let val = val.trim();
            if key == "content-length" {
                content_length = val.parse().unwrap_or(0);
            }
        }
    }

    // Read body
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        let _ = reader.read_exact(&mut body);
    }

    // Route
    let (status_code, response_body) = if method == "POST" && path == "/upload" {
        handle_upload(&body, storage_dir, packages)
    } else if method == "GET" && path == "/status" {
        (200, serde_json::json!({
            "status": "running",
            "service": "RecoverKit Remote Agent Receiver",
            "version": crate::VERSION,
        }).to_string())
    } else if method == "GET" && path == "/health" {
        (200, r#"{"status":"ok","service":"RecoverKit Remote Agent Receiver"}"#.to_string())
    } else {
        (404, r#"{"error":"not found","endpoints":["POST /upload","GET /status","GET /health"]}"#.to_string())
    };

    let status_text = match status_code {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        500 => "500 Internal Server Error",
        _ => "200 OK",
    };

    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status_text,
        response_body.len(),
        response_body
    );
    let _ = stream.write_all(response.as_bytes());
}

/// Handle a POST /upload request.
fn handle_upload(
    body: &[u8],
    storage_dir: &Path,
    packages: &Arc<Mutex<Vec<ReceivedPackage>>>,
) -> (u16, String) {
    let body_str = match std::str::from_utf8(body) {
        Ok(s) => s,
        Err(e) => {
            return (400, serde_json::json!({"error": format!("invalid UTF-8: {e}")}).to_string());
        }
    };

    let package: ForensicPackage = match serde_json::from_str(body_str) {
        Ok(pkg) => pkg,
        Err(e) => {
            return (400, serde_json::json!({"error": format!("invalid JSON: {e}")}).to_string());
        }
    };

    let pkg_id = format!("{}_{}", package.device_id, now_ms());

    let pkg_path = storage_dir.join(format!("{pkg_id}.json"));
    if let Err(e) = std::fs::write(&pkg_path, body_str) {
        return (500, serde_json::json!({"error": format!("failed to save: {e}")}).to_string());
    }

    let summary = ReceivedPackage {
        id: pkg_id.clone(),
        device_id: package.device_id.clone(),
        device_model: package.device_model.clone(),
        android_version: package.android_version.clone(),
        timestamp: package.timestamp,
        received_at: now_ms(),
        contacts_count: package.contacts.len(),
        calls_count: package.call_logs.len(),
        sms_count: package.sms_messages.len(),
        bookmarks_count: package.browser_bookmarks.len(),
        wifi_count: package.wifi_networks.len(),
        bluetooth_count: package.bluetooth_devices.len(),
        apps_count: package.app_list.len(),
        media_count: package.media_manifest.len(),
        local_path: pkg_path.display().to_string(),
        notes: package.notes.clone(),
    };

    if let Ok(mut pkgs) = packages.lock() {
        pkgs.push(summary.clone());
    }
    save_package_index(storage_dir, packages);

    eprintln!(
        "[remote-agent] Received package {} from {} ({}): {} contacts, {} calls, {} sms",
        pkg_id,
        package.device_id,
        package.device_model.as_deref().unwrap_or("unknown"),
        package.contacts.len(),
        package.call_logs.len(),
        package.sms_messages.len(),
    );

    (200, serde_json::json!({
        "status": "ok",
        "package_id": pkg_id,
        "summary": summary,
    }).to_string())
}

/// Load the package index from disk.
fn load_package_index(storage_dir: &Path) -> Vec<ReceivedPackage> {
    let index_path = storage_dir.join("packages.json");
    if index_path.exists() {
        if let Ok(data) = std::fs::read_to_string(&index_path) {
            if let Ok(pkgs) = serde_json::from_str(&data) {
                return pkgs;
            }
        }
    }
    Vec::new()
}

/// Save the package index to disk.
fn save_package_index(storage_dir: &Path, packages: &Arc<Mutex<Vec<ReceivedPackage>>>) {
    if let Ok(pkgs) = packages.lock() {
        let index_path = storage_dir.join("packages.json");
        if let Ok(json) = serde_json::to_string_pretty(&*pkgs) {
            let _ = std::fs::write(index_path, json);
        }
    }
}

/// Simple millisecond timestamp.
fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Convert a received forensic package into the format used by the
/// existing scan-folder → recover pipeline.
pub fn export_package_to_workspace(
    package_path: &Path,
    workspace_dir: &Path,
) -> Result<String> {
    let data = std::fs::read_to_string(package_path).map_err(|e| {
        CoreError::Adb(format!("cannot read package: {e}"))
    })?;
    let package: ForensicPackage = serde_json::from_str(&data).map_err(|e| {
        CoreError::Adb(format!("invalid package: {e}"))
    })?;

    let out_dir = workspace_dir.join(&package.device_id);
    std::fs::create_dir_all(&out_dir).map_err(|e| {
        CoreError::Adb(format!("cannot create workspace: {e}"))
    })?;

    // 1) Export contacts as VCF
    if !package.contacts.is_empty() {
        let mut vcf = String::new();
        for c in &package.contacts {
            vcf.push_str("BEGIN:VCARD\nVERSION:3.0\n");
            if let Some(name) = &c.name {
                vcf.push_str(&format!("FN:{name}\n"));
            }
            if let Some(phone) = &c.phone {
                vcf.push_str(&format!("TEL:{phone}\n"));
            }
            if let Some(email) = &c.email {
                vcf.push_str(&format!("EMAIL:{email}\n"));
            }
            vcf.push_str("END:VCARD\n\n");
        }
        std::fs::write(out_dir.join("contacts.vcf"), vcf).ok();
    }

    // 2) Export SMS as XML
    if !package.sms_messages.is_empty() {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<smses count=\"");
        xml.push_str(&package.sms_messages.len().to_string());
        xml.push_str("\">\n");
        for sms in &package.sms_messages {
            let msg_type_num = match sms.msg_type.as_str() {
                "inbox" => 1,
                "sent" => 2,
                "draft" => 3,
                "outbox" => 4,
                _ => 1,
            };
            xml.push_str(&format!(
                "  <sms address=\"{}\" body=\"{}\" type=\"{}\" date=\"{}\" />\n",
                xml_escape(&sms.address),
                xml_escape(sms.body.as_deref().unwrap_or("")),
                msg_type_num,
                sms.timestamp,
            ));
        }
        xml.push_str("</smses>\n");
        std::fs::write(out_dir.join("sms_backup.xml"), xml).ok();
    }

    // 3) Export call logs as JSON
    if !package.call_logs.is_empty() {
        let json = serde_json::to_string_pretty(&package.call_logs).unwrap_or_default();
        std::fs::write(out_dir.join("call_log.json"), json).ok();
    }

    // 4) Export browser bookmarks
    if !package.browser_bookmarks.is_empty() {
        let json = serde_json::to_string_pretty(&package.browser_bookmarks).unwrap_or_default();
        std::fs::write(out_dir.join("bookmarks.json"), json).ok();
    }

    // 5) Export WiFi networks
    if !package.wifi_networks.is_empty() {
        let json = serde_json::to_string_pretty(&package.wifi_networks).unwrap_or_default();
        std::fs::write(out_dir.join("wifi_networks.json"), json).ok();
    }

    // 6) Export Bluetooth devices
    if !package.bluetooth_devices.is_empty() {
        let json = serde_json::to_string_pretty(&package.bluetooth_devices).unwrap_or_default();
        std::fs::write(out_dir.join("bluetooth_devices.json"), json).ok();
    }

    // 7) Export app list
    if !package.app_list.is_empty() {
        let json = serde_json::to_string_pretty(&package.app_list).unwrap_or_default();
        std::fs::write(out_dir.join("app_list.json"), json).ok();
    }

    // 8) Copy the full raw package
    std::fs::copy(package_path, out_dir.join("package.json")).ok();

    // 9) Write device info summary
    if let Some(info) = &package.device_info {
        let json = serde_json::to_string_pretty(info).unwrap_or_default();
        std::fs::write(out_dir.join("device_info.json"), json).ok();
    }

    Ok(out_dir.display().to_string())
}

/// Minimal XML escaping for attribute values.
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
