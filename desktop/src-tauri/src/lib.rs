//! RecoverKit desktop â€” Tauri commands wrapping the `recovery_core` engine.

mod icons;
#[macro_use]
mod forensics;
use forensics::*;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use sha2::{Sha256, Digest};

use recovery_core::{
    acquisition::{self, DirectAcquisition},
    account_forensics,
    adb,
    adb::Device,
    apk_analyzer,
    bluetooth_forensics,
    browser_forensics,
    case_file,
    clipboard_forensics,
    crypto,
    db_carver,
    deleted_sms,
    exif_parser,
    fastboot,
    fastboot::{FastbootDevice, PartitionInfo},
    forensic_report,
    keyboard_forensics,
    location_forensics,
    network_forensics,
    power_forensics,
    remote_agent,
    ringtone,
    scanner::{self, ScanReport},
    screen_capture,
    sim_forensics,
    sms_xml,
    sqlite::{self, Analysis, Message, ParseResult},
    thumbnail_recovery,
    txt_export, vcf,
    unified_timeline,
    wifi_forensics,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Cap on messages sent to the UI in one shot (full data stays in state for export).
const UI_MESSAGE_CAP: usize = 3000;

struct AppState {
    last_result: Mutex<Option<ParseResult>>,
}

/// Tracks running screen streams per device serial.
struct StreamManager {
    streams: Mutex<std::collections::HashMap<String, std::sync::Arc<std::sync::atomic::AtomicBool>>>,
}

/// What the UI receives after a parse/recover run.
#[derive(Serialize)]
struct UiRecovery {
    db_path: String,
    schema: String,
    /// SHA-256 fingerprint of the parsed database (forensic integrity).
    sha256: String,
    total_messages: u64,
    recovered_deleted: usize,
    truncated: bool,
    chats: usize,
    contacts: usize,
    media: usize,
    messages: Vec<Message>,
    messages_hidden: usize,
    /// jid â†’ display name (identity resolution for the UI).
    names: Vec<(String, String)>,
    /// Contact book preview (name, number) pairs.
    contacts_list: Vec<(String, String)>,
    /// Statistical summary (insights layer).
    analysis: Analysis,
}

pub(crate) fn run_blocking<F, T>(f: F) -> Result<T, String>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    // Synchronous Tauri commands already execute on a worker thread,
    // so a direct call keeps things simple and avoids nested runtimes.
    Ok(f())
}

#[tauri::command]
fn list_devices() -> Result<Vec<Device>, String> {
    run_blocking(|| adb::AdbClient::default().list_devices().map_err(|e| e.to_string()))?
}

/// List devices in fastboot/bootloader mode (no USB debugging required).
#[tauri::command]
fn list_fastboot_devices() -> Result<Vec<FastbootDevice>, String> {
    run_blocking(|| fastboot::FastbootClient::default().list_devices().map_err(|e| e.to_string()))?
}

/// Get all fastboot variables for a device.
#[tauri::command]
fn fastboot_getvar_all(serial: String) -> Result<Vec<(String, String)>, String> {
    run_blocking(move || {
        fastboot::FastbootClient::default()
            .getvar_all(&serial)
            .map_err(|e| e.to_string())
    })?
}

/// List partitions on a fastboot device.
#[tauri::command]
fn fastboot_list_partitions(serial: String) -> Result<Vec<PartitionInfo>, String> {
    run_blocking(move || {
        fastboot::FastbootClient::default()
            .list_partitions(&serial)
            .map_err(|e| e.to_string())
    })?
}

/// Read a partition from a fastboot device to a local file.
#[tauri::command]
fn fastboot_read_partition(
    app: AppHandle,
    serial: String,
    partition: String,
) -> Result<String, String> {
    let dest = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("partitions")
        .join(&serial);
    run_blocking(move || {
        let output = dest.join(format!("{}.img", partition));
        fastboot::FastbootClient::default()
            .read_partition(&serial, &partition, &output, |_, _| {})
            .map(|_| output.display().to_string())
            .map_err(|e| e.to_string())
    })?
}

/// Boot a custom image (e.g. TWRP) on a fastboot device without flashing.
#[tauri::command]
fn fastboot_boot_image(serial: String, image_path: String) -> Result<String, String> {
    run_blocking(move || {
        fastboot::FastbootClient::default()
            .boot_image(&serial, Path::new(&image_path))
            .map_err(|e| e.to_string())
    })?
}

/// Reboot device from fastboot mode.
#[tauri::command]
fn fastboot_reboot(serial: String, mode: String) -> Result<String, String> {
    run_blocking(move || {
        let fb = fastboot::FastbootClient::default();
        match mode.as_str() {
            "recovery" => fb.reboot_recovery(&serial).map_err(|e| e.to_string()),
            "bootloader" => fb.reboot_bootloader(&serial).map_err(|e| e.to_string()),
            _ => fb.reboot(&serial).map_err(|e| e.to_string()),
        }
        .map(|_| format!("Rebooting to {}...", mode))
    })?
}

/// Reboot an ADB device into bootloader/fastboot mode.
#[tauri::command]
fn reboot_to_bootloader(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let fb = fastboot::FastbootClient::default();
        fb.reboot_bootloader(&serial)
            .map(|_| "Rebooting to bootloader...".to_string())
            .map_err(|e| e.to_string())
    })?
}

/// Start the remote agent HTTP receiver server.
#[tauri::command]
fn remote_agent_start(port: Option<u16>, storage_dir: Option<String>) -> Result<remote_agent::RemoteAgentState, String> {
    run_blocking(move || {
        let p = port.unwrap_or(7799);
        let dir = match storage_dir {
            Some(d) => PathBuf::from(d),
            None => {
                let base = std::env::temp_dir().join("recoverkit").join("remote_agent");
                base
            }
        };
        remote_agent::start_server(p, &dir).map_err(|e| e.to_string())
    })?
}

/// Stop the remote agent HTTP receiver server.
#[tauri::command]
fn remote_agent_stop() -> Result<remote_agent::RemoteAgentState, String> {
    run_blocking(move || {
        Ok::<_, String>(remote_agent::stop_server())
    })?
}

/// Get the current remote agent server status.
#[tauri::command]
fn remote_agent_status(storage_dir: Option<String>) -> Result<remote_agent::RemoteAgentState, String> {
    run_blocking(move || {
        let dir = match storage_dir {
            Some(d) => PathBuf::from(d),
            None => std::env::temp_dir().join("recoverkit").join("remote_agent"),
        };
        Ok::<_, String>(remote_agent::get_server_state(&dir))
    })?
}

/// Export a received forensic package into the workspace for analysis.
#[tauri::command]
fn remote_agent_export(package_path: String, workspace_dir: Option<String>) -> Result<String, String> {
    run_blocking(move || {
        let ws = match workspace_dir {
            Some(d) => PathBuf::from(d),
            None => std::env::temp_dir().join("recoverkit").join("remote_workspace"),
        };
        remote_agent::export_package_to_workspace(Path::new(&package_path), &ws)
            .map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn scan_folder(path: String) -> Result<ScanReport, String> {
    run_blocking(move || scanner::scan_folder(Path::new(&path)).map_err(|e| e.to_string()))?
}

/// Scan a device; with `pull = true` the found artifacts are copied locally.
#[tauri::command]
fn scan_device(
    app: AppHandle,
    serial: String,
    pull: bool,
    include_media: bool,
) -> Result<(ScanReport, Vec<String>), String> {
    let dest = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("pulled")
        .join(&serial);

    let (report, pulled) = run_blocking(move || -> Result<_, String> {
        let adb = adb::AdbClient::default();
        let report = scanner::scan_device(&adb, &serial).map_err(|e| e.to_string())?;
        let mut pulled_local: Vec<String> = Vec::new();
        if pull {
            std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
            let paths = scanner::pull_artifacts(&adb, &serial, &report.artifacts, &dest, include_media, |_, _, _| {})
                .map_err(|e| e.to_string())?;
            pulled_local = paths.iter().map(|p| p.display().to_string()).collect();
        }
        Ok((report, pulled_local))
    })??;
    Ok((report, pulled))
}

/// If `path` is a device path (`/sdcard/...`) pull it locally first and return
/// the local copy; otherwise return it unchanged.
fn localize(app: &AppHandle, serial: &Option<String>, path: &str) -> Result<PathBuf, String> {
    if !path.starts_with('/') {
        return Ok(PathBuf::from(path));
    }
    let serial = serial
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or("This file is still on the phone and no device is selected â€” go to â‘  Connect, then use â¬‡ Scan & pull on step â‘¡")?;
    let local = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("pulled")
        .join(serial)
        .join(path.trim_start_matches('/').replace('/', std::path::MAIN_SEPARATOR_STR));
    let serial = serial.to_string();
    let remote = path.to_string();
    std::fs::create_dir_all(local.parent().unwrap()).map_err(|e| e.to_string())?;
    adb::AdbClient::default()
        .pull(&serial, &remote, &local)
        .map_err(|e| format!("failed to pull {remote} from phone: {e}"))?;
    Ok(local)
}

/// Decrypt a crypt12/14/15 backup. `output` may be empty for an auto path.
/// Device paths (`/sdcard/...`) are pulled from the phone automatically.
#[tauri::command]
fn decrypt_backup(
    app: AppHandle,
    input: String,
    key_file: String,
    output: Option<String>,
    serial: Option<String>,
) -> Result<crypto::DecryptOutcome, String> {
    run_blocking(move || {
        let input = localize(&app, &serial, &input)?;
        let key_file = localize(&app, &serial, &key_file)?;
        let out = match output {
            Some(o) if !o.is_empty() => PathBuf::from(o),
            _ => input.with_extension("decrypted.db"),
        };
        crypto::decrypt_backup(&input, &key_file, &out).map_err(|e| e.to_string())
    })?
}

/// Returns Some(error description) when `bytes` look like shell error text
/// rather than binary key material.
fn looks_like_shell_error(b: &[u8]) -> Option<String> {
    if b.is_empty() {
        return Some("empty output".into());
    }
    let head = String::from_utf8_lossy(&b[..b.len().min(512)]);
    let t = head.trim();
    let first = t.lines().next().unwrap_or("").to_string();
    if t.contains("su:")
        || t.contains("not found")
        || t.contains("denied")
        || t.contains("inaccessible")
        || t.contains("ermission")
        || t.starts_with('/') && t.contains(':')
    {
        return Some(first);
    }
    // Real key material is ~random bytes; error messages are printable text.
    let printable = b.iter().filter(|c| matches!(**c, 0x20..=0x7e | b'\n')).count();
    if b.len() < 512 && printable * 10 > b.len() * 9 {
        return Some(format!("got text, not a key: {:?}", first.chars().take(60).collect::<String>()));
    }
    None
}

/// Fetch the WhatsApp key file from a rooted device via `su -c cat`.
#[tauri::command]
fn fetch_key(app: AppHandle, serial: String, remote_path: Option<String>) -> Result<String, String> {
    let remote = remote_path.unwrap_or_else(|| "/data/data/com.whatsapp/files/key".to_string());
    let keys_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("keys");
    run_blocking(move || -> Result<String, String> {
        let adb = adb::AdbClient::default();
        let cat = format!("cat '{remote}'");
        // Different su flavors take the command differently â€” try the common styles.
        let variants: Vec<Vec<&str>> = vec![
            vec!["su", "-c", &cat],
            vec!["su", "0", "-c", &cat],
            vec!["su", "root", "-c", &cat],
        ];
        let mut last_err = String::from("no output from su");
        for args in &variants {
            match adb.exec_out(&serial, args) {
                Ok(bytes) => {
                    if let Some(err) = looks_like_shell_error(&bytes) {
                        last_err = err;
                        continue;
                    }
                    if bytes.len() < 128 {
                        last_err = format!("key file too small ({} bytes)", bytes.len());
                        continue;
                    }
                    std::fs::create_dir_all(&keys_dir).map_err(|e| e.to_string())?;
                    let local = keys_dir.join("whatsapp.key");
                    std::fs::write(&local, &bytes).map_err(|e| e.to_string())?;
                    return Ok(local.display().to_string());
                }
                Err(e) => last_err = e.to_string(),
            }
        }
        // Diagnostic: what does su actually answer?
        let diag = adb
            .exec_out(&serial, &["su", "-c", "id"])
            .map(|b| String::from_utf8_lossy(&b).trim().to_string())
            .unwrap_or_default();
        Err(format!(
            "root access not working â€” su said: '{}'. If a superuser prompt appeared on the phone, tap GRANT and retry. \
             On custom ROMs also check Developer options â†’ Root access â†’ 'Apps and ADB'.",
            if diag.is_empty() { last_err } else { diag }
        ))
    })?
}

/// Streamed progress for the media step so the UI fills up live.
#[derive(Clone, Serialize)]
struct MediaProgress {
    /// "scan" or "pull"
    phase: &'static str,
    folder: String,
    index: usize,
    total: usize,
    /// All files copied locally so far (grows folder by folder).
    files: Vec<scanner::MediaFile>,
}

/// Recover images & videos from the phone. `source` selects the scope:
/// `whatsapp` (default), `gallery` (DCIM/Pictures/Movies â€” any app) or
/// `downloads`. Lists read-only, then pulls the relevant folders locally.
#[tauri::command]
fn recover_media(
    app: AppHandle,
    serial: String,
    source: Option<String>,
) -> Result<scanner::MediaRecovery, String> {
    run_blocking(move || -> Result<scanner::MediaRecovery, String> {
        let adb = adb::AdbClient::default();
        let _ = app.emit(
            "media-progress",
            MediaProgress { phase: "scan", folder: "Scanning the phoneâ€¦".into(), index: 0, total: 0, files: Vec::new() },
        );
        let roots: &[&str] = match source.as_deref().unwrap_or("whatsapp") {
            "gallery" => scanner::DEVICE_GALLERY_ROOTS,
            "downloads" => scanner::DEVICE_DOWNLOAD_ROOTS,
            _ => scanner::DEVICE_MEDIA_ROOTS,
        };
        let files = scanner::scan_media_paths(&adb, &serial, roots).map_err(|e| e.to_string())?;
        if files.is_empty() {
            return Err("No images or videos found in those phone folders.".into());
        }
        let dest_root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("pulled")
            .join(&serial);

        // Group by top-level subfolder under each root and pull each whole
        // folder with a single fast `adb pull`.
        let mut dirs: BTreeSet<String> = BTreeSet::new();
        for f in &files {
            for root in roots {
                if let Some(rest) = f.path.strip_prefix(root) {
                    let sub = rest.trim_start_matches('/').split('/').next().unwrap_or("");
                    // Skip thumbnail caches and temp send copies.
                    if sub.starts_with('.') || sub == "Sent" {
                        // Files directly under the root still get pulled below.
                        dirs.insert(root.to_string());
                        continue;
                    }
                    if sub.is_empty() {
                        dirs.insert(root.to_string());
                    } else {
                        dirs.insert(format!("{root}/{sub}"));
                    }
                }
            }
        }
        let (mut pulled_dirs, mut failed_dirs) = (0, 0);
        let total = dirs.len();
        for (i, dir) in dirs.iter().enumerate() {
            let rel = dir.trim_start_matches('/');
            let short = rel.split('/').last().unwrap_or(rel).to_string();
            let local = dest_root.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
            std::fs::create_dir_all(&local).map_err(|e| e.to_string())?;
            if adb.pull(&serial, dir, &local).is_ok() {
                pulled_dirs += 1;
            } else {
                failed_dirs += 1;
            }
            // Stream the gallery to the UI folder by folder.
            let _ = app.emit(
                "media-progress",
                MediaProgress {
                    phase: "pull",
                    folder: short,
                    index: i + 1,
                    total,
                    files: scanner::scan_media_folder(&dest_root),
                },
            );
        }
        let files = scanner::scan_media_folder(&dest_root);
        Ok(scanner::MediaRecovery { files, pulled_dirs, failed_dirs })
    })?
}

/// Current on-device presence of every media file (read-only `ls`).
/// Comparing it against the recovered local copies proves which files are
/// still on the phone and which are lost from it (local copy = only evidence).
#[tauri::command]
async fn device_presence(serial: String) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<String>, String> {
        let adb = adb::AdbClient::default();
        let mut roots: Vec<&str> = scanner::DEVICE_MEDIA_ROOTS.to_vec();
        roots.extend_from_slice(scanner::DEVICE_GALLERY_ROOTS);
        roots.extend_from_slice(scanner::DEVICE_DOWNLOAD_ROOTS);
        let files = scanner::scan_media_paths(&adb, &serial, &roots).map_err(|e| e.to_string())?;
        Ok(files.into_iter().map(|f| f.path).collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Copy all recovered media into a user-chosen folder, keeping the folder
/// structure from the phone's `WhatsApp` directory onwards.
#[tauri::command]
fn save_media(app: AppHandle, serial: String, dest: String) -> Result<String, String> {
    run_blocking(move || -> Result<String, String> {
        let root = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("pulled")
            .join(&serial);
        let files = scanner::scan_media_folder(&root);
        if files.is_empty() {
            return Err("No recovered media to save yet â€” click \u{2b07} Recover media first.".into());
        }
        let dest = PathBuf::from(&dest);
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        // Copy in parallel across all CPU cores â€” thousands of files finish
        // in a fraction of the sequential time.
        use rayon::prelude::*;
        let saved: usize = files
            .par_iter()
            .filter_map(|f| {
                let src = PathBuf::from(&f.path);
                let comps: Vec<String> = src
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                let sub: PathBuf = match comps.iter().position(|c| c.eq_ignore_ascii_case("whatsapp")) {
                    Some(i) => comps[i..].iter().collect(),
                    // Gallery/downloads pulls: keep everything after the serial folder.
                    None => match comps.iter().position(|c| c == &serial) {
                        Some(i) => comps[i + 1..].iter().collect(),
                        None => PathBuf::from(src.file_name().unwrap_or_default()),
                    },
                };
                let out = dest.join(sub);
                if let Some(p) = out.parent() {
                    let _ = std::fs::create_dir_all(p);
                }
                std::fs::copy(&src, &out).ok()
            })
            .count();
        Ok(format!("{saved} media files saved to {}", dest.display()))
    })?
}

/// Live device telemetry for the mirror panel (all read-only ADB).
#[derive(Serialize)]
struct DeviceMirror {
    model: String,
    android: String,
    battery: String,
    storage: String,
    resolution: String,
    uptime: String,
    whatsapp: String,
}

fn fmt_kb(kb: u64) -> String {
    let gb = kb as f64 / (1024.0 * 1024.0);
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{:.0} MB", kb as f64 / 1024.0)
    }
}

fn prop(adb: &adb::AdbClient, serial: &str, key: &str) -> String {
    adb.exec_out(serial, &["getprop", key])
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[tauri::command]
fn device_mirror(serial: String) -> Result<DeviceMirror, String> {
    run_blocking(move || -> Result<DeviceMirror, String> {
        let adb = adb::AdbClient::default();
        let model = prop(&adb, &serial, "ro.product.model");
        let android = prop(&adb, &serial, "ro.build.version.release");

        // Battery: `dumpsys battery` â†’ "level: 87", "USB powered: true"
        let battery = adb
            .exec_out(&serial, &["dumpsys", "battery"])
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .map(|s| {
                let level = s
                    .lines()
                    .find_map(|l| l.trim().strip_prefix("level:"))
                    .map(str::trim)
                    .unwrap_or("?");
                let charging = s.lines().any(|l| {
                    let t = l.trim();
                    t.starts_with("AC powered: true") || t.starts_with("USB powered: true")
                });
                format!("{level}%{}", if charging { " \u{26a1}" } else { "" })
            })
            .unwrap_or_else(|| "unknown".into());

        // Storage: `df /sdcard` second line â†’ total / available (KB)
        let storage = adb
            .exec_out(&serial, &["df", "/sdcard"])
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| s.lines().nth(1).map(|l| l.split_whitespace().map(String::from).collect::<Vec<_>>()))
            .and_then(|cols| {
                if cols.len() >= 4 {
                    let total = cols[1].parse::<u64>().ok()?;
                    let avail = cols[3].parse::<u64>().ok()?;
                    Some(format!("{} free of {}", fmt_kb(avail), fmt_kb(total)))
                } else {
                    None
                }
            })
            .unwrap_or_else(|| "unknown".into());

        // Screen: `wm size`
        let resolution = adb
            .exec_out(&serial, &["wm", "size"])
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("Physical size:").map(|v| v.trim().to_string()))
            })
            .unwrap_or_else(|| "unknown".into());

        // Uptime: /proc/uptime first field (seconds)
        let uptime = adb
            .exec_out(&serial, &["cat", "/proc/uptime"])
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()))
            .map(|secs| {
                let h = (secs / 3600.0) as u64;
                let m = ((secs % 3600.0) / 60.0) as u64;
                format!("{h}h {m}m")
            })
            .unwrap_or_else(|| "unknown".into());

        // WhatsApp version if installed
        let whatsapp = adb
            .exec_out(&serial, &["dumpsys", "package", "com.whatsapp"])
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.trim().strip_prefix("versionName=").map(|v| v.to_string()))
            })
            .unwrap_or_else(|| "not installed".into());

        Ok(DeviceMirror { model, android, battery, storage, resolution, uptime, whatsapp })
    })?
}

/// Comprehensive device information â€” 50+ fields collected via ADB.
#[derive(Serialize, Clone)]
struct DeviceFullInfo {
    // Identity
    manufacturer: String,
    brand: String,
    model: String,
    device_name: String,
    board: String,
    hardware: String,
    serial_no: String,
    bootloader: String,
    baseband: String,
    // OS
    android_version: String,
    sdk_level: String,
    security_patch: String,
    build_id: String,
    build_type: String,
    build_fingerprint: String,
    build_date: String,
    // CPU
    cpu_abi: String,
    cpu_abi2: String,
    cpu_cores: String,
    cpu_max_freq: String,
    // Display
    resolution: String,
    density: String,
    refresh_rate: String,
    // Memory & Storage
    total_ram: String,
    available_ram: String,
    internal_storage: String,
    sd_card: String,
    // Battery
    battery_level: String,
    battery_status: String,
    battery_temp: String,
    battery_voltage: String,
    battery_health: String,
    battery_technology: String,
    // Network
    wifi_state: String,
    wifi_ssid: String,
    ip_address: String,
    mac_address: String,
    // SIM
    sim_operator: String,
    sim_carrier: String,
    sim_country: String,
    sim_state: String,
    // System
    language: String,
    timezone: String,
    encryption: String,
    uptime: String,
    // Apps
    total_apps: String,
    user_apps: String,
    // USB
    usb_state: String,
    adb_version: String,
}

fn shell_str(adb: &adb::AdbClient, serial: &str, cmd: &[&str]) -> String {
    adb.exec_out(serial, cmd)
        .ok()
        .and_then(|b| String::from_utf8(b).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

#[tauri::command]
fn device_full_info(serial: String) -> Result<DeviceFullInfo, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let s = &serial;

        // Identity
        let manufacturer = prop(&adb, s, "ro.product.manufacturer");
        let brand = prop(&adb, s, "ro.product.brand");
        let model = prop(&adb, s, "ro.product.model");
        let device_name = prop(&adb, s, "ro.product.device");
        let board = prop(&adb, s, "ro.product.board");
        let hardware = prop(&adb, s, "ro.hardware");
        let serial_no = shell_str(&adb, s, &["getprop", "ro.serialno"]);
        let bootloader = prop(&adb, s, "ro.bootloader");
        let baseband = prop(&adb, s, "gsm.version.baseband");

        // OS
        let android_version = prop(&adb, s, "ro.build.version.release");
        let sdk_level = prop(&adb, s, "ro.build.version.sdk");
        let security_patch = prop(&adb, s, "ro.build.version.security_patch");
        let build_id = prop(&adb, s, "ro.build.id");
        let build_type = prop(&adb, s, "ro.build.type");
        let build_fingerprint = prop(&adb, s, "ro.build.fingerprint");
        let build_date = prop(&adb, s, "ro.build.date");

        // CPU
        let cpu_abi = prop(&adb, s, "ro.product.cpu.abi");
        let cpu_abi2 = prop(&adb, s, "ro.product.cpu.abilist");
        let cpu_cores = shell_str(&adb, s, &["nproc"]);
        let cpu_max_freq = shell_str(&adb, s, &["cat", "/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq"]);
        let cpu_max_freq = if !cpu_max_freq.is_empty() {
            cpu_max_freq.parse::<u64>().map(|f| format!("{} MHz", f / 1000)).unwrap_or(cpu_max_freq)
        } else { String::new() };

        // Display
        let resolution = shell_str(&adb, s, &["wm", "size"])
            .lines().find_map(|l| l.strip_prefix("Physical size:").map(|v| v.trim().to_string()))
            .unwrap_or_default();
        let density = shell_str(&adb, s, &["wm", "density"])
            .lines().find_map(|l| l.strip_prefix("Physical density:").map(|v| v.trim().to_string()))
            .unwrap_or_default();
        let refresh_rate = shell_str(&adb, s, &["dumpsys", "display"])
            .lines().find_map(|l| l.trim().strip_prefix("mBaseDisplayInfo=").and_then(|v| {
                v.split("fps").next().and_then(|pre| pre.rsplit(" ").next().map(|f| format!("{} fps", f.trim())))
            }))
            .unwrap_or_default();

        // Memory
        let meminfo = shell_str(&adb, s, &["cat", "/proc/meminfo"]);
        let total_ram = meminfo.lines().find_map(|l| l.strip_prefix("MemTotal:"))
            .map(|v| v.trim().split_whitespace().next().unwrap_or("0"))
            .and_then(|kb| kb.parse::<u64>().ok())
            .map(|kb| if kb > 1048576 { format!("{:.1} GB", kb as f64 / 1048576.0) } else { format!("{} MB", kb / 1024) })
            .unwrap_or_default();
        let available_ram = meminfo.lines().find_map(|l| l.strip_prefix("MemAvailable:"))
            .map(|v| v.trim().split_whitespace().next().unwrap_or("0"))
            .and_then(|kb| kb.parse::<u64>().ok())
            .map(|kb| if kb > 1048576 { format!("{:.1} GB", kb as f64 / 1048576.0) } else { format!("{} MB", kb / 1024) })
            .unwrap_or_default();

        // Storage
        let df_out = shell_str(&adb, s, &["df", "-h"]);
        let internal_storage = df_out.lines().find(|l| l.contains("/data") && !l.contains("/data/media"))
            .map(|l| { let c: Vec<&str> = l.split_whitespace().collect(); if c.len() >= 4 { format!("{} free / {}", c[3], c[1]) } else { String::new() } })
            .unwrap_or_default();
        let sd_card = df_out.lines().find(|l| l.contains("/sdcard") || l.contains("/storage/emulated"))
            .map(|l| { let c: Vec<&str> = l.split_whitespace().collect(); if c.len() >= 4 { format!("{} free / {}", c[3], c[1]) } else { String::new() } })
            .unwrap_or_default();

        // Battery
        let batt = shell_str(&adb, s, &["dumpsys", "battery"]);
        let battery_level = batt.lines().find_map(|l| l.trim().strip_prefix("level:")).map(|v| format!("{}%", v.trim())).unwrap_or_default();
        let battery_status = batt.lines().find_map(|l| l.trim().strip_prefix("status:"))
            .map(|v| match v.trim() { "1" => "Unknown", "2" => "Charging", "3" => "Discharging", "4" => "Not charging", "5" => "Full", _ => v.trim() })
            .map(String::from).unwrap_or_default();
        let batt_temp = shell_str(&adb, s, &["dumpsys", "battery", "--check"]);
        let battery_temp = shell_str(&adb, s, &["cat", "/sys/class/power_supply/battery/temp"]);
        let battery_temp = if !battery_temp.is_empty() {
            battery_temp.parse::<f64>().map(|t| format!("{:.1}Â°C", t / 10.0)).unwrap_or_default()
        } else { String::new() };
        let battery_voltage = shell_str(&adb, s, &["cat", "/sys/class/power_supply/battery/voltage_now"]);
        let battery_voltage = if !battery_voltage.is_empty() {
            battery_voltage.parse::<u64>().map(|v| format!("{:.2} V", v as f64 / 1000000.0)).unwrap_or_default()
        } else { String::new() };
        let battery_health = shell_str(&adb, s, &["cat", "/sys/class/power_supply/battery/health"]);
        let battery_technology = shell_str(&adb, s, &["cat", "/sys/class/power_supply/battery/technology"]);

        // Network
        let wifi_state = shell_str(&adb, s, &["dumpsys", "wifi"])
            .lines().find_map(|l| l.trim().strip_prefix("Wi-Fi is").map(|v| v.trim().to_string()))
            .unwrap_or_default();
        let ip_address = shell_str(&adb, s, &["ip", "route"])
            .lines().find_map(|l| if l.contains("src") { l.split("src").nth(1).and_then(|v| v.split_whitespace().next().map(String::from)) } else { None })
            .unwrap_or_default();
        let mac_address = shell_str(&adb, s, &["cat", "/sys/class/net/wlan0/address"]);

        // SIM
        let sim_operator = prop(&adb, s, "gsm.sim.operator.alpha");
        let sim_carrier = prop(&adb, s, "gsm.operator.alpha");
        let sim_country = prop(&adb, s, "gsm.sim.operator.iso-country");
        let sim_state = prop(&adb, s, "gsm.sim.state");

        // System
        let language = prop(&adb, s, "persist.sys.locale");
        let timezone = prop(&adb, s, "persist.sys.timezone");
        let encryption = prop(&adb, s, "ro.crypto.state");
        let uptime = shell_str(&adb, s, &["cat", "/proc/uptime"])
            .split_whitespace().next()
            .and_then(|v| v.parse::<f64>().ok())
            .map(|secs| { let d = (secs / 86400.0) as u64; let h = ((secs % 86400.0) / 3600.0) as u64; let m = ((secs % 3600.0) / 60.0) as u64;
                if d > 0 { format!("{d}d {h}h {m}m") } else { format!("{h}h {m}m") } })
            .unwrap_or_default();

        // Apps
        let pm_list = shell_str(&adb, s, &["pm", "list", "packages"]);
        let total_apps = pm_list.lines().count().to_string();
        let user_apps = shell_str(&adb, s, &["pm", "list", "packages", "-3"]).lines().count().to_string();

        // USB
        let usb_state = shell_str(&adb, s, &["getprop", "sys.usb.state"]);
        let adb_version = shell_str(&adb, s, &["version"])
            .lines().next().unwrap_or("").to_string();

        Ok(DeviceFullInfo {
            manufacturer, brand, model, device_name, board, hardware, serial_no, bootloader, baseband,
            android_version, sdk_level, security_patch, build_id, build_type, build_fingerprint, build_date,
            cpu_abi, cpu_abi2, cpu_cores, cpu_max_freq,
            resolution, density, refresh_rate,
            total_ram, available_ram, internal_storage, sd_card,
            battery_level, battery_status, battery_temp, battery_voltage, battery_health, battery_technology,
            wifi_state, wifi_ssid: String::new(), ip_address, mac_address,
            sim_operator, sim_carrier, sim_country, sim_state,
            language, timezone, encryption, uptime,
            total_apps, user_apps,
            usb_state, adb_version,
        })
    })?
}

#[derive(Serialize)]
struct ScreenshotResult {
    path: String,
    /// True when the frame is almost certainly a black/locked screen
    /// (uniform frames compress to a tiny PNG).
    dark: bool,
}

/// One screen snapshot (read-only `screencap`), saved locally for the
/// mirror simulation. Returns the PNG path plus a dark-frame flag.
#[tauri::command]
fn device_screenshot(app: AppHandle, serial: String) -> Result<ScreenshotResult, String> {
    run_blocking(move || -> Result<ScreenshotResult, String> {
        let adb = adb::AdbClient::default();
        // Wake the display first (KEYCODE_WAKEUP) so the lock surface renders;
        // this only lights the screen and never touches user data.
        let _ = adb.exec_out(&serial, &["input", "keyevent", "224"]);
        std::thread::sleep(std::time::Duration::from_millis(450));
        let bytes = adb
            .exec_out(&serial, &["screencap", "-p"])
            .map_err(|e| e.to_string())?;
        if bytes.len() < 100 || !bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
            return Err("Could not capture the screen â€” unlock the phone and re-allow USB debugging.".into());
        }
        let path = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("mirror")
            .join(format!("{serial}.png"));
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(ScreenshotResult {
            path: path.display().to_string(),
            dark: bytes.len() < 60_000,
        })
    })?
}

/// Play a ringtone on the connected device to physically locate it.
#[tauri::command]
async fn play_ringtone(serial: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::play_ringtone(&adb, &serial).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

/// Start a looping ringtone that plays until explicitly stopped.
#[tauri::command]
async fn start_ringtone_loop(serial: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::start_ringtone_loop(&adb, &serial).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

/// Stop the looping ringtone by creating a stop-flag on the device.
#[tauri::command]
async fn stop_ringtone(serial: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::stop_ringtone(&adb, &serial).map_err(|e| e.to_string())
    }).await.map_err(|e| e.to_string())?
}

/// Lock down the device: hide navigation + status bars, block user interaction.
/// General-purpose feature â€” usable during ringtone, extraction, etc.
#[tauri::command]
async fn lockdown_device(serial: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::lock_device(&adb, &serial);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// Release the device lockdown â€” restore navigation + status bars.
#[tauri::command]
async fn release_device(serial: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::unlock_device(&adb, &serial);
        Ok(())
    }).await.map_err(|e| e.to_string())?
}

/// Stop device charging (freeze battery as unplugged).
#[tauri::command]
fn stop_charging(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::stop_charging(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Start/resume device charging (reset battery to hardware values).
#[tauri::command]
fn start_charging(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::start_charging(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Get current battery status from the device.
#[tauri::command]
fn get_battery_status(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::get_battery_status(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect WiFi network history from the device.
#[tauri::command]
fn collect_wifi(serial: String) -> Result<Vec<wifi_forensics::WifiNetwork>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        wifi_forensics::collect_wifi_networks(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect Bluetooth paired devices from the device.
#[tauri::command]
fn collect_bluetooth(serial: String) -> Result<Vec<bluetooth_forensics::BluetoothDevice>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        bluetooth_forensics::collect_bluetooth_devices(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect registered accounts from the device.
#[tauri::command]
fn collect_accounts(serial: String) -> Result<Vec<account_forensics::DeviceAccount>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        account_forensics::collect_accounts(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect power/battery events from the device.
#[tauri::command]
fn collect_power_events(serial: String) -> Result<Vec<power_forensics::PowerEvent>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        power_forensics::collect_power_events(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect notification log from the device.
#[tauri::command]
fn collect_notifications(serial: String) -> Result<Vec<clipboard_forensics::NotificationEntry>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        clipboard_forensics::collect_notifications(&adb, &serial).map_err(|e| e.to_string())
    })?
}

/// Collect user dictionary words from the device.
#[tauri::command]
fn collect_dictionary(serial: String) -> Result<Vec<keyboard_forensics::DictionaryWord>, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        keyboard_forensics::collect_dictionary(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn recover_deleted_sms(serial: String) -> Result<deleted_sms::DeletedSmsResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        deleted_sms::recover_deleted_sms(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn recover_thumbnails(serial: String) -> Result<thumbnail_recovery::ThumbnailRecoveryResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        thumbnail_recovery::recover_thumbnails(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn carve_device_db(serial: String, db_path: String, tables: Vec<String>) -> Result<db_carver::CarveResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let table_refs: Vec<&str> = tables.iter().map(|s| s.as_str()).collect();
        db_carver::pull_and_carve(&adb, &serial, &db_path, &table_refs).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn collect_browser_data(serial: String) -> Result<browser_forensics::BrowserForensicsResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        browser_forensics::collect_browser_data(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn collect_network_artifacts(serial: String) -> Result<network_forensics::NetworkForensicsResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        network_forensics::collect_network_artifacts(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn analyze_installed_apks(serial: String) -> Result<apk_analyzer::ApkAnalysisResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        apk_analyzer::analyze_apks(&adb, &serial, &[]).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn collect_location_data(serial: String) -> Result<location_forensics::LocationForensicsResult, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        location_forensics::collect_location_data(&adb, &serial).map_err(|e| e.to_string())
    })?
}

#[tauri::command]
fn build_timeline(
    calls: Vec<(i64, String, String)>,
    sms: Vec<(i64, String, String, bool)>,
    whatsapp: Vec<(i64, String, String, bool)>,
    power: Vec<(i64, String, u8)>,
    notifications: Vec<(i64, String, String)>,
    browser: Vec<(i64, String, String)>,
    location: Vec<(i64, f64, f64, String)>,
    serial: Option<String>,
) -> Result<Vec<unified_timeline::UnifiedEvent>, String> {
    run_blocking(move || {
        let input = unified_timeline::TimelineInput {
            calls, sms, whatsapp, power_events: power, notifications,
            browser_history: browser, location_events: location,
            ..Default::default()
        };
        Ok(unified_timeline::build_unified_timeline(&input, serial.as_deref()))
    })?
}

#[tauri::command]
fn generate_report(
    serial: String,
    model: Option<String>,
    sources: Vec<forensic_report::SourceReport>,
    events: Vec<unified_timeline::UnifiedEvent>,
) -> Result<String, String> {
    run_blocking(move || {
        let report = forensic_report::build_report(&serial, model.as_deref(), sources, &events);
        Ok(forensic_report::generate_html_report(&report, &events))
    })?
}

/// Start a high-frequency screen stream for live viewing.
/// `mode` = "scrcpy" uses scrcpy H.264 hardware encoding (30-60 fps, ~35-70ms latency),
/// "fast" uses screenrecord (30-60 fps), "normal" uses screencap (5-30 fps).
/// Emits frames via Tauri events. Returns immediately; stream runs in background.
#[tauri::command]
fn start_screen_stream(app: AppHandle, serial: String, fps: u32, mode: String, state: tauri::State<StreamManager>) -> Result<(), String> {
    // Stop any existing stream for this serial
    {
        let streams = state.streams.lock().unwrap();
        if let Some(existing) = streams.get(&serial) {
            existing.store(false, std::sync::atomic::Ordering::Relaxed);
        }
    }

    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    {
        let mut streams = state.streams.lock().unwrap();
        streams.insert(serial.clone(), running.clone());
    }
    let running_clone = running.clone();
    let app_clone = app.clone();
    let serial_for_thread = serial.clone();

    std::thread::spawn(move || {
        let adb = adb::AdbClient::default();
        let s = &serial_for_thread;
        let mirror_dir = match app_clone.path().app_data_dir() {
            Ok(dir) => dir.join("mirror"),
            Err(e) => {
                eprintln!("Failed to get app data dir: {}", e);
                return;
            }
        };
        if let Err(e) = std::fs::create_dir_all(&mirror_dir) {
            eprintln!("Failed to create mirror dir: {}", e);
            return;
        }

        // Wake display once
        let _ = adb.exec_out(s, &["input", "keyevent", "224"]);
        std::thread::sleep(std::time::Duration::from_millis(300));

        if mode == "scrcpy" {
            // SCRCPY MODE: Launch scrcpy.exe — true real-time hardware-accelerated mirror
            eprintln!("[scrcpy] Starting at {} fps for device {}", fps, serial_for_thread);

            // 1. Resolve scrcpy.exe — check multiple locations:
            //    - Bundled resources (production): <exe_dir>/resources/scrcpy/
            //    - Source directory (dev): src-tauri/resources/scrcpy/
            let scrcpy_dir = {
                let mut candidates = Vec::new();
                // Bundled resource path (production builds)
                if let Ok(res) = app_clone.path().resource_dir() {
                    candidates.push(res.join("scrcpy"));
                }
                // Dev path: src-tauri/resources/scrcpy/ (relative to Cargo.toml)
                if let Ok(cwd) = std::env::current_dir() {
                    candidates.push(cwd.join("resources").join("scrcpy"));
                }
                // Also try manifest_dir
                if let Ok(dir) = std::env::var("CARGO_MANIFEST_DIR") {
                    candidates.push(std::path::Path::new(&dir).join("resources").join("scrcpy"));
                }
                candidates.into_iter().find(|d| d.join("scrcpy.exe").exists())
            };

            let scrcpy_dir = match scrcpy_dir {
                Some(d) => {
                    eprintln!("[scrcpy] Found at {:?}", d);
                    d
                }
                None => {
                    let msg = "scrcpy.exe not found. Searched resource_dir/scrcpy, src-tauri/resources/scrcpy";
                    eprintln!("[scrcpy] ERROR: {}", msg);
                    let _ = app_clone.emit("screen-frame", serde_json::json!({
                        "serial": serial_for_thread,
                        "path": "",
                        "frame": 0,
                        "timestamp": 0u64,
                        "mode": "scrcpy-error",
                        "isVideo": false,
                        "error": msg
                    }));
                    return;
                }
            };
            let scrcpy_exe = scrcpy_dir.join("scrcpy.exe");

            // 2. Wake display
            let _ = adb.exec_out(s, &["input", "keyevent", "224"]);

            // 3. Spawn scrcpy.exe — opens its own native SDL3 window with
            //    hardware-accelerated rendering, FFmpeg decode, and input control
            eprintln!("[scrcpy] Spawning: {:?} --serial {} --max-size 720 --max-fps {}", scrcpy_exe, s, fps);
            let child_result = std::process::Command::new(&scrcpy_exe)
                .args(&[
                    "--serial", s,
                    "--max-size", "720",
                    "--max-fps", &fps.to_string(),
                    "--bit-rate", "2000000",
                    "--no-audio",
                    "--window-title", &format!("RecoverKit Mirror — {}", s),
                ])
                .current_dir(&scrcpy_dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn();

            match child_result {
                Ok(mut child) => {
                    let scrcpy_pid = child.id();
                    eprintln!("[scrcpy] Launched successfully (PID {})", scrcpy_pid);

                    // Capture stderr in a background thread for diagnostics
                    if let Some(stderr) = child.stderr.take() {
                        let serial_clone = serial_for_thread.clone();
                        std::thread::spawn(move || {
                            use std::io::BufRead;
                            let reader = std::io::BufReader::new(stderr);
                            for line in reader.lines().flatten() {
                                eprintln!("[scrcpy:{}] {}", serial_clone, line);
                            }
                        });
                    }

                    // Notify frontend that scrcpy is running
                    let _ = app_clone.emit("screen-frame", serde_json::json!({
                        "serial": serial_for_thread,
                        "path": "",
                        "frame": 0,
                        "timestamp": 0u64,
                        "mode": "scrcpy",
                        "isVideo": false,
                        "scrcpyPid": scrcpy_pid
                    }));

                    // Wait for stop signal or process exit
                    loop {
                        if !running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                            eprintln!("[scrcpy] Stopping (kill PID {})", scrcpy_pid);
                            let _ = child.kill();
                            break;
                        }
                        match child.try_wait() {
                            Ok(Some(status)) => {
                                eprintln!("[scrcpy] Process exited: {}", status);
                                let _ = app_clone.emit("screen-frame", serde_json::json!({
                                    "serial": serial_for_thread,
                                    "path": "",
                                    "frame": 0,
                                    "timestamp": 0u64,
                                    "mode": "scrcpy-exited",
                                    "isVideo": false
                                }));
                                break;
                            }
                            Ok(None) => {} // still running
                            Err(e) => {
                                eprintln!("[scrcpy] Wait error: {}", e);
                                break;
                            }
                        }
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
                Err(e) => {
                    let msg = format!("Failed to launch scrcpy.exe: {}", e);
                    eprintln!("[scrcpy] ERROR: {}", msg);
                    let _ = app_clone.emit("screen-frame", serde_json::json!({
                        "serial": serial_for_thread,
                        "path": "",
                        "frame": 0,
                        "timestamp": 0u64,
                        "mode": "scrcpy-error",
                        "isVideo": false,
                        "error": msg
                    }));
                }
            }
            eprintln!("[scrcpy] Mode ended");
            return;
        }

        if mode == "fast" {
            // HIGH-FPS MODE: Use screenrecord for 30-60 fps
            eprintln!("Starting high-FPS screenrecord mode at {} fps", fps);
            
            let clip_duration = 1; // 1-second clips for lower latency
            let remote_path = "/sdcard/recoverkit_stream.mp4";
            let mut clip_count = 0u64;
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let record_cmd = format!(
                    "screenrecord --time-limit {} --bit-rate 4000000 --size 720x1280 {}",
                    clip_duration, remote_path
                );
                
                match adb.exec_out(s, &["sh", "-c", &record_cmd]) {
                    Ok(_) => {
                        let local_clip = mirror_dir.join(format!("clip_{}.mp4", clip_count % 3));
                        if adb.pull(s, remote_path, &local_clip).is_ok() {
                            let _ = app_clone.emit(
                                "screen-frame",
                                serde_json::json!({
                                    "serial": serial_for_thread,
                                    "path": local_clip.display().to_string(),
                                    "frame": clip_count,
                                    "timestamp": std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64,
                                    "mode": "fast",
                                    "isVideo": true
                                }),
                            );
                            clip_count += 1;
                        }
                        let _ = adb.exec_out(s, &["rm", "-f", remote_path]);
                    }
                    Err(e) => {
                        eprintln!("screenrecord failed: {}", e);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
        } else {
            // NORMAL MODE: Use screencap — optimized for minimum latency
            let interval_ms = 1000 / fps.max(1);
            let mut frame_count = 0u64;
            // Use 2 alternating file paths — keeps browser asset cache warm
            let frame_a = mirror_dir.join(format!("{}_stream_a.png", serial_for_thread));
            let frame_b = mirror_dir.join(format!("{}_stream_b.png", serial_for_thread));
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let start = std::time::Instant::now();

                match adb.exec_out(s, &["screencap", "-p"]) {
                    Ok(bytes) if bytes.len() > 100 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => {
                        // Alternate between 2 fixed paths
                        let frame_path = if frame_count % 2 == 0 { &frame_a } else { &frame_b };
                        if std::fs::write(frame_path, &bytes).is_ok() {
                            let _ = app_clone.emit(
                                "screen-frame",
                                serde_json::json!({
                                    "serial": serial_for_thread,
                                    "path": frame_path.display().to_string(),
                                    "frame": frame_count,
                                    "timestamp": std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .unwrap_or_default()
                                        .as_millis() as u64,
                                    "mode": "normal",
                                    "isVideo": false
                                }),
                            );
                            frame_count += 1;
                        }
                    }
                    Err(e) => {
                        eprintln!("Screen capture failed: {}", e);
                    }
                    _ => {}
                }

                let elapsed = start.elapsed().as_millis() as u64;
                if elapsed < interval_ms as u64 {
                    std::thread::sleep(std::time::Duration::from_millis(interval_ms as u64 - elapsed));
                }
            }
        }
    });

    Ok(())
}

/// Stop the screen stream for a device.
#[tauri::command]
async fn stop_screen_stream(serial: String, state: tauri::State<'_, StreamManager>) -> Result<(), String> {
    let streams = state.streams.lock().unwrap();
    if let Some(running) = streams.get(&serial) {
        running.store(false, std::sync::atomic::Ordering::Relaxed);
        eprintln!("Stopped stream for {}", serial);
    }
    Ok(())
}

/// One installed app + its configuration, as far as non-root ADB allows.
#[derive(Serialize)]
struct AppInfo {
    package: String,
    apk_path: String,
    /// Installed under /system, /product, /vendor, â€¦ (preloaded firmware).
    system: bool,
    version_name: Option<String>,
    version_code: Option<String>,
    target_sdk: Option<String>,
    first_install: String,
    last_update: String,
    data_dir: Option<String>,
    signer: Option<String>,
    perms: usize,
}

/// Inventory of every installed app (read-only `pm list packages -f`).
/// Async + on the blocking pool so the UI stays responsive while the
/// package list streams in.
#[tauri::command]
async fn list_apps(serial: String) -> Result<Vec<AppInfo>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<AppInfo>, String> {
        let adb = adb::AdbClient::default();
        let out = adb
            .exec_out(&serial, &["pm", "list", "packages", "-f"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        let mut apps = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            // line format: package:/data/app/â€¦/base.apk=com.example
            let Some(rest) = line.strip_prefix("package:") else { continue };
            let Some((apk, package)) = rest.rsplit_once('=') else { continue };
            if package.is_empty() {
                continue;
            }
            let system = apk.starts_with("/system")
                || apk.starts_with("/product")
                || apk.starts_with("/vendor")
                || apk.starts_with("/odm")
                || apk.starts_with("/apex");
            apps.push(AppInfo {
                package: package.to_string(),
                apk_path: apk.to_string(),
                system,
                version_name: None,
                version_code: None,
                target_sdk: None,
                first_install: String::new(),
                last_update: String::new(),
                data_dir: None,
                signer: None,
                perms: 0,
            });
        }
        apps.sort_by(|a, b| a.package.cmp(&b.package));
        Ok(apps)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Full configuration for one app (read-only `dumpsys package`).
#[tauri::command]
async fn app_details(serial: String, package: String) -> Result<AppInfo, String> {
    tokio::task::spawn_blocking(move || -> Result<AppInfo, String> {
        let adb = adb::AdbClient::default();
        let out = adb
            .exec_out(&serial, &["dumpsys", "package", &package])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        let mut version_name = None;
        let mut version_code = None;
        let mut target_sdk = None;
        let mut first_install = String::new();
        let mut last_update = String::new();
        let mut data_dir = None;
        let mut signer = None;
        let mut perms = 0;
        for raw in text.lines() {
            let l = raw.trim();
            if version_name.is_none() {
                if let Some(v) = l.strip_prefix("versionName=") {
                    version_name = Some(v.trim().to_string());
                    continue;
                }
            }
            if version_code.is_none() && l.starts_with("versionCode=") {
                // "versionCode=263307622 minSdk=23 targetSdk=36"
                let mut it = l.split_whitespace();
                if let Some(vc) = it.next().and_then(|t| t.strip_prefix("versionCode=")) {
                    version_code = Some(vc.to_string());
                }
                for t in it {
                    if let Some(v) = t.strip_prefix("targetSdk=") {
                        target_sdk = Some(v.to_string());
                    }
                }
                continue;
            }
            if first_install.is_empty() {
                if let Some(v) = l.strip_prefix("firstInstallTime=") {
                    if !v.trim().starts_with("1970") {
                        first_install = v.trim().to_string();
                    }
                    continue;
                }
            }
            if last_update.is_empty() {
                if let Some(v) = l.strip_prefix("lastUpdateTime=") {
                    if !v.trim().starts_with("1970") {
                        last_update = v.trim().to_string();
                    }
                    continue;
                }
            }
            if data_dir.is_none() {
                if let Some(v) = l.strip_prefix("dataDir=") {
                    let v = v.trim();
                    if v != "null" && !v.is_empty() {
                        data_dir = Some(v.to_string());
                    }
                    continue;
                }
            }
            if signer.is_none() {
                if let Some(v) = l.strip_prefix("Signatures: [") {
                    signer = Some(v.trim_end_matches(']').to_string());
                    continue;
                }
            }
            if l.starts_with("android.permission.") && l.contains(": granted=") {
                perms += 1;
            }
        }
        Ok(AppInfo {
            package,
            apk_path: String::new(),
            system: false,
            version_name,
            version_code,
            target_sdk,
            first_install,
            last_update,
            data_dir,
            signer,
            perms,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Local PNG path of an extracted launcher icon.
#[derive(Serialize)]
struct IconResult {
    path: String,
}

/// Extract the app's real launcher icon from its on-device APK (read-only,
/// streams only a few KB). Cached on disk per package.
#[tauri::command]
async fn app_icon(app: AppHandle, serial: String, package: String) -> Result<IconResult, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("icons");
    tokio::task::spawn_blocking(move || -> Result<IconResult, String> {
        // Already extracted on a previous run?
        for ext in ["webp", "png", "jpg"] {
            let p = dir.join(format!("{package}.{ext}"));
            if p.exists() {
                return Ok(IconResult { path: p.display().to_string() });
            }
        }
        let adb = adb::AdbClient::default();
        let icon = icons::fetch_icon(&adb, &serial, &package)?;
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("{package}.{}", icon.ext));
        std::fs::write(&path, &icon.bytes).map_err(|e| e.to_string())?;
        Ok(IconResult { path: path.display().to_string() })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forensic proof that a recovered file is genuine: reads the actual bytes,
/// detects the true format from magic numbers and computes SHA-256.

/// Detect all supported messaging apps installed on the device.
#[tauri::command]
async fn detect_apps(serial: String) -> Result<recovery_core::apps::AppInventory, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::apps::AppInventory, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::apps::detect_apps(&adb, &serial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get media files from a specific app.
#[tauri::command]
async fn get_app_media(serial: String, app_id: String) -> Result<Vec<(String, i64)>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<(String, i64)>, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::apps::get_app_media(&adb, &serial, &app_id))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract Instagram data from device.
#[tauri::command]
async fn extract_instagram(serial: String) -> Result<recovery_core::apps::instagram::InstagramData, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::apps::instagram::InstagramData, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::apps::instagram::extract_instagram(&adb, &serial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract Messenger data from device.
#[tauri::command]
async fn extract_messenger(serial: String) -> Result<recovery_core::apps::messenger::MessengerData, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::apps::messenger::MessengerData, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::apps::messenger::extract_messenger(&adb, &serial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract Telegram data from device.
#[tauri::command]
async fn extract_telegram(serial: String) -> Result<recovery_core::apps::telegram::TelegramData, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::apps::telegram::TelegramData, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::apps::telegram::extract_telegram(&adb, &serial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Extract forensic data from any app with deleted record carving.
#[tauri::command]
async fn extract_app_forensic(serial: String, app_id: String) -> Result<recovery_core::apps::extraction::ForensicExtraction, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::apps::extraction::ForensicExtraction, String> {
        let adb = adb::AdbClient::default();
        let temp_dir = std::path::PathBuf::from("D:\\RecoverKit_Temp");
        std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        Ok(recovery_core::apps::extraction::extract_app(&adb, &serial, &app_id, &temp_dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Automatic WhatsApp deep extraction - key, decrypt, parse, extract chats.
#[tauri::command]
async fn extract_whatsapp_deep(serial: String) -> Result<recovery_core::acquisition::WhatsAppDeepResult, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::acquisition::WhatsAppDeepResult, String> {
        let adb = adb::AdbClient::default();
        recovery_core::acquisition::acquire_whatsapp_deep(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Check root access on the device.
#[tauri::command]
async fn check_root(serial: String) -> Result<recovery_core::acquisition::RootCheckResult, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::acquisition::RootCheckResult, String> {
        let adb = adb::AdbClient::default();
        Ok(recovery_core::acquisition::check_root_access(&adb, &serial))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// WhatsApp downgrade extraction - installs legacy WhatsApp, does ADB backup, extracts key + DB.
#[tauri::command]
async fn extract_whatsapp_downgrade(serial: String) -> Result<recovery_core::acquisition::WhatsAppDeepResult, String> {
    tokio::task::spawn_blocking(move || -> Result<recovery_core::acquisition::WhatsAppDeepResult, String> {
        let adb = adb::AdbClient::default();
        recovery_core::acquisition::acquire_whatsapp_downgrade(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Poll current call state from device.
#[tauri::command]
async fn poll_call_state(serial: String) -> Result<Option<recovery_core::call_monitor::CallEvent>, String> {
    tokio::task::spawn_blocking(move || -> Result<Option<recovery_core::call_monitor::CallEvent>, String> {
        let adb = adb::AdbClient::default();
        recovery_core::call_monitor::poll_call_state(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Check recent call log entries since a given timestamp.
#[tauri::command]
async fn check_recent_calls(serial: String, since_ms: i64) -> Result<Vec<recovery_core::call_monitor::CallEvent>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<recovery_core::call_monitor::CallEvent>, String> {
        let adb = adb::AdbClient::default();
        recovery_core::call_monitor::check_recent_calls(&adb, &serial, since_ms).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Get formatted call notification message.
#[tauri::command]
async fn get_call_notification(serial: String) -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(move || -> Result<Option<String>, String> {
        let adb = adb::AdbClient::default();
        recovery_core::call_monitor::get_call_notification(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Poll live call state: `dumpsys telecom` (RINGING/DIALING/ACTIVE) with a
/// call-log fallback for ENDED/MISSED entries.
#[tauri::command]
async fn poll_live_call(serial: String) -> Result<Option<recovery_core::call_monitor::CallEvent>, String> {
    tokio::task::spawn_blocking(move || -> Result<Option<recovery_core::call_monitor::CallEvent>, String> {
        let adb = adb::AdbClient::default();
        recovery_core::call_monitor::poll_live_call(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Discover call recordings on the device (broad vendor dirs + recursive sweep),
/// each matched to its call-log entry by number/timestamp.
#[tauri::command]
async fn get_call_recordings(serial: String) -> Result<Vec<recovery_core::acquisition::CallRecording>, String> {
    tokio::task::spawn_blocking(move || -> Result<Vec<recovery_core::acquisition::CallRecording>, String> {
        let adb = adb::AdbClient::default();
        let logs = acquisition::acquire_call_logs(&adb, &serial).unwrap_or_default();
        acquisition::acquire_call_recordings(&adb, &serial, &logs).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct MediaInspect {
    size: u64,
    sha256: String,
    format: &'static str,
    valid: bool,
}

#[tauri::command]
async fn inspect_media(path: String) -> Result<MediaInspect, String> {
    tokio::task::spawn_blocking(move || -> Result<MediaInspect, String> {
        use std::io::Read;
        let mut file = std::fs::File::open(&path).map_err(|e| format!("cannot read the file: {e}"))?;
        let size = file.metadata().map(|m| m.len()).unwrap_or(0);
        // Stream in 1 MB chunks: videos can be hundreds of MB and must not
        // be buffered whole, and the UI thread stays free the entire time.
        let mut head = [0u8; 16];
        let mut head_len = 0usize;
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        let mut buf = vec![0u8; 1024 * 1024];
        let mut total = 0u64;
        loop {
            let n = file.read(&mut buf).map_err(|e| format!("read failed: {e}"))?;
            if n == 0 {
                break;
            }
            if head_len < 16 {
                let take = (16 - head_len).min(n);
                head[head_len..head_len + take].copy_from_slice(&buf[..take]);
                head_len += take;
            }
            h.update(&buf[..n]);
            total += n as u64;
        }
        let b = &head[..head_len];
        let format = match b {
            b if b.starts_with(&[0xFF, 0xD8, 0xFF]) => "JPEG",
            b if b.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => "PNG",
            b if b.starts_with(b"GIF8") => "GIF",
            b if b.starts_with(b"BM") => "BMP",
            b if b.starts_with(b"RIFF") && b.len() >= 12 && &b[8..12] == b"WEBP" => "WEBP",
            b if b.len() >= 8 && &b[4..8] == b"ftyp" => "MP4",
            b if b.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) => "MKV/WebM",
            _ => "unknown / not a valid media file",
        };
        Ok(MediaInspect {
            size: size.max(total),
            sha256: hex::encode(h.finalize()),
            format,
            valid: !format.starts_with("unknown") && total > 0,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Local folder holding the pulled media for a device.
#[tauri::command]
fn media_dir(app: AppHandle, serial: String) -> Result<String, String> {
    let d = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("pulled")
        .join(&serial);
    Ok(d.display().to_string())
}

/// Import an SMS Backup & Restore `.xml` file â€” real SMS/MMS, no root.
#[tauri::command]
fn import_sms_xml(state: tauri::State<AppState>, path: String) -> Result<UiRecovery, String> {
    let result = run_blocking(move || -> Result<ParseResult, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read the XML file: {e}"))?;
        let result = sms_xml::import_from_xml(&text, &path);
        if result.messages.is_empty() {
            return Err(
                "No SMS entries found â€” is this an SMS Backup & Restore .xml file? \
                 (install \"SMS Backup & Restore\" on the phone â†’ Back up â†’ copy the .xml here)"
                    .into(),
            );
        }
        Ok(result)
    })??;
    let ui = build_ui(&result);
    *state.last_result.lock().unwrap() = Some(result);
    Ok(ui)
}

/// Import a `.vcf` contacts export â€” full address book, no root.
#[tauri::command]
fn import_vcf(state: tauri::State<AppState>, path: String) -> Result<UiRecovery, String> {
    let result = run_blocking(move || -> Result<ParseResult, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read the .vcf file: {e}"))?;
        let result = vcf::import_from_vcf(&text, &path);
        if result.contacts.is_empty() {
            return Err(
                "No contacts found â€” is this a .vcf exported from the phone's Contacts app? \
                 (Contacts â†’ â‹¯ â†’ Export â†’ .vcf)"
                    .into(),
            );
        }
        Ok(result)
    })??;
    let ui = build_ui(&result);
    *state.last_result.lock().unwrap() = Some(result);
    Ok(ui)
}

/// Shape a full engine result into the payload the UI displays.
fn build_ui(result: &ParseResult) -> UiRecovery {
    let analysis = sqlite::analyze(result);
    let mut names: Vec<(String, String)> = sqlite::jid_names(result)
        .into_iter()
        .filter(|(_, n)| !n.is_empty())
        .collect();
    names.sort_by(|a, b| b.1.to_lowercase().cmp(&a.1.to_lowercase()));
    UiRecovery {
        db_path: result.db_path.clone(),
        schema: result.schema.clone(),
        sha256: result.sha256.clone(),
        total_messages: result.total_messages,
        recovered_deleted: result.recovered_deleted,
        truncated: result.truncated,
        chats: result.chats.len(),
        contacts: result.contacts.len(),
        media: result.media.len(),
        messages: result.messages.iter().take(UI_MESSAGE_CAP).cloned().collect(),
        messages_hidden: result.messages.len().saturating_sub(UI_MESSAGE_CAP),
        names,
        contacts_list: result
            .contacts
            .iter()
            .take(500)
            .map(|c| (c.name.clone(), c.jid.clone()))
            .collect(),
        analysis,
    }
}

/// Parse (or deep-recover) a msgstore database; keeps the full result for export.
/// Device paths (`/sdcard/...`) are pulled from the phone automatically.
#[tauri::command]
fn recover_db(
    app: AppHandle,
    state: tauri::State<AppState>,
    db_path: String,
    deep: bool,
    serial: Option<String>,
) -> Result<UiRecovery, String> {
    let result = run_blocking(move || {
        let p = localize(&app, &serial, &db_path)?;
        if deep {
            sqlite::recover(&p)
        } else {
            sqlite::parse_msgstore(&p)
        }
        .map_err(|e| e.to_string())
    })??;

    let ui = build_ui(&result);
    *state.last_result.lock().unwrap() = Some(result);
    Ok(ui)
}

/// Import a WhatsApp â€œExport chatâ€ `.txt` transcript â€” real messages,
/// no root and no key file needed.
#[tauri::command]
fn import_chat_txt(state: tauri::State<AppState>, path: String) -> Result<UiRecovery, String> {
    let result = run_blocking(move || -> Result<ParseResult, String> {
        let p = PathBuf::from(&path);
        let text = std::fs::read_to_string(&p)
            .map_err(|e| format!("cannot read the export file: {e}"))?;
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "WhatsApp Export".into());
        let result = txt_export::import_from_text(&text, &name, &path);
        if result.messages.is_empty() {
            return Err(
                "No messages found in this file â€” is it a WhatsApp chat export .txt? \
                 (WhatsApp â†’ chat â†’ â‹¯ â†’ More â†’ Export chat â†’ Without media)"
                    .into(),
            );
        }
        Ok(result)
    })??;
    let ui = build_ui(&result);
    *state.last_result.lock().unwrap() = Some(result);
    Ok(ui)
}

/// Export the last result as `json` | `csv` | `html` to `dest`.
#[tauri::command]
fn export_result(state: tauri::State<AppState>, format: String, dest: String) -> Result<String, String> {
    let guard = state.last_result.lock().unwrap();
    let result = guard.as_ref().ok_or("Run a recovery first â€” nothing to export")?;
    let dest = PathBuf::from(&dest);

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
            std::fs::write(&dest, json).map_err(|e| e.to_string())?;
        }
        "csv" => {
            let mut out = String::from("row_id,timestamp,from_me,sender,chat_jid,text,source\n");
            for m in &result.messages {
                let source = match m.source {
                    sqlite::Source::Live => "live",
                    sqlite::Source::Wal => "wal",
                    sqlite::Source::Freelist => "freelist",
                };
                out.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    m.row_id.map(|v| v.to_string()).unwrap_or_default(),
                    m.timestamp.map(|v| v.to_string()).unwrap_or_default(),
                    m.from_me,
                    csv_field(m.sender.as_deref().unwrap_or("")),
                    csv_field(m.chat_jid.as_deref().unwrap_or("")),
                    csv_field(m.text.as_deref().unwrap_or("")),
                    source
                ));
            }
            std::fs::write(&dest, out).map_err(|e| e.to_string())?;
        }
        "html" => {
            std::fs::write(&dest, build_html_report(result)).map_err(|e| e.to_string())?;
        }
        other => return Err(format!("unknown format: {other}")),
    }
    Ok(dest.display().to_string())
}

fn csv_field(s: &str) -> String {
    let needs_quote = s.contains(',') || s.contains('"') || s.contains('\n');
    if needs_quote {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn build_html_report(r: &ParseResult) -> String {
    let names = sqlite::jid_names(r);
    let display_name = |jid: &str| -> String {
        names
            .get(jid)
            .map(|n| format!("{n} ({jid})"))
            .unwrap_or_else(|| jid.to_string())
    };
    let analysis = sqlite::analyze(r);
    let mut rows = String::new();
    for m in &r.messages {
        let (class, label) = match m.source {
            sqlite::Source::Live => ("live", "live"),
            sqlite::Source::Wal => ("wal", "wal"),
            sqlite::Source::Freelist => ("deleted", "deleted"),
        };
        let ts = m
            .timestamp
            .map(|v| {
                chrono_fmt(v)
            })
            .unwrap_or_else(|| "â€”".into());
        rows.push_str(&format!(
            "<tr class=\"{class}\"><td>{ts}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            html_escape(&display_name(m.chat_jid.as_deref().unwrap_or("â€”"))),
            if m.from_me { "out" } else { "in" },
            html_escape(m.text.as_deref().unwrap_or("")),
        ));
        let _ = label;
    }

    let range = match (analysis.first_ts, analysis.last_ts) {
        (Some(a), Some(b)) => format!("{} â†’ {}", chrono_fmt(a), chrono_fmt(b)),
        _ => "unknown".into(),
    };
    let top = analysis
        .top_chats
        .iter()
        .map(|(jid, name, count)| {
            let who = if name.is_empty() { jid.clone() } else { format!("{name} ({jid})") };
            format!("<li>{} â€” {} messages</li>", html_escape(&who), count)
        })
        .collect::<String>();

    format!(
        r#"<!doctype html>
<html><head><meta charset="utf-8"><title>RecoverKit report</title>
<style>
body{{font-family:Segoe UI,system-ui,sans-serif;background:#fff8f0;color:#3a2e1f;margin:2rem}}
h1{{color:#c77700;font-size:1.4rem}} h2{{color:#c77700;font-size:1rem;margin:1.2rem 0 .4rem}} .meta{{color:#8a7a60;font-size:.85rem}}
code{{background:#fde8c8;padding:1px 5px;border-radius:4px;font-size:.8rem}} ul.top{{margin:0;font-size:.9rem}}
table{{border-collapse:collapse;width:100%;font-size:.85rem;background:#fff;border-radius:8px;overflow:hidden;box-shadow:0 1px 4px rgba(196,120,0,.15)}}
th{{background:#f4a300;color:#fff;text-align:left;padding:6px 10px}} td{{padding:5px 10px;border-bottom:1px solid #fde8c8;vertical-align:top}}
tr.deleted td{{background:#fff3e0}} tr.deleted td:last-child{{color:#b34700;font-style:italic}}
</style></head><body>
<h1>RecoverKit â€” recovery report</h1>
<p class="meta">db: {db} Â· schema: {schema} Â· messages: {total} Â· recovered deleted: {deleted} Â· chats: {chats} Â· contacts: {contacts} Â· media refs: {media}</p>
<p class="meta">ðŸ›¡ SHA-256 of source database: <code>{sha}</code> Â· activity span: {range} Â· outgoing share: {outgoing}%</p>
<h2>Top chats</h2><ul class="top">{top}</ul>
<table><thead><tr><th>Time</th><th>Chat</th><th>Dir</th><th>Message</th></tr></thead><tbody>
{rows}
</tbody></table></body></html>"#,
        db = html_escape(&r.db_path),
        schema = r.schema,
        sha = r.sha256,
        range = html_escape(&range),
        outgoing = (analysis.from_me_ratio * 100.0).round(),
        top = top,
        total = r.total_messages,
        deleted = r.recovered_deleted,
        chats = r.chats.len(),
        contacts = r.contacts.len(),
        media = r.media.len(),
        rows = rows,
    )
}

/// Direct acquisition of call logs, SMS, and contacts from Android content providers.
#[tauri::command]
async fn direct_acquisition(serial: String) -> Result<DirectAcquisition, String> {
    tokio::task::spawn_blocking(move || -> Result<DirectAcquisition, String> {
        let adb = adb::AdbClient::default();
        acquisition::acquire_all(&adb, &serial).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pull a recording file from the device and return its local path.
#[tauri::command]
async fn pull_recording(serial: String, remote_path: String) -> Result<String, String> {
    tokio::task::spawn_blocking(move || -> Result<String, String> {
        let adb = adb::AdbClient::default();
        
        // Create a temp directory on D: drive for pulled files
        let temp_dir = std::path::PathBuf::from("D:\\RecoverKit_Temp");
        std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        
        // Extract filename from remote path
        let filename = remote_path.rsplit('/').next().unwrap_or("recording");
        let local_path = temp_dir.join(filename);
        
        // Pull the file
        adb.pull(&serial, &remote_path, &local_path).map_err(|e| e.to_string())?;
        
        // Return the local path as a string
        Ok(local_path.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// â”€â”€ Screen capture history â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Global flag for auto-capture thread.
static AUTO_CAPTURE_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Take a single screenshot, save it to the local DB, and return the capture info.
#[tauri::command]
fn take_screenshot_capture(app: AppHandle, serial: String, note: Option<String>) -> Result<screen_capture::ScreenCapture, String> {
    let adb = adb::AdbClient::default();
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = screen_capture::ScreenCaptureDB::new(&base_dir).map_err(|e| e.to_string())?;
    
    // Get device model for metadata
    let model = adb.prop(&serial, "ro.product.model").ok().flatten().filter(|s| !s.is_empty());
    
    screen_capture::capture_and_save(
        &adb,
        &serial,
        model.as_deref(),
        &db,
        note.as_deref(),
    ).map_err(|e| e.to_string())
}

/// Get all saved captures from the local DB.
#[tauri::command]
fn get_screen_captures(app: AppHandle, serial: Option<String>, limit: Option<usize>) -> Result<Vec<screen_capture::ScreenCapture>, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = screen_capture::ScreenCaptureDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.get_captures(serial.as_deref(), limit.unwrap_or(100)).map_err(|e| e.to_string())
}

/// Delete a capture by ID.
#[tauri::command]
fn delete_screen_capture(app: AppHandle, id: i64) -> Result<(), String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = screen_capture::ScreenCaptureDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.delete_capture(id).map_err(|e| e.to_string())
}

/// Start automatic periodic screen capture (every N seconds).
#[tauri::command]
fn start_auto_capture(app: AppHandle, serial: String, interval_secs: Option<u64>) -> Result<(), String> {
    if AUTO_CAPTURE_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(()); // Already running
    }
    
    AUTO_CAPTURE_RUNNING.store(true, std::sync::atomic::Ordering::SeqCst);
    let running = AUTO_CAPTURE_RUNNING.load(std::sync::atomic::Ordering::SeqCst);
    let _ = running; // suppress warning
    
    let interval = interval_secs.unwrap_or(10);
    let app_clone = app.clone();
    let serial_clone = serial.clone();
    
    std::thread::spawn(move || {
        let adb = adb::AdbClient::default();
        let base_dir = match app_clone.path().app_data_dir() {
            Ok(dir) => dir,
            Err(_) => return,
        };
        let db = match screen_capture::ScreenCaptureDB::new(&base_dir) {
            Ok(db) => db,
            Err(_) => return,
        };
        let model = adb.prop(&serial_clone, "ro.product.model").ok().flatten().filter(|s| !s.is_empty());
        
        // Smart gate state â€” tracks the last saved frame hash + foreground app
        let mut gate = screen_capture::CaptureState::new();
        
        while AUTO_CAPTURE_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
            match screen_capture::capture_smart(
                &adb,
                &serial_clone,
                model.as_deref(),
                &db,
                &mut gate,
            ) {
                Ok(Some(capture)) => {
                    // Meaningful, distinct screen â€” emit to frontend
                    let _ = app_clone.emit("screen-capture-event", &capture);
                }
                Ok(None) => {
                    // Skipped: dark/locked frame, or identical static screen â€” no event
                }
                Err(e) => {
                    eprintln!("Auto-capture failed: {}", e);
                }
            }
            
            // Sleep for interval, checking running flag every second
            for _ in 0..interval {
                if !AUTO_CAPTURE_RUNNING.load(std::sync::atomic::Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
    });
    
    Ok(())
}

/// Stop automatic periodic screen capture.
#[tauri::command]
fn stop_auto_capture() -> Result<(), String> {
    AUTO_CAPTURE_RUNNING.store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

// â”€â”€ SIM / network forensics â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Collect a live SIM/network snapshot from the device and persist it.
#[tauri::command]
fn collect_sim_snapshot(app: AppHandle, serial: String) -> Result<sim_forensics::SimSnapshot, String> {
    run_blocking(move || -> Result<sim_forensics::SimSnapshot, String> {
        let adb = adb::AdbClient::default();
        let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
        let snap = sim_forensics::collect_sim_snapshot(&adb, &serial).map_err(|e| e.to_string())?;
        db.save_snapshot(&snap).map_err(|e| e.to_string())
    })?
}

/// SIM snapshot history (newest first).
#[tauri::command]
fn get_sim_history(app: AppHandle, serial: Option<String>, limit: Option<usize>) -> Result<Vec<sim_forensics::SimSnapshot>, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.get_history(serial.as_deref(), limit.unwrap_or(50)).map_err(|e| e.to_string())
}

/// Mark a snapshot as the known-good baseline for its device.
#[tauri::command]
fn set_sim_baseline(app: AppHandle, id: i64) -> Result<(), String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.set_baseline(id).map_err(|e| e.to_string())
}

/// Delete a SIM snapshot from history.
#[tauri::command]
fn delete_sim_snapshot(app: AppHandle, id: i64) -> Result<(), String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.delete_snapshot(id).map_err(|e| e.to_string())
}

/// Collect a fresh snapshot, compare it against the baseline and the device's
/// own historical SIM records, and return the swap verdict.
#[tauri::command]
fn check_sim_swap(app: AppHandle, serial: String) -> Result<sim_forensics::SimSwapReport, String> {
    run_blocking(move || -> Result<sim_forensics::SimSwapReport, String> {
        let adb = adb::AdbClient::default();
        let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
        sim_forensics::check_sim_swap(&adb, &serial, &db).map_err(|e| e.to_string())
    })?
}

// â”€â”€ Consolidated case file + chain of custody â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

/// Create a new forensic case (auto-generates a case number when omitted).
#[tauri::command]
fn create_case(
    app: AppHandle,
    case_number: Option<String>,
    title: Option<String>,
    examiner: Option<String>,
    subject: Option<String>,
    device_serial: Option<String>,
    device_model: Option<String>,
    notes: Option<String>,
) -> Result<case_file::Case, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.create_case(
        case_number.as_deref().unwrap_or(""),
        title.as_deref().unwrap_or(""),
        examiner.as_deref().unwrap_or(""),
        subject.as_deref().unwrap_or(""),
        device_serial.as_deref(),
        device_model.as_deref(),
        notes.as_deref(),
    )
    .map_err(|e| e.to_string())
}

/// List all cases (newest first).
#[tauri::command]
fn list_cases(app: AppHandle) -> Result<Vec<case_file::Case>, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.list_cases().map_err(|e| e.to_string())
}

/// Case dashboard summary (counts, categories, chain status).
#[tauri::command]
fn get_case_summary(app: AppHandle, case_id: i64) -> Result<case_file::CaseSummary, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.summary(case_id).map_err(|e| e.to_string())
}

/// Evidence items in a case (newest first).
#[tauri::command]
fn get_case_evidence(app: AppHandle, case_id: i64) -> Result<Vec<case_file::EvidenceItem>, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.get_evidence(case_id).map_err(|e| e.to_string())
}

/// Chain-of-custody audit log for a case (chronological).
#[tauri::command]
fn get_case_audit(app: AppHandle, case_id: i64) -> Result<Vec<case_file::AuditEntry>, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.get_audit(case_id).map_err(|e| e.to_string())
}

/// Verify the tamper-evident hash chain of a case's audit log.
#[tauri::command]
fn verify_case_chain(app: AppHandle, case_id: i64) -> Result<case_file::ChainStatus, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.verify_chain(case_id).map_err(|e| e.to_string())
}

/// Acquire every source from a device fresh and bundle it all into the case.
#[tauri::command]
async fn consolidate_case(app: AppHandle, case_id: i64, serial: String, actor: String) -> Result<case_file::CaseSummary, String> {
    tokio::task::spawn_blocking(move || -> Result<case_file::CaseSummary, String> {
        let adb = adb::AdbClient::default();
        let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
        db.add_audit(case_id, "ACQUISITION_START", &format!("Consolidating evidence from {serial}"), &actor)
            .map_err(|e| e.to_string())?;

        let acq = acquisition::acquire_all(&adb, &serial).map_err(|e| e.to_string())?;
        // Re-consolidating rebuilds a clean bundle (the audit chain is kept).
        db.clear_case_data(case_id).map_err(|e| e.to_string())?;
        let n1 = case_file::ingest_direct_acquisition(&db, case_id, &acq, &actor).map_err(|e| e.to_string())?;

        let sdb = screen_capture::ScreenCaptureDB::new(&base_dir).map_err(|e| e.to_string())?;
        let caps = sdb.get_captures(Some(serial.as_str()), 500).map_err(|e| e.to_string())?;
        let n2 = case_file::ingest_captures(&db, case_id, &caps, &actor).map_err(|e| e.to_string())?;

        let simdb = sim_forensics::SimHistoryDB::new(&base_dir).map_err(|e| e.to_string())?;
        let snaps = simdb.get_history(Some(serial.as_str()), 50).map_err(|e| e.to_string())?;
        let n3 = case_file::ingest_sim(&db, case_id, &snaps, &actor).map_err(|e| e.to_string())?;

        db.add_audit(case_id, "ACQUISITION_COMPLETE", &format!("Bundled {n1} acquisition + {n2} capture + {n3} SIM evidence items"), &actor)
            .map_err(|e| e.to_string())?;
        db.summary(case_id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Mark a case closed (recorded in the chain of custody).
#[tauri::command]
fn close_case(app: AppHandle, case_id: i64, actor: Option<String>) -> Result<(), String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.close_case(case_id, actor.as_deref().unwrap_or("examiner")).map_err(|e| e.to_string())
}

/// Delete a case and all of its evidence + audit rows.
#[tauri::command]
fn delete_case(app: AppHandle, case_id: i64) -> Result<(), String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.delete_case(case_id).map_err(|e| e.to_string())
}

/// Export the consolidated case report (json | html) into the app data dir.
#[tauri::command]
fn export_case(app: AppHandle, case_id: i64, format: String) -> Result<String, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    let cases_dir = base_dir.join("cases");
    std::fs::create_dir_all(&cases_dir).map_err(|e| e.to_string())?;
    let (content, ext) = match format.as_str() {
        "json" => (case_file::build_report_json(&db, case_id).map_err(|e| e.to_string())?, "json"),
        _ => (case_file::build_report_html(&db, case_id).map_err(|e| e.to_string())?, "html"),
    };
    let path = cases_dir.join(format!("case_{case_id}_report.{ext}"));
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    db.add_audit(case_id, "EXPORT", &format!("Case report exported as {ext}"), "examiner")
        .map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Query the unified cross-source timeline for a case (kind / text / date filters).
#[tauri::command]
fn get_case_timeline(
    app: AppHandle,
    case_id: i64,
    kind: Option<String>,
    q: Option<String>,
    from: Option<i64>,
    to: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<case_file::TimelinePage, String> {
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.query_timeline(
        case_id,
        kind.as_deref(),
        q.as_deref(),
        from,
        to,
        limit.unwrap_or(500),
        offset.unwrap_or(0),
    )
    .map_err(|e| e.to_string())
}

/// Add the WhatsApp messages from the last recovery run into the case timeline so
/// global search covers actual message text. Replaces prior message events.
#[tauri::command]
fn ingest_recovered_messages(
    app: AppHandle,
    state: tauri::State<AppState>,
    case_id: i64,
    actor: Option<String>,
) -> Result<usize, String> {
    let actor = actor.unwrap_or_else(|| "examiner".into());
    let events: Vec<case_file::TimelineEvent> = {
        let guard = state.last_result.lock().map_err(|e| e.to_string())?;
        let result = guard
            .as_ref()
            .ok_or("No recovered WhatsApp data â€” run the Recover step first")?;
        result
            .messages
            .iter()
            .map(|m| case_file::TimelineEvent {
                id: 0,
                case_id,
                timestamp: m.timestamp.unwrap_or(0),
                kind: "whatsapp_message".to_string(),
                title: format!(
                    "WhatsApp Â· {}",
                    if m.from_me {
                        "You".to_string()
                    } else {
                        m.sender.clone().or(m.chat_jid.clone()).unwrap_or_else(|| "unknown".into())
                    }
                ),
                detail: m.text.clone().unwrap_or_default(),
                source: m.chat_jid.clone().unwrap_or_else(|| "WhatsApp".into()),
                local_path: None,
            })
            .collect()
    };
    let base_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let db = case_file::CaseDB::new(&base_dir).map_err(|e| e.to_string())?;
    db.clear_timeline_kind(case_id, "whatsapp_message").map_err(|e| e.to_string())?;
    let n = db.add_timeline_events(&events).map_err(|e| e.to_string())?;
    db.add_audit(
        case_id,
        "EVIDENCE_ADDED",
        &format!("{n} recovered WhatsApp message(s) added to timeline"),
        &actor,
    )
    .map_err(|e| e.to_string())?;
    Ok(n)
}

/// Milliseconds-since-epoch â†’ simple local-ish timestamp string without extra crates.
fn chrono_fmt(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days algorithm (Howard Hinnant) â€” UTC.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}

#[tauri::command]
async fn capture_device_photo(serial: String, app: AppHandle) -> Result<acquisition::CameraCaptureResult, String> {
    let adb = adb::AdbClient::default();
    let output_dir = PathBuf::from("D:\\RecoverKit_Temp\\camera");
    
    // Emit progress event
    let _ = app.emit("capture-progress", serde_json::json!({
        "type": "camera",
        "status": "Opening camera app on device..."
    }));
    
    // Run blocking operation in a separate thread
    let result = tokio::task::spawn_blocking(move || {
        acquisition::capture_device_photo(&adb, &serial, &output_dir)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    
    Ok(result)
}

#[tauri::command]
async fn capture_device_mic(serial: String, duration: u32, app: AppHandle) -> Result<acquisition::MicCaptureResult, String> {
    let adb = adb::AdbClient::default();
    let output_dir = PathBuf::from("D:\\RecoverKit_Temp\\mic");
    
    // Emit progress event
    let _ = app.emit("capture-progress", serde_json::json!({
        "type": "mic",
        "status": format!("Recording {} seconds from device mic...", duration)
    }));
    
    // Run blocking operation in a separate thread
    let result = tokio::task::spawn_blocking(move || {
        acquisition::capture_device_mic(&adb, &serial, &output_dir, duration)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;
    
    Ok(result)
}

#[tauri::command]
fn adb_shell_command(serial: String, command: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let result = adb.shell(&serial, &["sh", "-c", &command])
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Launch an app on the device. Resolves the launcher activity via dumpsys,
/// then starts it with `am start -n`. Works on all devices (Vivo/Oppo/MIUI/Pixel/etc).
#[tauri::command]
fn launch_app(serial: String, package: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();

        // Step 1: Resolve the launcher activity for this package
        let dump = adb.shell(&serial, &[
            "dumpsys", "package", &package,
        ]).map_err(|e| e.to_string())?;
        let dump_str = String::from_utf8_lossy(&dump);

        // Look for the android.intent.action.MAIN + LAUNCHER activity
        // dumpsys output contains lines like:
        //   <package>/<ActivityName> filter <hash>
        //     Action: "android.intent.action.MAIN"
        //     Category: "android.intent.category.LAUNCHER"
        // Or in the resolver section:
        //   <package>/<ActivityName>
        let mut launcher_activity: Option<String> = None;

        // Strategy: find lines with "android.intent.action.MAIN" and look backwards
        // for the activity name, or parse the "Android launchable activities" section.
        let lines: Vec<&str> = dump_str.lines().collect();

        // Look for the "android.intent.category.LAUNCHER" section
        let mut in_launcher_section = false;
        let mut current_activity: Option<String> = None;

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Track activity names like "com.whatsapp/.Main"
            if trimmed.starts_with(&format!("{}/", package)) && trimmed.contains('/') {
                current_activity = Some(trimmed.to_string());
            }

            if trimmed.contains("android.intent.action.MAIN") {
                in_launcher_section = true;
            }

            if in_launcher_section && trimmed.contains("android.intent.category.LAUNCHER") {
                if let Some(ref act) = current_activity {
                    launcher_activity = Some(act.clone());
                    break;
                }
            }

            // Reset when we hit a new section
            if trimmed.is_empty() || (trimmed.starts_with(&format!("{}/", package)) && !trimmed.contains(' ')) {
                if !trimmed.starts_with(&format!("{}/", package)) {
                    in_launcher_section = false;
                }
            }
        }

        // Fallback: try pm resolve-activity
        let activity = if let Some(act) = launcher_activity {
            act
        } else {
            let resolve = adb.shell(&serial, &[
                "cmd", "package", "resolve-activity", "--brief", "-c",
                "android.intent.category.LAUNCHER", &package,
            ]).map_err(|e| e.to_string())?;
            let resolve_str = String::from_utf8_lossy(&resolve);
            // Output is like: "<package>/<Activity>\n"
            resolve_str
                .lines()
                .find(|l| l.contains('/') && !l.starts_with('#'))
                .map(|l| l.trim().to_string())
                .ok_or_else(|| format!("Could not find launcher activity for {}", package))?
        };

        // Step 2: Start the resolved activity
        let result = adb.shell(&serial, &[
            "am", "start", "-n", &activity,
        ]).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Tap at coordinates on the device screen.
#[tauri::command]
fn screen_tap(serial: String, x: u32, y: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let result = adb.shell(&serial, &[
            "input", "tap", &x.to_string(), &y.to_string(),
        ]).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Swipe from (x1,y1) to (x2,y2) over duration_ms milliseconds.
#[tauri::command]
fn screen_swipe(serial: String, x1: u32, y1: u32, x2: u32, y2: u32, duration_ms: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let result = adb.shell(&serial, &[
            "input", "swipe",
            &x1.to_string(), &y1.to_string(),
            &x2.to_string(), &y2.to_string(),
            &duration_ms.to_string(),
        ]).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Type text on the device (sends each character via input text).
#[tauri::command]
fn screen_text(serial: String, text: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Escape special characters for shell
        let escaped = text.replace('\\', "\\\\").replace('"', "\\\"");
        let result = adb.shell(&serial, &[
            "input", "text", &escaped,
        ]).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Press a key event on the device (e.g., 4=BACK, 3=HOME, 187=RECENTS, 26=POWER).
#[tauri::command]
fn screen_key(serial: String, keycode: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let result = adb.shell(&serial, &[
            "input", "keyevent", &keycode.to_string(),
        ]).map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&result).to_string())
    })?
}

/// Acquire a partial wake lock so the CPU stays awake even with the screen off.
#[tauri::command]
fn headless_start(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Acquire a partial wake lock (CPU stays awake, screen can turn off)
        let lock_res = adb.shell(&serial, &[
            "sh", "-c",
            "echo 'RecoverKitHeadless' > /sys/power/wake_lock",
        ]).map_err(|e| e.to_string());
        // Fallback: use svc power (works on most Android builds)
        if lock_res.is_err() || !lock_res.as_ref().map(|o| String::from_utf8_lossy(o).is_empty()).unwrap_or(true) {
            let _ = adb.shell(&serial, &["svc", "power", "stayon", "usb"]);
        }
        // Turn the physical display off
        let _ = adb.shell(&serial, &["input", "keyevent", "26"]);
        Ok("Headless mode started â€” screen off, CPU awake".to_string())
    })?
}

/// Release headless mode â€” wake the screen and remove stay-on.
#[tauri::command]
fn headless_stop(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Wake the display
        let _ = adb.shell(&serial, &["input", "keyevent", "224"]);
        // Remove stay-on
        let _ = adb.shell(&serial, &["svc", "power", "stayon", "none"]);
        // Release wake lock
        let _ = adb.shell(&serial, &[
            "sh", "-c",
            "echo 'RecoverKitHeadless' > /sys/power/wake_unlock",
        ]);
        Ok("Headless mode stopped â€” screen restored".to_string())
    })?
}

/// Check whether the device screen is currently on or off.
#[tauri::command]
fn get_screen_state(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &[
            "dumpsys", "power", "|", "grep", "mWakefulness=",
        ]).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("Asleep") || text.contains("Dozing") {
            Ok("off".to_string())
        } else {
            Ok("on".to_string())
        }
    })?
}

/// Rotate the physical device display.
/// `rotation`: 0=portrait, 1=landscape 90Â°, 2=portrait upside-down, 3=landscape 270Â°
/// Uses multiple methods for maximum device compatibility.
#[tauri::command]
fn device_rotate(serial: String, rotation: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let rot = rotation % 4;
        let rot_str = rot.to_string();

        // Method 1: Disable auto-rotation via settings
        let _ = adb.shell(&serial, &["settings", "put", "system", "accelerometer_rotation", "0"]);

        // Method 2: Set user rotation via settings
        let r1 = adb.shell(&serial, &["settings", "put", "system", "user_rotation", &rot_str]);

        // Method 3: Also try wm (window manager) command â€” works on Android 12+
        let wm_rot = match rot {
            0 => "0",
            1 => "1", 
            2 => "2",
            3 => "3",
            _ => "0",
        };
        let r2 = adb.shell(&serial, &["cmd", "display", "set-user-rotation", "--rotation", wm_rot]);

        // Method 4: content provider insert (fallback for stubborn devices)
        let r3 = adb.shell(&serial, &[
            "content", "insert", "--uri", "content://settings/system",
            "--bind", "name:s:user_rotation",
            "--bind", &format!("value:i:{}", rot),
        ]);

        let any_ok = r1.is_ok() || r2.is_ok() || r3.is_ok();
        let label = match rot {
            0 => "Portrait (0Â°)",
            1 => "Landscape (90Â°)",
            2 => "Portrait upside-down (180Â°)",
            3 => "Landscape (270Â°)",
            _ => unreachable!(),
        };

        if any_ok {
            Ok(format!("Device rotated: {}", label))
        } else {
            let err_msg = r1.err().map(|e| e.to_string())
                .or_else(|| r2.err().map(|e| e.to_string()))
                .or_else(|| r3.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "unknown error".to_string());
            Err(format!("Rotation failed on all methods: {}", err_msg))
        }
    })?
}

/// Restore auto-rotation on the device (let the accelerometer control orientation).
#[tauri::command]
fn device_auto_rotate(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Try all methods
        let _ = adb.shell(&serial, &["settings", "put", "system", "accelerometer_rotation", "1"]);
        let _ = adb.shell(&serial, &["cmd", "display", "set-user-rotation", "--auto", "true"]);
        Ok("Auto-rotation restored â€” device follows accelerometer".to_string())
    })?
}

/// Get the device's current display density (DPI).
/// Returns the density as a string like "480" or "480x480".
#[tauri::command]
fn device_get_density(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Try wm density first (shows current + default)
        if let Ok(out) = adb.shell(&serial, &["wm", "density"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                return Ok(text);
            }
        }
        // Fallback: getprop
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.sf.lcd_density"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                return Ok(text);
            }
        }
        Err("Could not read device density".to_string())
    })?
}

/// Change the device display density (zoom level).
/// Uses `wm density` to override the DPI â€” higher = bigger UI elements (zoom in),
/// lower = smaller UI elements (zoom out). Works on all Android 4.2+ devices.
/// `density`: target DPI value (typical range 120â€“640).
#[tauri::command]
fn device_zoom(serial: String, density: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let dpi = density.to_string();

        // Method 1: wm density (window manager â€” most reliable)
        let r1 = adb.shell(&serial, &["wm", "density", &dpi]);

        // Method 2: settings put secure (fallback for some devices)
        let r2 = adb.shell(&serial, &[
            "settings", "put", "secure", "display_density_forced", &dpi,
        ]);

        if r1.is_ok() || r2.is_ok() {
            Ok(format!("Device zoom set â€” density: {} DPI", dpi))
        } else {
            let err = r1.err().map(|e| e.to_string())
                .or_else(|| r2.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "unknown error".to_string());
            Err(format!("Zoom failed: {}", err))
        }
    })?
}

/// Reset device density to factory default (removes wm density override).
#[tauri::command]
fn device_zoom_reset(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();

        // Method 1: wm density reset (Android 4.2+)
        let r1 = adb.shell(&serial, &["wm", "density", "reset"]);

        // Method 2: clear the forced density setting
        let r2 = adb.shell(&serial, &[
            "settings", "delete", "secure", "display_density_forced",
        ]);

        // Try to read back the default density for the message
        let default_info = adb.shell(&serial, &["wm", "density"])
            .ok()
            .map(|o| String::from_utf8_lossy(&o).trim().to_string())
            .unwrap_or_default();

        if r1.is_ok() || r2.is_ok() {
            let msg = if default_info.is_empty() {
                "Device zoom reset to default".to_string()
            } else {
                format!("Device zoom reset â€” {}", default_info)
            };
            Ok(msg)
        } else {
            let err = r1.err().map(|e| e.to_string())
                .or_else(|| r2.err().map(|e| e.to_string()))
                .unwrap_or_else(|| "unknown error".to_string());
            Err(format!("Zoom reset failed: {}", err))
        }
    })?
}

/// Freeze the device: full lockdown but keeps display ON for screen mirror.
/// Physical finger touches on the screen will have NO effect.
/// Uses AppRestrictor DEX + send-disable-flag + immersive mode + launcher blocking.
/// NO POWER key — screen stays on so the mirror stream is not disrupted.
#[tauri::command]
fn device_freeze(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::lock_device_mirror_safe(&adb, &serial);
        Ok("Device frozen — full lockdown active, screen visible on mirror".to_string())
    })?
}

/// Unfreeze the device: restore normal touch input and navigation.
#[tauri::command]
fn device_unfreeze(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        ringtone::unlock_device_mirror_safe(&adb, &serial);
        Ok("Device unfrozen — touch input restored".to_string())
    })?
}

/// Get the currently running foreground app on the device.
/// Returns a component string like "com.whatsapp/.Main" or empty if none detected.
#[tauri::command]
fn get_foreground_app(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Try dumpsys window first (mCurrentFocus / mFocusedApp)
        let window_out = adb.shell(&serial, &["dumpsys", "window"]);
        if let Ok(out) = window_out {
            let text = String::from_utf8_lossy(&out);
            if let Some(pkg) = parse_foreground_component(&text) {
                return Ok(pkg);
            }
        }
        // Fallback: dumpsys activity activities (mResumedActivity)
        let act_out = adb.shell(&serial, &["dumpsys", "activity", "activities"]);
        if let Ok(out) = act_out {
            let text = String::from_utf8_lossy(&out);
            if let Some(pkg) = parse_foreground_component(&text) {
                return Ok(pkg);
            }
        }
        Ok(String::new())
    })?
}

/// Extract the package name from dumpsys foreground activity output.
fn parse_foreground_component(text: &str) -> Option<String> {
    for line in text.lines() {
        if line.contains("mCurrentFocus=")
            || line.contains("mFocusedApp=")
            || line.contains("mResumedActivity:")
            || line.contains("topResumedActivity=")
            || line.contains("ResumedActivity:")
        {
            for tok in line.split_whitespace() {
                let t = tok.trim_end_matches('}').trim_end_matches(',').trim_end_matches(')');
                // Component token contains '/' (e.g. com.whatsapp/.Main)
                if t.contains('/') && !t.contains('{') && !t.contains("u0") {
                    // Extract just the package part (before the '/')
                    if let Some(pkg) = t.split('/').next() {
                        if !pkg.is_empty() && pkg.contains('.') {
                            return Some(pkg.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
//  TOOL COMMANDS â€” Clipboard, Notifications, WiFi, Backup, Export
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Read the current clipboard content from the device.
#[tauri::command]
fn get_clipboard(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &["am", "broadcast", "-a", "clip.get"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out).trim().to_string();
        if !text.is_empty() {
            return Ok(text);
        }
        // Fallback: try content provider
        let out2 = adb.shell(&serial, &["content", "read", "--uri",
            "content://com.android.providers.settings/secure",
            "--where", "name='clipboard'"])
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out2).trim().to_string())
    })?
}

/// Dump current notifications from the device.
#[tauri::command]
fn get_notifications(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &["dumpsys", "notification", "--noredact"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        // Parse active notifications
        let mut notifications = Vec::new();
        let mut in_record = false;
        let mut current_pkg = String::new();
        let mut current_title = String::new();
        let mut current_text = String::new();

        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("NotificationRecord(") {
                if in_record && !current_pkg.is_empty() {
                    notifications.push(format!("{}|{}|{}", current_pkg, current_title, current_text));
                }
                in_record = true;
                current_pkg.clear();
                current_title.clear();
                current_text.clear();
            }
            if trimmed.starts_with("pkg=") {
                current_pkg = trimmed.trim_start_matches("pkg=").split_whitespace().next().unwrap_or("").to_string();
            }
            if trimmed.contains("android.title=") {
                current_title = trimmed.split("android.title=").nth(1).unwrap_or("").trim().to_string();
            }
            if trimmed.contains("android.text=") {
                current_text = trimmed.split("android.text=").nth(1).unwrap_or("").trim().to_string();
            }
        }
        if in_record && !current_pkg.is_empty() {
            notifications.push(format!("{}|{}|{}", current_pkg, current_title, current_text));
        }
        Ok(notifications.join("\n"))
    })?
}

/// Extract saved WiFi network names and passwords from the device.
#[tauri::command]
fn get_wifi_credentials(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &["cat", "/data/misc/wifi/wpa_supplicant.conf"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("network=") {
            let mut results = Vec::new();
            let mut ssid = String::new();
            let mut psk = String::new();
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with("ssid=\"") {
                    ssid = t.trim_start_matches("ssid=\"").trim_end_matches('"').to_string();
                }
                if t.starts_with("psk=\"") {
                    psk = t.trim_start_matches("psk=\"").trim_end_matches('"').to_string();
                }
                if t == "}" && !ssid.is_empty() {
                    results.push(format!("{}|{}", ssid, if psk.is_empty() { "OPEN" } else { &psk }));
                    ssid.clear();
                    psk.clear();
                }
            }
            return Ok(results.join("\n"));
        }
        // Fallback: try WiFi user config on newer Android
        let out2 = adb.shell(&serial, &["cat", "/data/misc/apexdata/com.android.wifi/WifiConfigStore.xml"])
            .map_err(|e| e.to_string())?;
        let text2 = String::from_utf8_lossy(&out2);
        if text2.is_empty() || text2.contains("Permission denied") {
            return Err("WiFi credentials require root access on this device".to_string());
        }
        Ok(text2.to_string())
    })?
}

/// Create a full ADB backup of the device.
#[tauri::command]
fn create_device_backup(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let backup_file = format!("backup_{}.ab", ts);
        let backup_path = std::path::Path::new(&output_dir).join(&backup_file);

        // Use adb backup (works on Android < 12, limited on newer)
        let out = adb.shell(&serial, &["bu", "backup", "-f", "/sdcard/recoverkit_backup.ab",
            "-apk", "-obb", "-shared"])
            .map_err(|e| e.to_string())?;

        // Pull the backup file
        match adb.pull(&serial, "/sdcard/recoverkit_backup.ab", &backup_path) {
            Ok(_) => {
                // Clean up on device
                let _ = adb.shell(&serial, &["rm", "-f", "/sdcard/recoverkit_backup.ab"]);
                Ok(format!("Backup saved to {}", backup_path.display()))
            }
            Err(e) => Err(format!("Backup pull failed: {}", e))
        }
    })?
}

/// Export recovered data to a JSON file.
#[tauri::command]
fn export_data_json(data: String, filename: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let path = std::path::Path::new(&output_dir).join(&filename);
        std::fs::write(&path, &data).map_err(|e| e.to_string())?;
        Ok(format!("Exported to {}", path.display()))
    })?
}

/// Pull all accessible data from a specific app package.
#[tauri::command]
fn pull_app_data(serial: String, package: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let pkg_dir = std::path::Path::new(&output_dir).join(&package);
        std::fs::create_dir_all(&pkg_dir).map_err(|e| e.to_string())?;

        let mut pulled = Vec::new();

        // Try to pull known database paths for common apps
        let db_paths = match package.as_str() {
            "com.whatsapp" => vec![
                "databases/msgstore.db",
                "databases/wa.db",
                "databases/axolotl.db",
                "databases/journal.db",
            ],
            "com.instagram.android" => vec![
                "databases/direct.db",
                "databases/instagram.db",
            ],
            _ => vec!["databases/"],
        };

        let data_dir = format!("/data/data/{}/", package);
        for db_path in db_paths {
            let remote = format!("{}{}", data_dir, db_path);
            let local = pkg_dir.join(db_path.replace('/', "_"));
            if adb.pull(&serial, &remote, &local).is_ok() {
                pulled.push(db_path.to_string());
            }
        }

        // Also try to pull shared storage data
        let sd_paths = vec![
            format!("/sdcard/Android/data/{}/", package),
            format!("/sdcard/Android/media/{}/", package),
        ];
        for sd_path in &sd_paths {
            let out = adb.shell(&serial, &["ls", sd_path]);
            if let Ok(files) = out {
                let file_list = String::from_utf8_lossy(&files);
                for fname in file_list.lines().take(50) {
                    let remote = format!("{}{}", sd_path, fname.trim());
                    let local = pkg_dir.join(format!("sd_{}", fname.trim()));
                    if adb.pull(&serial, &remote, &local).is_ok() {
                        pulled.push(format!("sdcard:{}", fname.trim()));
                    }
                }
            }
        }

        if pulled.is_empty() {
            Err(format!("No accessible data found for {}. App data requires root or ADB backup.", package))
        } else {
            Ok(format!("Pulled {} files: {}", pulled.len(), pulled.join(", ")))
        }
    })?
}

/// Search through a database file for matching text.
#[tauri::command]
fn search_database(db_path: String, query: String) -> Result<String, String> {
    run_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path).map_err(|e| e.to_string())?;
        let mut results = Vec::new();

        // Get all tables
        let mut stmt = conn.prepare(
            "SELECT name FROM sqlite_master WHERE type='table'"
        ).map_err(|e| e.to_string())?;
        let tables: Vec<String> = stmt.query_map([], |row| row.get(0))
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        // Search each table's text columns
        for table in &tables {
            let cols_sql = format!("PRAGMA table_info(\"{}\")", table);
            let mut col_stmt = conn.prepare(&cols_sql).map_err(|e| e.to_string())?;
            let columns: Vec<String> = col_stmt.query_map([], |row| row.get(1))
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();

            let text_cols: Vec<&String> = columns.iter()
                .filter(|c| {
                    let c_lower = c.to_lowercase();
                    c_lower.contains("text") || c_lower.contains("message") ||
                    c_lower.contains("body") || c_lower.contains("content") ||
                    c_lower.contains("name") || c_lower.contains("title") ||
                    c_lower.contains("data") || c_lower == "data"
                })
                .collect();

            for col in &text_cols {
                let search_sql = format!(
                    "SELECT rowid, \"{}\" FROM \"{}\" WHERE \"{}\" LIKE ? LIMIT 20",
                    col, table, col
                );
                if let Ok(mut s) = conn.prepare(&search_sql) {
                    let pattern = format!("%{}%", query);
                    let rows = s.query_map([&pattern], |row| {
                        let rid: i64 = row.get(0).unwrap_or(0);
                        let val: String = row.get(1).unwrap_or_default();
                        Ok(format!("{}:{}:{}", table, rid, val))
                    });
                    if let Ok(rows) = rows {
                        for row in rows.filter_map(|r| r.ok()) {
                            results.push(row);
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(format!("No matches for '{}' in {}", query, db_path))
        } else {
            Ok(results.join("\n"))
        }
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
//  ADVANCED TOOLS â€” File Explorer, Clipboard, Notifications, etc.
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// List files in a directory on the device.
#[tauri::command]
fn list_device_files(serial: String, path: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &["ls", "-la", &path])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("No such file") || text.contains("Permission denied") {
            return Err(text.trim().to_string());
        }
        let mut entries = Vec::new();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                let perms = parts[0];
                let size = parts[4];
                let name = parts[8..].join(" ");
                let is_dir = perms.starts_with('d');
                entries.push(format!("{}|{}|{}|{}", if is_dir { "D" } else { "F" }, size, name, perms));
            }
        }
        Ok(entries.join("\n"))
    })?
}

/// Get list of running processes on the device.
#[tauri::command]
fn get_running_processes(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let out = adb.shell(&serial, &["ps", "-A"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        let mut procs = Vec::new();
        for line in text.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                let pid = parts[1];
                let name = parts[parts.len()-1];
                procs.push(format!("{}|{}", pid, name));
            }
        }
        Ok(procs.join("\n"))
    })?
}

/// Extract the APK file of an installed app.
#[tauri::command]
fn extract_apk(serial: String, package: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Get APK path from pm
        let pm_out = adb.shell(&serial, &["pm", "path", &package])
            .map_err(|e| e.to_string())?;
        let pm_text = String::from_utf8_lossy(&pm_out);
        let apk_path = pm_text.lines()
            .find(|l| l.starts_with("package:"))
            .map(|l| l.trim_start_matches("package:").trim().to_string())
            .ok_or_else(|| format!("APK path not found for {}", package))?;

        let local_path = std::path::Path::new(&output_dir).join(format!("{}.apk", package));
        adb.pull(&serial, &apk_path, &local_path).map_err(|e| e.to_string())?;
        Ok(format!("APK saved to {}", local_path.display()))
    })?
}

/// Get battery usage history and stats.
#[tauri::command]
fn get_battery_stats(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Battery Forensics ═══\n\n");

        // Current battery state — structured parsing
        report.push_str("── Current Battery State ──\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "battery"]) {
            let text = String::from_utf8_lossy(&out);
            let mut level = String::new();
            let mut status = String::new();
            let mut health = String::new();
            let mut temp = String::new();
            let mut voltage = String::new();
            let mut technology = String::new();
            let mut ac_powered = false;
            let mut usb_powered = false;
            let mut wireless_powered = false;

            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("level:") { level = l.split(':').nth(1).unwrap_or("").trim().to_string(); }
                if l.starts_with("status:") {
                    let v = l.split(':').nth(1).unwrap_or("").trim();
                    status = match v {
                        "1" => "Unknown".to_string(),
                        "2" => "Charging".to_string(),
                        "3" => "Discharging".to_string(),
                        "4" => "Not charging".to_string(),
                        "5" => "Full".to_string(),
                        _ => v.to_string()
                    }.to_string();
                }
                if l.starts_with("health:") {
                    let v = l.split(':').nth(1).unwrap_or("").trim();
                    health = match v {
                        "1" => "Unknown".to_string(),
                        "2" => "Good".to_string(),
                        "3" => "Overheat".to_string(),
                        "4" => "Dead".to_string(),
                        "5" => "Over voltage".to_string(),
                        "6" => "Failure".to_string(),
                        "7" => "Cold".to_string(),
                        _ => v.to_string()
                    }.to_string();
                }
                if l.starts_with("temperature:") {
                    let raw = l.split(':').nth(1).unwrap_or("").trim();
                    if let Ok(t) = raw.parse::<f64>() {
                        temp = format!("{:.1}°C", t / 10.0);
                    } else {
                        temp = format!("{} (raw)", raw);
                    }
                }
                if l.starts_with("voltage:") {
                    let raw = l.split(':').nth(1).unwrap_or("").trim();
                    if let Ok(v) = raw.parse::<f64>() {
                        voltage = format!("{:.2}V", v / 1000.0);
                    } else {
                        voltage = format!("{}mV", raw);
                    }
                }
                if l.starts_with("technology:") { technology = l.split(':').nth(1).unwrap_or("").trim().to_string(); }
                if l.starts_with("AC powered:") && l.contains("true") { ac_powered = true; }
                if l.starts_with("USB powered:") && l.contains("true") { usb_powered = true; }
                if l.starts_with("Wireless powered:") && l.contains("true") { wireless_powered = true; }
            }

            let charge_source = if ac_powered { "AC Adapter" } else if usb_powered { "USB" } else if wireless_powered { "Wireless" } else { "Battery" };
            report.push_str(&format!("  Level: {}%\n", level));
            report.push_str(&format!("  Status: {} ({})\n", status, charge_source));
            report.push_str(&format!("  Health: {}\n", health));
            report.push_str(&format!("  Temperature: {}\n", temp));
            report.push_str(&format!("  Voltage: {}\n", voltage));
            if !technology.is_empty() { report.push_str(&format!("  Technology: {}\n", technology)); }
        }

        // Battery health history from batterystats
        report.push_str("\n── Battery Statistics (since last charge) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --charged 2>/dev/null | head -80"]) {
            let text = String::from_utf8_lossy(&out);
            let mut in_summary = false;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Estimated power use") || l.contains("System stats") { in_summary = true; }
                if l.contains("Statistics since last charge") || l.contains("Time on battery") || l.contains("Screen on") || l.contains("Screen off") || l.contains("Discharge") || l.contains("Capacity") {
                    report.push_str(&format!("  {}\n", l));
                }
                if in_summary && (l.contains("Uid ") || l.contains("Estimated power")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Per-app battery consumption
        report.push_str("\n── Per-App Battery Drain ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats | grep -E 'Estimated power use|Uid ' | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
            }
        }

        // Wake lock analysis
        report.push_str("\n── Wake Lock Analysis ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --wake-locks 2>/dev/null | head -40"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(25) {
                let l = line.trim();
                if !l.is_empty() && (l.contains(':') || l.contains("Wake lock")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Doze mode status
        report.push_str("\n── Doze / App Standby ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys deviceidle 2>/dev/null | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("mState") || l.contains("Screen") || l.contains("Charging") || l.contains("Motion") || l.contains("Idle") || l.contains("Light") || l.contains("Deep")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }
        // App standby buckets
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "am get-standby-bucket 2>/dev/null || dumpsys battery | grep standby"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(5) {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
            }
        }

        // Charge cycle info
        report.push_str("\n── Charge Cycle Info ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --daily 2>/dev/null | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(20) {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
            }
        }

        // Battery saver status
        report.push_str("\n── Battery Saver ──\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "low_power"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Battery Saver: {}\n", if v == "1" { "ON" } else { "OFF" }));
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "low_power_trigger_level"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Auto-enable at: {}%\n", v));
            }
        }

        Ok(report)
    })?
}

/// One-click full data extraction from the device.
#[tauri::command]
fn full_device_extract(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let extract_dir = std::path::Path::new(&output_dir).join("full_extract");
        std::fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

        let mut results = Vec::new();

        // 1. Pull contacts database
        let contacts_path = extract_dir.join("contacts.db");
        if adb.pull(&serial, "/data/data/com.android.providers.contacts/databases/contacts2.db", &contacts_path).is_ok() {
            results.push("Contacts DB".to_string());
        }

        // 2. Pull SMS/MMS database
        let sms_path = extract_dir.join("mmssms.db");
        if adb.pull(&serial, "/data/data/com.android.providers.telephony/databases/mmssms.db", &sms_path).is_ok() {
            results.push("SMS/MMS DB".to_string());
        }

        // 3. Pull call log
        let calls_path = extract_dir.join("calls.db");
        if adb.pull(&serial, "/data/data/com.android.providers.contacts/databases/calllog.db", &calls_path).is_ok() {
            results.push("Call Log DB".to_string());
        }

        // 4. Pull browser bookmarks/history
        let browser_path = extract_dir.join("browser.db");
        if adb.pull(&serial, "/data/data/com.android.browser/databases/browser.db", &browser_path).is_ok() {
            results.push("Browser DB".to_string());
        }

        // 5. Pull settings
        let settings_out = adb.shell(&serial, &["settings", "get", "global", "device_name"])
            .map_err(|e| e.to_string())?;
        let device_name = String::from_utf8_lossy(&settings_out).trim().to_string();
        std::fs::write(extract_dir.join("device_info.txt"),
            format!("Device: {}\nSerial: {}\nExtracted: {}", device_name, serial, std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()))
            .ok();
        results.push("Device info".to_string());

        // 6. Pull WhatsApp databases if accessible
        let wa_paths = ["msgstore.db", "wa.db", "axolotl.db", "journal.db"];
        for wa_db in &wa_paths {
            let local = extract_dir.join(format!("wa_{}", wa_db));
            let remote = format!("/data/data/com.whatsapp/databases/{}", wa_db);
            if adb.pull(&serial, &remote, &local).is_ok() {
                results.push(format!("WhatsApp {}", wa_db));
            }
        }

        // 7. Pull shared storage media index
        let media_out = adb.shell(&serial, &["find", "/sdcard/DCIM", "-type", "f", "-name", "*.jpg"])
            .map_err(|e| e.to_string());
        if let Ok(media) = media_out {
            std::fs::write(extract_dir.join("media_index.txt"),
                String::from_utf8_lossy(&media).as_ref()).ok();
            results.push("Media index".to_string());
        }

        if results.is_empty() {
            Err("No data could be extracted. Device may require root for app data access.".to_string())
        } else {
            Ok(format!("Extracted {} items to {}:\n{}", results.len(), extract_dir.display(), results.join("\n")))
        }
    })?
}

/// Pull a specific file from the device.
#[tauri::command]
fn pull_device_file(serial: String, remote_path: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let filename = std::path::Path::new(&remote_path)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_else(|| "pulled_file".to_string());
        let local_path = std::path::Path::new(&output_dir).join(&filename);
        adb.pull(&serial, &remote_path, &local_path).map_err(|e| e.to_string())?;
        Ok(format!("File saved to {}", local_path.display()))
    })?
}

/// Scan for deleted/thumbnail images on the device.
#[tauri::command]
fn scan_thumbnails(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let thumb_dir = std::path::Path::new(&output_dir).join("thumbnails");
        std::fs::create_dir_all(&thumb_dir).map_err(|e| e.to_string())?;

        let mut found = 0;
        // Scan common thumbnail caches
        let thumb_paths = [
            "/data/data/com.whatsapp/Cache/Thumbs",
            "/sdcard/Android/data/com.whatsapp/cache/Thumbs",
            "/data/data/com.instagram.android/cache/image_manager",
            "/sdcard/.thumbnails",
        ];
        for tp in &thumb_paths {
            let ls_out = adb.shell(&serial, &["ls", tp]);
            if let Ok(files) = ls_out {
                let text = String::from_utf8_lossy(&files);
                for fname in text.lines().take(100) {
                    let remote = format!("{}/{}", tp, fname.trim());
                    let local = thumb_dir.join(format!("thumb_{}_{}", found, fname.trim()));
                    if adb.pull(&serial, &remote, &local).is_ok() {
                        found += 1;
                    }
                }
            }
        }

        if found == 0 {
            Err("No thumbnails found. Device may require root or apps may not have cached thumbnails.".to_string())
        } else {
            Ok(format!("Recovered {} thumbnails to {}", found, thumb_dir.display()))
        }
    })?
}


// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
//  DEVICE COOLING â€” reduce temperature via ADB
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Set CPU governor to reduce heat. governor: "powersave", "interactive", "schedutil", "performance"
#[tauri::command]
fn set_cpu_governor(serial: String, governor: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Try settings put first (works on some devices)
        let out = adb.shell(&serial, &["settings", "put", "system", "cpu_scaling_governor", &governor])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("error") || text.contains("Error") {
            // Fallback: try writing sysfs directly (needs root)
            let governors = ["interactive", "schedutil", "powersave", "ondemand"];
            for g in &governors {
                let set_out = adb.shell(&serial, &["echo", g, ">", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor"]);
                if let Ok(bytes) = set_out {
                    let s = String::from_utf8_lossy(&bytes);
                    if !s.contains("Permission") && !s.contains("error") {
                        return Ok(format!("CPU governor set to {} via sysfs", g));
                    }
                }
            }
            Err("Could not set CPU governor. Root access may be required.".to_string())
        } else {
            Ok(format!("CPU governor set to '{}'", governor))
        }
    })?
}

/// Kill the top CPU-hungry processes to reduce heat.
#[tauri::command]
fn kill_heavy_processes(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Get top processes by CPU usage
        let out = adb.shell(&serial, &["top", "-b", "-n", "1", "-o", "PID,CPU%,NAME"])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        let mut killed = Vec::new();

        // Parse top output â€” look for lines with high CPU%
        for line in text.lines().skip(2).take(20) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 3 {
                // Try to parse CPU percentage (second column)
                if let Some(cpu_str) = parts.get(1) {
                    if let Ok(cpu) = cpu_str.trim_end_matches('%').parse::<f64>() {
                        if cpu > 10.0 {
                            let pid = parts[0];
                            let name = parts[2];
                            // Kill the process
                            let kill_out = adb.shell(&serial, &["kill", "-9", pid]);
                            if kill_out.is_ok() {
                                killed.push(format!("{} ({}, {:.0}% CPU)", name, pid, cpu));
                            }
                        }
                    }
                }
            }
        }

        // Fallback: if top parsing didn't work, try dumpsys meminfo for heavy apps
        if killed.is_empty() {
            let mem_out = adb.shell(&serial, &["dumpsys", "meminfo", "--sort"])
                .map_err(|e| e.to_string())?;
            let mem_text = String::from_utf8_lossy(&mem_out);
            for line in mem_text.lines().take(10) {
                let trimmed = line.trim();
                if trimmed.contains("com.") || trimmed.contains("android.") {
                    let pkg = trimmed.split_whitespace().next().unwrap_or("").to_string();
                    if !pkg.is_empty() && pkg.contains('.') {
                        let force_out = adb.shell(&serial, &["am", "force-stop", &pkg]);
                        if force_out.is_ok() {
                            killed.push(pkg);
                        }
                    }
                }
            }
        }

        if killed.is_empty() {
            Ok("No heavy processes found to kill".to_string())
        } else {
            Ok(format!("Killed {} processes:\n{}", killed.len(), killed.join("\n")))
        }
    })?
}

/// Set screen brightness (0-255). Lower = less heat.
#[tauri::command]
fn set_screen_brightness(serial: String, level: u32) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let lvl = level.min(255);
        let out = adb.shell(&serial, &["settings", "put", "system", "screen_brightness", &lvl.to_string()])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("error") || text.contains("Error") {
            Err(format!("Failed to set brightness: {}", text.trim()))
        } else {
            Ok(format!("Screen brightness set to {} (0-255)", lvl))
        }
    })?
}

/// Toggle auto-sync on/off. Off reduces background CPU/network usage.
#[tauri::command]
fn set_auto_sync(serial: String, enabled: bool) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let val = if enabled { "1" } else { "0" };
        let out = adb.shell(&serial, &["settings", "put", "global", "auto_sync", val])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("error") || text.contains("Error") {
            Err(format!("Failed to set auto-sync: {}", text.trim()))
        } else {
            Ok(format!("Auto-sync {}", if enabled { "enabled" } else { "disabled" }))
        }
    })?
}

/// Enable/disable battery saver mode.
#[tauri::command]
fn set_battery_saver(serial: String, enabled: bool) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let val = if enabled { "1" } else { "0" };
        let out = adb.shell(&serial, &["settings", "put", "global", "low_power", val])
            .map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&out);
        if text.contains("error") || text.contains("Error") {
            Err(format!("Failed to set battery saver: {}", text.trim()))
        } else {
            Ok(format!("Battery saver {}", if enabled { "enabled â€” device will throttle performance" } else { "disabled" }))
        }
    })?
}

/// Apply all cooling measures at once.
#[tauri::command]
fn apply_cooling_mode(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut results: Vec<String> = Vec::new();

        // 1. Battery saver ON
        let out = adb.shell(&serial, &["settings", "put", "global", "low_power", "1"]);
        if let Ok(b) = out { if !String::from_utf8_lossy(&b).contains("error") { results.push("Battery saver ON".to_string()); } }

        // 2. Screen brightness low
        let out = adb.shell(&serial, &["settings", "put", "system", "screen_brightness", "30"]);
        if let Ok(b) = out { if !String::from_utf8_lossy(&b).contains("error") { results.push("Brightness -> 30".to_string()); } }

        // 3. Auto-sync OFF
        let out = adb.shell(&serial, &["settings", "put", "global", "auto_sync", "0"]);
        if let Ok(b) = out { if !String::from_utf8_lossy(&b).contains("error") { results.push("Auto-sync OFF".to_string()); } }

        // 4. Try powersave governor
        let out = adb.shell(&serial, &["settings", "put", "system", "cpu_scaling_governor", "powersave"]);
        if let Ok(b) = out { if !String::from_utf8_lossy(&b).contains("error") { results.push("CPU -> powersave".to_string()); } }

        // 5. Kill heavy processes
        let top_out = adb.shell(&serial, &["top", "-b", "-n", "1", "-o", "PID,CPU%,NAME"]);
        if let Ok(top_bytes) = top_out {
            let top_text = String::from_utf8_lossy(&top_bytes);
            let mut kill_count = 0;
            for line in top_text.lines().skip(2).take(15) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    if let Ok(cpu) = parts[1].trim_end_matches('%').parse::<f64>() {
                        if cpu > 15.0 {
                            if adb.shell(&serial, &["kill", "-9", parts[0]]).is_ok() {
                                kill_count += 1;
                            }
                        }
                    }
                }
            }
            if kill_count > 0 { results.push(format!("Killed {} heavy processes", kill_count)); }
        }

        if results.is_empty() {
            Err("Could not apply cooling measures. Some settings may need root.".to_string())
        } else {
            Ok(format!("Cooling applied:\n{}", results.join("\n")))
        }
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
//  FORENSIC EVIDENCE â€” Hashing, Logcat, Search, Timeline, Reports
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Compute SHA-256 hash of a file for evidence integrity.
#[tauri::command]
fn hash_evidence(file_path: String) -> Result<String, String> {
    run_blocking(move || {
        use std::io::Read;
        let mut file = std::fs::File::open(&file_path)
            .map_err(|e| format!("Cannot open file: {}", e))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let file_size = metadata.len();

        // Use a streaming hasher to handle large files
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 { break; }
            hasher.update(&buffer[..n]);
        }
        let hash = hasher.finalize();
        let hex = format!("{:x}", hash);

        Ok(format!("File: {}\nSize: {} bytes\nSHA-256: {}\nMD5: (use external tool for MD5)", file_path, file_size, hex))
    })?
}

/// Hash all files in a directory and produce a manifest.
#[tauri::command]
fn hash_directory(dir_path: String) -> Result<String, String> {
    run_blocking(move || {
        use std::io::Read;
        let entries = std::fs::read_dir(&dir_path)
            .map_err(|e| format!("Cannot read directory: {}", e))?;
        let mut manifest = Vec::new();
        manifest.push(format!("Evidence Manifest: {}", dir_path));
        manifest.push(format!("Generated: {}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()));
        manifest.push("â”€".repeat(60));

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Ok(mut file) = std::fs::File::open(&path) {
                    let mut hasher = Sha256::new();
                    let mut buf = [0u8; 8192];
                    loop {
                        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
                        if n == 0 { break; }
                        hasher.update(&buf[..n]);
                    }
                    let hex = format!("{:x}", hasher.finalize());
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    manifest.push(format!("{} | {} bytes | {}", hex, size, name));
                }
            }
        }

        Ok(manifest.join("\n"))
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// IMAGE FORENSICS â€” ELA, metadata, thumbnail analysis
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Error Level Analysis (ELA) for image forensics.
/// Re-saves the image at a known JPEG quality and computes pixel-level
/// difference. Manipulated regions show different error levels than
/// unmodified areas, revealing potential forgeries.
/// Returns the path to the ELA output image + analysis stats.
#[tauri::command]
fn image_ela(image_path: String, output_dir: String, quality: u8, scale: u32) -> Result<String, String> {
    run_blocking(move || {
        use image::GenericImageView;

        // Load the original image
        let original = image::open(&image_path)
            .map_err(|e| format!("Cannot open image: {}", e))?;

        let (width, height) = original.dimensions();
        let file_meta = std::fs::metadata(&image_path).map_err(|e| e.to_string())?;

        // Re-save as JPEG at the specified quality into a memory buffer
        let mut buf: Vec<u8> = Vec::new();
        let mut cursor = std::io::Cursor::new(&mut buf);
        let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, quality);
        original.write_with_encoder(encoder).map_err(|e| format!("JPEG encode failed: {}", e))?;

        // Decode the re-saved JPEG back
        let resaved = image::load_from_memory_with_format(&buf, image::ImageFormat::Jpeg)
            .map_err(|e| format!("JPEG re-decode failed: {}", e))?;

        // Compute pixel-level difference and scale for visibility
        let scale_f = scale as f32;
        let mut diff_img = image::RgbImage::new(width, height);
        let mut total_diff: f64 = 0.0;
        let mut max_diff: u32 = 0;
        let mut hot_pixels: u64 = 0; // pixels with diff > 30 (after scaling)

        for y in 0..height {
            for x in 0..width {
                let orig_px = original.get_pixel(x, y);
                let resaved_px = resaved.get_pixel(x, y);

                let dr = (orig_px[0] as i16 - resaved_px[0] as i16).unsigned_abs() as u32;
                let dg = (orig_px[1] as i16 - resaved_px[1] as i16).unsigned_abs() as u32;
                let db = (orig_px[2] as i16 - resaved_px[2] as i16).unsigned_abs() as u32;

                let avg = ((dr + dg + db) as f32 / 3.0 * scale_f).min(255.0) as u8;
                let scaled_diff = (dr + dg + db) / 3;

                total_diff += scaled_diff as f64;
                if scaled_diff > max_diff { max_diff = scaled_diff; }
                if scaled_diff * scale as u32 > 30 { hot_pixels += 1; }

                // Color-map: low diff = dark blue, high diff = bright red/yellow
                let r = if avg > 128 { 255 } else { (avg as u32 * 2) as u8 };
                let g = if avg > 180 { (avg as u32 * 2 - 200).min(255) as u8 } else { 0 };
                let b = if avg < 80 { avg as u8 * 3 } else { 0 };
                diff_img.put_pixel(x, y, image::Rgb([r, g, b]));
            }
        }

        let pixel_count = (width * height) as f64;
        let avg_diff = total_diff / pixel_count;
        let hot_pct = (hot_pixels as f64 / pixel_count) * 100.0;

        // Determine verdict
        let verdict = if hot_pct > 5.0 {
            "âš  HIGH â€” likely manipulation detected"
        } else if hot_pct > 1.0 {
            "âš¡ MODERATE â€” some suspicious regions"
        } else {
            "âœ… LOW â€” image appears unmodified"
        };

        // Save the ELA output image
        let fname = std::path::Path::new(&image_path)
            .file_stem().unwrap_or_default().to_string_lossy().to_string();
        let ela_path = format!("{}\\{}_ela.png", output_dir, fname);
        if let Some(parent) = std::path::Path::new(&ela_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        diff_img.save(&ela_path).map_err(|e| format!("Save ELA failed: {}", e))?;

        let report = format!(
            "â•â•â• ELA Report â•â•â•\n\
             File: {}\n\
             Dimensions: {}x{}\n\
             File size: {} bytes\n\
             JPEG quality: {}\n\
             Scale factor: {}x\n\
             â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€\n\
             Average diff: {:.2}\n\
             Max diff: {}\n\
             Hot pixels: {} ({:.2}%)\n\
             â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€\n\
             Verdict: {}\n\
             ELA output: {}",
            image_path, width, height, file_meta.len(), quality, scale,
            avg_diff, max_diff, hot_pixels, hot_pct,
            verdict, ela_path
        );

        Ok(report)
    })?
}

/// Extract comprehensive image metadata (dimensions, format, color type, timestamps).
#[tauri::command]
fn image_metadata(image_path: String) -> Result<String, String> {
    run_blocking(move || {
        use image::GenericImageView;

        let img = image::open(&image_path)
            .map_err(|e| format!("Cannot open image: {}", e))?;

        let (w, h) = img.dimensions();
        let file_meta = std::fs::metadata(&image_path).map_err(|e| e.to_string())?;
        let ext = std::path::Path::new(&image_path)
            .extension().unwrap_or_default().to_string_lossy().to_uppercase();

        let mut report = format!(
            "â•â•â• Image Metadata â•â•â•\n\
             File: {}\n\
             Format: {}\n\
             Dimensions: {}x{}\n\
             Aspect ratio: {:.2}\n\
             Megapixels: {:.1} MP\n\
             File size: {} bytes ({:.1} KB)\n\
             Color type: {:?}\n",
            image_path, ext, w, h,
            if h > 0 { w as f64 / h as f64 } else { 0.0 },
            (w as f64 * h as f64) / 1_000_000.0,
            file_meta.len(), file_meta.len() as f64 / 1024.0,
            img.color()
        );

        // Modification time
        if let Ok(modified) = file_meta.modified() {
            if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                report.push_str(&format!("Modified: {} (unix: {})\n",
                    humantime_format(duration.as_secs()), duration.as_secs()));
            }
        }

        // Creation time
        if let Ok(created) = file_meta.created() {
            if let Ok(duration) = created.duration_since(std::time::UNIX_EPOCH) {
                report.push_str(&format!("Created: {} (unix: {})\n",
                    humantime_format(duration.as_secs()), duration.as_secs()));
            }
        }

        Ok(report)
    })?
}

/// Simple human-readable timestamp formatting (no external crate needed).
pub(crate) fn humantime_format(unix_secs: u64) -> String {
    // Basic UTC formatting without chrono dependency
    let days = unix_secs / 86400;
    let time_of_day = unix_secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Calculate year/month/day from days since epoch (1970-01-01)
    let mut y = 1970i64;
    let mut remaining = days as i64;
    loop {
        let days_in_year = if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
        if remaining < days_in_year { break; }
        remaining -= days_in_year;
        y += 1;
    }
    let is_leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = [31, if is_leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining < md as i64 { m = i; break; }
        remaining -= md as i64;
        m = i + 1;
    }
    let d = remaining + 1;
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC", y, m + 1, d, hours, minutes, seconds)
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// DEVICE IDENTITY & FORENSIC COLLECTION
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            last_result: Mutex::new(None),
        })
        .manage(StreamManager {
            streams: Mutex::new(std::collections::HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            scan_folder,
            scan_device,
            decrypt_backup,
            fetch_key,
            recover_db,
            recover_media,
            save_media,
            media_dir,
            inspect_media,
            device_mirror,
            device_full_info,
            device_screenshot,
            play_ringtone,
            start_ringtone_loop,
            stop_ringtone,
            lockdown_device,
            release_device,
            stop_charging,
            start_charging,
            get_battery_status,
            device_presence,
            list_apps,
            app_details,
            app_icon,
            import_chat_txt,
            import_sms_xml,
            import_vcf,
            direct_acquisition,
            pull_recording,
            detect_apps,
            get_app_media,
            extract_instagram,
            extract_messenger,
            extract_telegram,
            extract_app_forensic,
            extract_whatsapp_deep,
            extract_whatsapp_downgrade,
            check_root,
            poll_call_state,
            check_recent_calls,
            get_call_notification,
            poll_live_call,
            get_call_recordings,
            start_screen_stream,
            stop_screen_stream,
            take_screenshot_capture,
            get_screen_captures,
            delete_screen_capture,
            start_auto_capture,
            stop_auto_capture,
            collect_sim_snapshot,
            get_sim_history,
            set_sim_baseline,
            delete_sim_snapshot,
            check_sim_swap,
            create_case,
            list_cases,
            get_case_summary,
            get_case_evidence,
            get_case_audit,
            verify_case_chain,
            consolidate_case,
            close_case,
            delete_case,
            export_case,
            get_case_timeline,
            ingest_recovered_messages,
            export_result,
            collect_wifi,
            collect_bluetooth,
            collect_accounts,
            collect_power_events,
            collect_notifications,
            collect_dictionary,
            recover_deleted_sms,
            recover_thumbnails,
            carve_device_db,
            collect_browser_data,
            collect_network_artifacts,
            analyze_installed_apks,
            collect_location_data,
            build_timeline,
            generate_report,
            capture_device_photo,
            capture_device_mic,
            adb_shell_command,
            launch_app,
            screen_tap,
            screen_swipe,
            screen_text,
            screen_key,
            get_foreground_app,
            headless_start,
            headless_stop,
            get_screen_state,
            device_rotate,
            device_auto_rotate,
            device_zoom,
            device_zoom_reset,
            device_get_density,
            device_freeze,
            device_unfreeze,
            get_clipboard,
            get_notifications,
            get_wifi_credentials,
            create_device_backup,
            export_data_json,
            pull_app_data,
            search_database,
            list_device_files,
            get_running_processes,
            extract_apk,
            get_battery_stats,
            full_device_extract,
            pull_device_file,
            scan_thumbnails,
            get_device_temperatures,
            set_cpu_governor,
            kill_heavy_processes,
            set_screen_brightness,
            set_auto_sync,
            set_battery_saver,
            apply_cooling_mode,
            hash_evidence,
            hash_directory,
            image_ela,
            image_metadata,
            collect_device_identity,
            extract_lock_info,
            recover_deleted_contacts,
            collect_cell_tower_history,
            detect_anti_forensics,
            collect_dual_sim_info,
            collect_timezone_fingerprint,
            analyze_storage,
            recover_voice_memos,
            extract_saved_passwords,
            collect_gps_timeline,
            extract_whatsapp_chats,
            extract_email_accounts,
            extract_browser_history,
            extract_video_metadata,
            extract_app_timeline,
            extract_cloud_accounts,
            analyze_encrypted_backups,
            generate_pdf_report,
            start_logcat,
            get_logcat_snapshot,
            global_keyword_search,
            get_usage_timeline,
            generate_forensic_report,
            audit_app_permissions,
            detect_spyware,
            detect_linked_devices,
            get_bluetooth_forensics,
            get_network_connections,
            get_clipboard_history,
            recover_deleted_sms_advanced,
            extract_call_logs,
            extract_wifi_history,
            extract_bluetooth_devices,
            extract_social_media,
            recover_deleted_photos,
            extract_keyboard_dictionary,
            extract_clipboard_forensics,
            extract_usb_connections,
            extract_power_forensics,
            extract_notification_history,
            extract_screen_time,
            carve_sqlite_wal,
            extract_app_sandbox,
            scan_hidden_partitions,
            extract_network_traffic,
            extract_sensor_data,
            generate_file_manifest,
            run_thermal_stress_test,
            audit_permissions_pro,
            fuzz_intents,
            analyze_selinux,
            enumerate_content_providers,
            test_privilege_escalation,
            probe_secure_element,
            check_verified_boot,
            detect_kernel_exploits,
            list_fastboot_devices,
            fastboot_getvar_all,
            fastboot_list_partitions,
            fastboot_read_partition,
            fastboot_boot_image,
            fastboot_reboot,
            reboot_to_bootloader,
            remote_agent_start,
            remote_agent_stop,
            remote_agent_status,
            remote_agent_export
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
