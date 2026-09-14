//! RecoverKit desktop â€” Tauri commands wrapping the `recovery_core` engine.

mod icons;

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
    forensic_report,
    keyboard_forensics,
    location_forensics,
    network_forensics,
    power_forensics,
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

fn run_blocking<F, T>(f: F) -> Result<T, String>
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
/// `mode` = "fast" uses screenrecord (30-60 fps), "normal" uses screencap (5-30 fps).
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

        if mode == "fast" {
            // HIGH-FPS MODE: Use screenrecord for 30-60 fps
            eprintln!("Starting high-FPS screenrecord mode at {} fps", fps);
            
            let clip_duration = 2;
            let remote_path = "/sdcard/recoverkit_stream.mp4";
            let mut clip_count = 0u64;
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let record_cmd = format!(
                    "screenrecord --time-limit {} --bit-rate 4000000 --size 720x1280 {}",
                    clip_duration, remote_path
                );
                
                match adb.exec_out(s, &["sh", "-c", &record_cmd]) {
                    Ok(_) => {
                        let local_clip = mirror_dir.join(format!("clip_{}.mp4", clip_count % 5));
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
            // NORMAL MODE: Use screencap (5-30 fps)
            let interval_ms = 1000 / fps.max(1);
            let mut frame_count = 0u64;
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let start = std::time::Instant::now();

                match adb.exec_out(s, &["screencap", "-p"]) {
                    Ok(bytes) if bytes.len() > 100 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => {
                        let frame_path = mirror_dir.join(format!("{}_stream_{}.png", serial_for_thread, frame_count % 10));
                        if std::fs::write(&frame_path, &bytes).is_ok() {
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

/// Get device temperature readings from all thermal zones.
#[tauri::command]
fn get_device_temperatures(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut result = String::new();

        // Method 1: dumpsys battery (gives battery temp in tenths of degree) â€” works without root
        let batt_out = adb.shell(&serial, &["dumpsys", "battery"])
            .map_err(|e| e.to_string())?;
        let batt_text = String::from_utf8_lossy(&batt_out);
        for line in batt_text.lines() {
            if line.contains("temperature") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(t) = val.parse::<f64>() {
                        result.push_str(&format!("Battery: {:.1}Â°C\n", t / 10.0));
                    }
                }
            }
        }

        // Method 2: dumpsys thermalservice â€” works without root, shows all sensor temps
        let svc_out = adb.shell(&serial, &["dumpsys", "thermalservice"])
            .map_err(|e| e.to_string())?;
        let svc_text = String::from_utf8_lossy(&svc_out);
        let mut in_sensor_section = false;
        for line in svc_text.lines() {
            let l = line.trim();
            // Capture thermal status line
            if l.contains("mStatus") || l.contains("Status:") {
                result.push_str(&format!("{}\n", l));
            }
            // Parse sensor entries: look for lines with temperature values
            if l.contains("{t:") || l.contains("type:") {
                // Format: {t:SENSOR_TYPE, v:VALUE, ...}
                if let Some(type_start) = l.find("t:") {
                    let type_rest = &l[type_start + 2..];
                    if let Some(type_end) = type_rest.find(|c: char| c == ',' || c == '}') {
                        let sensor_type = type_rest[..type_end].trim();
                        if let Some(val_start) = l.find("v:") {
                            let val_rest = &l[val_start + 2..];
                            if let Some(val_end) = val_rest.find(|c: char| c == ',' || c == '}') {
                                if let Ok(v) = val_rest[..val_end].trim().parse::<f64>() {
                                    result.push_str(&format!("{}: {:.1}Â°C\n", sensor_type, v));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Method 3: Try thermal zones (may need root â€” silently skip if denied)
        let thermal_out = adb.shell(&serial, &["ls", "/sys/class/thermal/"]);
        if let Ok(thermal_bytes) = thermal_out {
            let thermal_text = String::from_utf8_lossy(&thermal_bytes);
            if !thermal_text.contains("Permission denied") {
                for entry in thermal_text.lines() {
                    if entry.starts_with("thermal_zone") {
                        let zone_path = format!("/sys/class/thermal/{}", entry);
                        let temp_out = adb.shell(&serial, &["cat", &format!("{}/temp", zone_path)]);
                        if let Ok(temp_bytes) = temp_out {
                            let temp_str = String::from_utf8_lossy(&temp_bytes).trim().to_string();
                            if let Ok(milli_c) = temp_str.parse::<f64>() {
                                let type_out = adb.shell(&serial, &["cat", &format!("{}/type", zone_path)]);
                                let zone_type = type_out.map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
                                result.push_str(&format!("{}: {:.1}Â°C\n", zone_type, milli_c / 1000.0));
                            }
                        }
                    }
                }
            }
        }

        if result.is_empty() {
            result = "No temperature data available".to_string();
        }
        Ok(result.trim().to_string())
    })?
}

/// Run a thermal stress test: load CPU on-device, monitor temperature rise and throttling.
/// Uses real CPU workloads (NOT fake battery temp â€” that causes hardware shutdown).
/// duration_secs: how long to stress the CPU (default 30, max 120).
#[tauri::command]
async fn run_thermal_stress_test(serial: String, duration_secs: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        let dur = duration_secs.min(120).max(5); // clamp 5-120s
        let mut report = String::from("â•â•â• Thermal Stress Test â•â•â•\n\n");

        // â”€â”€ Helper: read battery temperature â”€â”€
        let read_battery_temp = |adb: &adb::AdbClient, serial: &str| -> Option<f64> {
            if let Ok(out) = adb.shell(serial, &["dumpsys", "battery"]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    if line.contains("temperature") {
                        if let Some(val) = line.split_whitespace().last() {
                            if let Ok(t) = val.parse::<f64>() {
                                return Some(t / 10.0);
                            }
                        }
                    }
                }
            }
            None
        };

        // â”€â”€ Helper: read all thermal sensor values â”€â”€
        let read_thermal_sensors = |adb: &adb::AdbClient, serial: &str| -> Vec<(String, f64)> {
            let mut sensors = Vec::new();
            if let Ok(out) = adb.shell(serial, &["dumpsys", "thermalservice"]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("{t:") || l.contains("type:") {
                        if let Some(type_start) = l.find("t:") {
                            let type_rest = &l[type_start + 2..];
                            if let Some(type_end) = type_rest.find(|c: char| c == ',' || c == '}') {
                                let sensor_type = type_rest[..type_end].trim().to_string();
                                if let Some(val_start) = l.find("v:") {
                                    let val_rest = &l[val_start + 2..];
                                    if let Some(val_end) = val_rest.find(|c: char| c == ',' || c == '}') {
                                        if let Ok(v) = val_rest[..val_end].trim().parse::<f64>() {
                                            sensors.push((sensor_type, v));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            sensors
        };

        // â”€â”€ Helper: read CPU frequency â”€â”€
        let read_cpu_freq = |adb: &adb::AdbClient, serial: &str| -> Option<String> {
            if let Ok(out) = adb.shell(serial, &["cat", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if let Ok(khz) = text.parse::<u64>() {
                    return Some(format!("{:.0} MHz", khz as f64 / 1000.0));
                }
                if !text.is_empty() && text != "Permission denied" {
                    return Some(text);
                }
            }
            None
        };

        // â”€â”€ Helper: read thermal status â”€â”€
        let read_thermal_status = |adb: &adb::AdbClient, serial: &str| -> String {
            if let Ok(out) = adb.shell(serial, &["dumpsys", "thermalservice"]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("mStatus") || l.contains("Status:") {
                        return l.to_string();
                    }
                }
            }
            "Unknown".to_string()
        };

        // â”€â”€ Helper: read CPU core count â”€â”€
        let read_cpu_cores = |adb: &adb::AdbClient, serial: &str| -> u32 {
            if let Ok(out) = adb.shell(serial, &["nproc"]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if let Ok(n) = text.parse::<u32>() {
                    return n;
                }
            }
            4 // default assumption
        };

        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        // PHASE 1: Baseline readings (before stress)
        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        report.push_str("â”€â”€ Phase 1: Baseline (Before Stress) â”€â”€\n");
        let baseline_batt = read_battery_temp(&adb, &serial);
        let baseline_sensors = read_thermal_sensors(&adb, &serial);
        let baseline_freq = read_cpu_freq(&adb, &serial);
        let baseline_status = read_thermal_status(&adb, &serial);
        let cpu_cores = read_cpu_cores(&adb, &serial);

        if let Some(t) = baseline_batt {
            report.push_str(&format!("  Battery Temperature: {:.1}Â°C\n", t));
        } else {
            report.push_str("  Battery Temperature: Not available\n");
        }
        if !baseline_sensors.is_empty() {
            report.push_str("  Thermal Sensors:\n");
            for (name, val) in &baseline_sensors {
                report.push_str(&format!("    {}: {:.1}Â°C\n", name, val));
            }
        }
        report.push_str(&format!("  CPU Frequency: {}\n", baseline_freq.as_deref().unwrap_or("N/A")));
        report.push_str(&format!("  CPU Cores: {}\n", cpu_cores));
        report.push_str(&format!("  Thermal Status: {}\n", baseline_status));

        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        // PHASE 2: CPU Stress (generate real heat)
        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        report.push_str(&format!("\nâ”€â”€ Phase 2: CPU Stress Test ({} seconds) â”€â”€\n", dur));
        report.push_str("  Starting CPU workload on device...\n");

        // Launch CPU stress on the device using multiple background shells
        // Each core gets a busy-loop doing arithmetic
        let stress_script = format!(
            "for i in $(seq 1 {}); do \
                (while true; do echo 'scale=10; 4*a(1)' | bc -l 2>/dev/null || \
                 awk 'BEGIN{{for(i=0;i<100000;i++)s+=sin(i)}}END{{print s}}' 2>/dev/null; done) & \
             done; \
             dd if=/dev/urandom bs=1M count=200 of=/dev/null 2>/dev/null; \
             sleep {}; \
             kill $(jobs -p) 2>/dev/null; \
             echo STRESS_DONE",
            cpu_cores, dur
        );

        // Fire-and-forget the stress script in background
        let _ = adb.shell(&serial, &["sh", "-c", &stress_script]);

        // â”€â”€ Monitor temperature during stress â”€â”€
        let mut _temp_readings: Vec<(u32, Option<f64>)> = Vec::new();
        let mut _freq_readings: Vec<(u32, Option<String>)> = Vec::new();
        let poll_interval = std::cmp::max(dur / 10, 3); // ~10 readings, min 3s apart
        let mut max_temp: f64 = baseline_batt.unwrap_or(0.0);
        let mut _min_freq: Option<String> = baseline_freq.clone();

        report.push_str(&format!("  Polling every {}s for {}s...\n\n", poll_interval, dur));
        report.push_str("  Time   | Battery Â°C | CPU Freq     | Thermal Status\n");
        report.push_str("  -------|------------|--------------|------------------\n");

        for elapsed in (0..=dur).step_by(poll_interval as usize) {
            std::thread::sleep(std::time::Duration::from_secs(poll_interval as u64));

            let batt_t = read_battery_temp(&adb, &serial);
            let cpu_f = read_cpu_freq(&adb, &serial);
            let status = read_thermal_status(&adb, &serial);

            if let Some(t) = batt_t {
                if t > max_temp { max_temp = t; }
            }
            if cpu_f.is_some() {
                _min_freq = cpu_f.clone();
            }

            _temp_readings.push((elapsed, batt_t));
            _freq_readings.push((elapsed, cpu_f.clone()));

            let temp_str = batt_t.map(|t| format!("{:.1}", t)).unwrap_or_else(|| "N/A".into());
            let freq_str = cpu_f.as_deref().unwrap_or("N/A");
            let short_status = if status.len() > 16 { &status[..16] } else { &status };
            report.push_str(&format!("  {:>4}s   | {:>7}Â°C   | {:>12} | {}\n",
                elapsed, temp_str, freq_str, short_status));
        }

        // Wait a moment for stress to fully end
        std::thread::sleep(std::time::Duration::from_secs(2));

        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        // PHASE 3: Post-stress analysis
        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        report.push_str("\nâ”€â”€ Phase 3: Post-Stress Analysis â”€â”€\n");
        let post_batt = read_battery_temp(&adb, &serial);
        let post_sensors = read_thermal_sensors(&adb, &serial);
        let post_freq = read_cpu_freq(&adb, &serial);
        let post_status = read_thermal_status(&adb, &serial);

        if let Some(t) = post_batt {
            report.push_str(&format!("  Battery Temperature: {:.1}Â°C\n", t));
        }
        if !post_sensors.is_empty() {
            report.push_str("  Thermal Sensors:\n");
            for (name, val) in &post_sensors {
                report.push_str(&format!("    {}: {:.1}Â°C\n", name, val));
            }
        }
        report.push_str(&format!("  CPU Frequency: {}\n", post_freq.as_deref().unwrap_or("N/A")));
        report.push_str(&format!("  Thermal Status: {}\n", post_status));

        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        // PHASE 4: Summary & Performance Analysis
        // â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
        report.push_str("\nâ”€â”€ Thermal Performance Summary â”€â”€\n");

        let _temp_rise = if let (Some(base), Some(post)) = (baseline_batt, post_batt) {
            let rise = post - base;
            report.push_str(&format!("  Temperature Rise: {:.1}Â°C ({:.1}Â°C â†’ {:.1}Â°C)\n", rise, base, post));
            Some(rise)
        } else {
            report.push_str("  Temperature Rise: Could not measure\n");
            None
        };

        // Detect throttling: compare baseline vs post frequency numerically
        // Helper to parse "1234 MHz" -> 1234
        let parse_mhz = |s: &str| -> Option<f64> {
            s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok())
        };
        let base_mhz = baseline_freq.as_deref().and_then(|s| parse_mhz(s));
        let post_mhz = post_freq.as_deref().and_then(|s| parse_mhz(s));

        // Track minimum frequency seen during stress
        let min_during_stress: Option<f64> = _freq_readings.iter()
            .filter_map(|(_, f)| f.as_deref().and_then(|s| parse_mhz(s)))
            .fold(None, |min: Option<f64>, v| Some(min.map_or(v, |m: f64| m.min(v))));

        let throttled = match (base_mhz, post_mhz) {
            (Some(before), Some(after)) if after < before => {
                report.push_str(&format!("  CPU Frequency DROPPED: {:.0} MHz â†’ {:.0} MHz\n", before, after));
                Some(format!("{:.0} MHz â†’ {:.0} MHz", before, after))
            }
            _ => None
        };

        // Also check if thermal status changed from normal (0) to throttled
        let thermal_mitigation = baseline_status != post_status
            && !baseline_status.is_empty() && !post_status.is_empty();

        // Thermal status change
        if baseline_status != post_status {
            report.push_str(&format!("  Thermal Status Change: {} â†’ {}\n", baseline_status, post_status));
        }

        // Risk assessment
        report.push_str("\nâ”€â”€ Risk Assessment â”€â”€\n");
        let peak = max_temp;
        if peak >= 50.0 {
            report.push_str("  âš  CRITICAL: Peak temperature exceeded 50Â°C!\n");
            report.push_str("  Device may trigger thermal shutdown or permanent damage.\n");
            report.push_str("  Recommendation: Stop stress test immediately, allow cooling.\n");
        } else if peak >= 45.0 {
            report.push_str("  âš  WARNING: Peak temperature reached 45-50Â°C range.\n");
            report.push_str("  Device is approaching thermal throttling threshold.\n");
            report.push_str("  Recommendation: Monitor closely, reduce workload.\n");
        } else if peak >= 40.0 {
            report.push_str("  MODERATE: Peak temperature in 40-45Â°C range.\n");
            report.push_str("  Normal for sustained CPU load. No immediate risk.\n");
        } else if peak > 0.0 {
            report.push_str(&format!("  SAFE: Peak temperature was {:.1}Â°C â€” well within limits.\n", peak));
        }

        if let Some(ref freq_change) = throttled {
            report.push_str(&format!("\n  âš  CPU THROTTLING DETECTED: {}\n", freq_change));
            report.push_str("  The device reduced CPU frequency to manage heat.\n");
            report.push_str("  This indicates the thermal management system is active.\n");
        } else if let Some(min_mhz) = min_during_stress {
            let base_m = base_mhz.unwrap_or(0.0);
            if base_m > 0.0 && min_mhz < base_m * 0.5 {
                report.push_str(&format!("\n  âš  CPU frequency dipped to {:.0} MHz during stress (baseline: {:.0} MHz)\n", min_mhz, base_mhz.unwrap_or(0.0)));
                report.push_str("  Significant frequency reduction suggests thermal or power throttling.\n");
            } else {
                report.push_str(&format!("\n  No CPU throttling detected. Min freq during stress: {:.0} MHz (baseline: {:.0} MHz)\n", min_mhz, base_mhz.unwrap_or(0.0)));
            }
        } else {
            report.push_str("\n  No CPU throttling detected during this test.\n");
        }

        if thermal_mitigation {
            report.push_str(&format!("  âš  Thermal status changed: {} â†’ {}\n", baseline_status, post_status));
            report.push_str("  The device entered a higher thermal mitigation level.\n");
        }

        // Device info
        let model = adb.shell(&serial, &["getprop", "ro.product.model"])
            .map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
        let chipset = adb.shell(&serial, &["getprop", "ro.hardware"])
            .map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
        if !model.is_empty() {
            report.push_str(&format!("\n  Device: {}\n", model));
        }
        if !chipset.is_empty() {
            report.push_str(&format!("  Chipset: {}\n", chipset));
        }
        report.push_str(&format!("  Stress Duration: {}s on {} cores\n", dur, cpu_cores));

        Ok(report)
    }).await.map_err(|e| e.to_string())?
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
fn humantime_format(unix_secs: u64) -> String {
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

/// Collect device identity: IMEI, ESN, MEID, serial number, model, brand, etc.
/// Uses multiple ADB methods for maximum compatibility.
#[tauri::command]
fn collect_device_identity(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Device Identity â•â•â•\n");

        // IMEI (GSM devices)
        let imei_methods = [
            (vec!["service", "call", "iphonesubinfo", "1"], "service call"),
            (vec!["dumpsys", "iphonesubinfo"], "dumpsys"),
            (vec!["getprop", "gsm.imei"], "getprop"),
        ];
        let mut imei_found = false;
        for (cmd, method) in &imei_methods {
            if let Ok(out) = adb.shell(&serial, cmd) {
                let text = String::from_utf8_lossy(&out);
                // Parse IMEI from different output formats
                let imei = if text.contains("'") {
                    // service call format: Result received... '123456789012345'
                    text.split('\'').filter(|s| s.chars().all(|c| c.is_ascii_digit()) && s.len() >= 14)
                        .next().unwrap_or("").to_string()
                } else if text.contains("Device ID:") {
                    text.lines().find(|l| l.contains("Device ID:"))
                        .map(|l| l.split(':').nth(1).unwrap_or("").trim().to_string())
                        .unwrap_or_default()
                } else {
                    text.trim().to_string()
                };
                if imei.len() >= 14 && imei.chars().all(|c| c.is_ascii_digit()) {
                    report.push_str(&format!("IMEI: {} (via {})\n", imei, method));
                    imei_found = true;
                    break;
                }
            }
        }
        if !imei_found {
            report.push_str("IMEI: Not available (may be CDMA-only or restricted)\n");
        }

        // MEID/ESN (CDMA devices)
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.cdma.meid"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() { report.push_str(&format!("MEID: {}\n", v)); }
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.cdma.esn"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() { report.push_str(&format!("ESN: {}\n", v)); }
        }

        // Serial number
        let serial_methods = [
            vec!["getprop", "ro.serialno"],
            vec!["getprop", "ro.boot.serialno"],
            vec!["settings", "get", "global", "android_id"],
        ];
        for cmd in &serial_methods {
            if let Ok(out) = adb.shell(&serial, cmd) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() && v != "unknown" {
                    report.push_str(&format!("Serial ({}): {}\n", cmd[cmd.len()-1], v));
                }
            }
        }

        // Brand/Model/Device
        let props = [
            ("ro.product.brand", "Brand"),
            ("ro.product.model", "Model"),
            ("ro.product.device", "Device"),
            ("ro.product.manufacturer", "Manufacturer"),
            ("ro.build.version.release", "Android Version"),
            ("ro.build.version.sdk", "SDK Level"),
            ("ro.build.display.id", "Build ID"),
            ("ro.build.fingerprint", "Fingerprint"),
        ];
        for (prop, label) in &props {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() { report.push_str(&format!("{}: {}\n", label, v)); }
            }
        }

        Ok(report)
    })?
}

/// Extract lock screen information: type, status, and security settings.
/// Note: actual PIN/pattern cannot be extracted (Android security boundary),
/// but we can determine the lock type and whether it's set.
#[tauri::command]
fn extract_lock_info(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Lock Screen Info â•â•â•\n\n");

        // Determine lock type from password_type value
        report.push_str("â”€â”€ Lock Configuration â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "lockscreen.password_type"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            let lock_type = match v.as_str() {
                "0" => "None (no lock)",
                "65536" => "Pattern",
                "131072" => "PIN (4+ digits)",
                "262144" => "Password (alphanumeric)",
                "524288" => "Managed (work profile)",
                other => other,
            };
            report.push_str(&format!("  Lock Type: {}\n", lock_type));
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "lockscreen.password_type_alternate"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                let alt_type = match v.as_str() {
                    "0" => "None",
                    "65536" => "Pattern",
                    "131072" => "PIN",
                    "262144" => "Password",
                    other => other,
                };
                report.push_str(&format!("  Alt Lock Type: {}\n", alt_type));
            }
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "lockscreen.disabled"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Lock Disabled: {}\n", if v == "1" { "Yes" } else { "No" }));
            }
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "lock_screen_show_notifications"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Show Notifications: {}\n", if v == "1" { "Yes" } else { "No" }));
            }
        }

        // Security patches
        report.push_str("\nâ”€â”€ Security Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.build.version.security_patch"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() { report.push_str(&format!("  Security Patch: {}\n", v)); }
        }

        // Encryption status
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.state"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Encryption: {}\n", if v.is_empty() { "unknown" } else { &v }));
        }

        // Trust agents â€” parsed
        report.push_str("\nâ”€â”€ Trust Agents â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "trust"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_agents = false;
            for line in text.lines() {
                let l = line.trim();
                // Extract trust agent info
                if l.contains("Agent") || l.contains("agent") || l.contains("trust") {
                    if l.contains("enabled") || l.contains("disabled") || l.contains("trusted") || l.contains("Agent:") {
                        report.push_str(&format!("  {}\n", l));
                        found_agents = true;
                    }
                }
            }
            if !found_agents { report.push_str("  No trust agents configured\n"); }
        }

        // Biometric status
        report.push_str("\nâ”€â”€ Biometric Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "biometric"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_bio = false;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("enrolled") || l.contains("BiometricManager") || l.contains("fingerprint") || l.contains("face") {
                    if l.contains("=") || l.contains(":") {
                        report.push_str(&format!("  {}\n", l));
                        found_bio = true;
                    }
                }
            }
            if !found_bio { report.push_str("  No biometric data accessible\n"); }
        }

        Ok(report)
    })?
}

/// Recover deleted contacts by parsing contacts2.db WAL and journal files.
/// Pulls the database from the device and scans for deleted rows.
#[tauri::command]
fn recover_deleted_contacts(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Deleted Contacts Recovery â•â•â•\n");

        // Try to pull contacts database
        let db_paths = [
            "/data/data/com.android.providers.contacts/databases/contacts2.db",
            "/data/user/0/com.android.providers.contacts/databases/contacts2.db",
        ];

        let mut db_found = false;
        for db_path in &db_paths {
            let local_db = format!("{}\\contacts2.db", output_dir);
            if let Some(parent) = std::path::Path::new(&local_db).parent() {
                let _ = std::fs::create_dir_all(parent);
            }

            // Try via run-as (debuggable builds) or root
            let pull_cmd = format!("run-as com.android.providers.contacts cat {}", db_path);
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &pull_cmd]) {
                if !out.is_empty() {
                    let _ = std::fs::write(&local_db, &out);
                    report.push_str(&format!("Pulled contacts DB via run-as: {} bytes\n", out.len()));
                    db_found = true;
                    break;
                }
            }

            // Try direct pull (root)
            if adb.pull(&serial, db_path, std::path::Path::new(&local_db)).is_ok() {
                let size = std::fs::metadata(&local_db).map(|m| m.len()).unwrap_or(0);
                if size > 100 {
                    report.push_str(&format!("Pulled contacts DB via root: {} bytes\n", size));
                    db_found = true;
                    break;
                }
            }
        }

        if !db_found {
            report.push_str("Could not pull contacts2.db â€” device requires root or debuggable build\n");
            report.push_str("Alternative: Using content provider query...\n\n");

            // Fallback: query contacts via content provider
            if let Ok(out) = adb.shell(&serial, &[
                "content", "query", "--uri", "content://com.android.contacts/data",
                "--projection", "mimetype_id:data1:display_name",
            ]) {
                let text = String::from_utf8_lossy(&out);
                let lines: Vec<&str> = text.lines().collect();
                report.push_str(&format!("Content query returned {} rows\n", lines.len().saturating_sub(1)));
                for line in lines.iter().take(50) {
                    report.push_str(line);
                    report.push('\n');
                }
            }
        } else {
            // Parse the SQLite database for deleted records
            let local_db = format!("{}\\contacts2.db", output_dir);
            match rusqlite::Connection::open(&local_db) {
                Ok(conn) => {
                    // Check for contacts in the raw_contacts table
                    let count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM raw_contacts", [], |r| r.get(0)
                    ).unwrap_or(0);
                    report.push_str(&format!("Total raw contacts in DB: {}\n", count));

                    // Check for deleted entries (deleted=1 flag)
                    let deleted: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM raw_contacts WHERE deleted=1", [], |r| r.get(0)
                    ).unwrap_or(0);
                    report.push_str(&format!("Marked as deleted: {}\n", deleted));

                    // Try to read deleted contacts
                    if deleted > 0 {
                        let mut stmt = conn.prepare(
                            "SELECT d.data1, d.mimetype_id FROM data d JOIN raw_contacts rc ON d.raw_contact_id=rc._id WHERE rc.deleted=1 LIMIT 100"
                        ).ok();
                        if let Some(ref mut s) = stmt {
                            let rows = s.query_map([], |row| {
                                Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
                            }).ok();
                            if let Some(rows) = rows {
                                let mut found = 0;
                                for row in rows.flatten() {
                                    if let (Some(data), mime_id) = row {
                                        let mime = match mime_id {
                                            5 => "phone",
                                            1 => "name",
                                            _ => "other",
                                        };
                                        report.push_str(&format!("  [DELETED] ({}): {}\n", mime, data));
                                        found += 1;
                                    }
                                }
                                report.push_str(&format!("Recovered {} deleted contact entries\n", found));
                            }
                        }
                    }
                }
                Err(e) => {
                    report.push_str(&format!("Failed to open DB: {}\n", e));
                }
            }
        }

        Ok(report)
    })?
}

/// Collect cell tower connection history from telephony registry.
/// Parses raw dumpsys output into a clean, structured forensic report.
#[tauri::command]
fn collect_cell_tower_history(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Cell Tower History â•â•â•\n\n");

        // Helper to extract a value from a line like "mField=value" or "mField=value(desc)"
        fn extract<'a>(text: &'a str, key: &str) -> Option<&'a str> {
            text.find(key).map(|i| {
                let start = i + key.len();
                let rest = &text[start..];
                // skip '=' or ': ' if present
                let rest = rest.trim_start_matches('=').trim_start_matches(": ");
                let end = rest.find(|c: char| c == ' ' || c == ',' || c == '}' || c == ')' || c == '(' || c == '\n')
                    .unwrap_or(rest.len());
                rest[..end].trim()
            }).filter(|s| !s.is_empty() && *s != "null" && *s != "2147483647")
        }

        // â”€â”€ 1. Current Cell Identity â”€â”€
        report.push_str("â”€â”€ Current Cell Identity â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "telephony.registry"]) {
            let text = String::from_utf8_lossy(&out);

            // Find the registered cell (mRegistered=YES)
            let mut found_registered = false;
            for block in text.split("CellInfoLte:") {
                if block.contains("mRegistered=YES") {
                    // Parse identity fields
                    let cid = extract(block, "mCi=").unwrap_or("?");
                    let pci = extract(block, "mPci=").unwrap_or("?");
                    let tac = extract(block, "mTac=").unwrap_or("?");
                    let earfcn = extract(block, "mEarfcn=").unwrap_or("?");
                    let mcc = extract(block, "mMcc=").unwrap_or("?");
                    let mnc = extract(block, "mMnc=").unwrap_or("?");
                    let alpha = extract(block, "mAlphaLong=").unwrap_or("?");
                    let bandwidth = extract(block, "mBandwidth=").unwrap_or("?");
                    let bands = extract(block, "mBands=").unwrap_or("?");

                    // Parse signal strength
                    let rssi = extract(block, "rssi=").unwrap_or("?");
                    let rsrp = extract(block, "rsrp=").unwrap_or("?");
                    let rsrq = extract(block, "rsrq=").unwrap_or("?");
                    let rssnr = extract(block, "rssnr=").unwrap_or("?");
                    let level = extract(block, "level=").unwrap_or("?");
                    let ta = extract(block, "ta=").unwrap_or("?");

                    report.push_str(&format!("  Operator:      {} (MCC: {}, MNC: {})\n", alpha, mcc, mnc));
                    report.push_str(&format!("  Cell ID (CID): {}\n", cid));
                    report.push_str(&format!("  PCI:           {}\n", pci));
                    report.push_str(&format!("  TAC:           {}\n", tac));
                    report.push_str(&format!("  EARFCN:        {}\n", earfcn));
                    report.push_str(&format!("  Band:          {} (width: {} kHz)\n", bands, bandwidth));
                    report.push_str(&format!("  â”€â”€ Signal â”€â”€\n"));
                    report.push_str(&format!("  RSSI:          {} dBm\n", rssi));
                    report.push_str(&format!("  RSRP:          {} dBm\n", rsrp));
                    report.push_str(&format!("  RSRQ:          {} dB\n", rsrq));
                    report.push_str(&format!("  RSSNR:         {} dB\n", rssnr));
                    report.push_str(&format!("  Level:         {}/4\n", level));
                    report.push_str(&format!("  Timing Adv:    {}\n", ta));
                    found_registered = true;
                    break;
                }
            }
            if !found_registered {
                report.push_str("  No registered LTE cell found\n");
            }

            // â”€â”€ 2. Service State â”€â”€
            report.push_str("\nâ”€â”€ Service State â”€â”€\n");
            for line in text.lines() {
                if line.contains("mServiceState=") {
                    let voice = extract(line, "mVoiceRegState=").unwrap_or("?");
                    let data = extract(line, "mDataRegState=").unwrap_or("?");
                    let tech = extract(line, "getRilDataRadioTechnology=").unwrap_or("?");
                    let operator = extract(line, "mOperatorAlphaLong=").unwrap_or("?");
                    let roaming = if line.contains("roamingType=NOT_ROAMING") { "No" }
                        else { extract(line, "roamingType=").unwrap_or("?") };
                    let ca = if line.contains("isUsingCarrierAggregation=true") { "Yes" } else { "No" };

                    // Decode radio tech
                    let tech_name = match tech {
                        "14" => "LTE",
                        "13" => "eHRPD",
                        "15" => "LTE Advanced",
                        "18" => "NR (5G)",
                        "0" => "Unknown",
                        other => other,
                    };

                    let voice_name = match voice {
                        "0" => "IN SERVICE",
                        "1" => "OUT OF SERVICE",
                        "2" => "EMERGENCY ONLY",
                        "3" => "POWER OFF",
                        other => other,
                    };

                    report.push_str(&format!("  Voice:         {}\n", voice_name));
                    report.push_str(&format!("  Data:          {}\n", if data == "0" { "IN SERVICE" } else { "OUT OF SERVICE" }));
                    report.push_str(&format!("  Technology:    {}\n", tech_name));
                    report.push_str(&format!("  Operator:      {}\n", operator));
                    report.push_str(&format!("  Roaming:       {}\n", roaming));
                    report.push_str(&format!("  Carrier Agg:   {}\n", ca));
                    break; // only first mServiceState
                }
            }

            // â”€â”€ 3. Neighboring Cells â”€â”€
            report.push_str("\nâ”€â”€ Neighboring Cells â”€â”€\n");
            let mut neighbor_count = 0;
            for block in text.split("CellInfoLte:") {
                if block.contains("mRegistered=NO") {
                    let pci = extract(block, "mPci=").unwrap_or("?");
                    let cid = extract(block, "mCi=").unwrap_or("?");
                    let earfcn = extract(block, "mEarfcn=").unwrap_or("?");
                    let rssi = extract(block, "rssi=").unwrap_or("?");
                    let rsrp = extract(block, "rsrp=").unwrap_or("?");
                    let level = extract(block, "level=").unwrap_or("?");
                    report.push_str(&format!("  PCI:{:>4}  CID:{:>10}  EARFCN:{:>6}  RSSI:{:>4}dBm  RSRP:{:>4}dBm  Level:{}/4\n",
                        pci, cid, earfcn, rssi, rsrp, level));
                    neighbor_count += 1;
                    if neighbor_count >= 15 { break; }
                }
            }
            if neighbor_count == 0 {
                report.push_str("  No neighboring cells detected\n");
            } else {
                report.push_str(&format!("  ({} neighbors)\n", neighbor_count));
            }

            // â”€â”€ 4. State Change History (timestamps) â”€â”€
            report.push_str("\nâ”€â”€ State Change History â”€â”€\n");
            let mut event_count = 0;
            for line in text.lines() {
                if line.contains("notifyVivoServiceState") || line.contains("notifyServiceState") {
                    // Extract timestamp
                    if let Some(ts_end) = line.find(" - ") {
                        let timestamp = &line[..ts_end];
                        let event_type = if line.contains("notifyVivo") { "Vivo Notify" } else { "Service State" };
                        let earfcn = extract(line, "mChannelNumber=").unwrap_or("?");
                        let tech = extract(line, "getRilDataRadioTechnology=").unwrap_or("?");
                        let tech_name = match tech { "14" => "LTE", "18" => "5G", "0" => "?", o => o };
                        let reg = if line.contains("mVoiceRegState=0") { "IN_SVC" } else { "OUT_SVC" };

                        report.push_str(&format!("  [{}] {} | EARFCN:{} | {} | {}\n",
                            timestamp, event_type, earfcn, tech_name, reg));
                        event_count += 1;
                        if event_count >= 20 {
                            report.push_str("  ... (truncated)\n");
                            break;
                        }
                    }
                }
            }
            if event_count == 0 {
                report.push_str("  No state change events in registry\n");
            }

            // â”€â”€ 5. SIM / Network Properties â”€â”€
            report.push_str("\nâ”€â”€ SIM & Network Properties â”€â”€\n");

            // Helper: decode hex from `service call` Parcel output to ASCII string
            // Service call returns: "Result: Parcel(00000000 00000010 '........' 00000000 ...)"
            // The hex blocks are the actual byte data; we decode them to ASCII.
            fn decode_service_call_hex(raw: &str) -> String {
                let mut result = String::new();
                for word in raw.split_whitespace() {
                    // Only process 8-char hex words (4 bytes each)
                    if word.len() == 8 && word.chars().all(|c| c.is_ascii_hexdigit()) {
                        for i in (0..8).step_by(2) {
                            if let Ok(byte) = u8::from_str_radix(&word[i..i+2], 16) {
                                if byte >= 0x20 && byte < 0x7f {
                                    result.push(byte as char);
                                }
                            }
                        }
                    }
                }
                result
            }

            // Simple getprop values
            let simple_props: &[(&str, &str)] = &[
                ("SIM Operator", "gsm.sim.operator.alpha"),
                ("SIM Country", "gsm.sim.operator.iso-country"),
                ("SIM MCC/MNC", "gsm.sim.operator.numeric"),
                ("Signal (dBm)", "gsm.signal.strength"),
            ];
            for (label, prop) in simple_props {
                if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                    let v = String::from_utf8_lossy(&out).trim().to_string();
                    if !v.is_empty() && v != "null" {
                        report.push_str(&format!("  {}: {}\n", label, v));
                    }
                }
            }

            // Network Type â€” take first value only (avoid "LTE,Unknown")
            if let Ok(out) = adb.shell(&serial, &["getprop", "gsm.network.type"]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                let first = v.split(',').next().unwrap_or(&v).trim();
                if !first.is_empty() && first != "null" {
                    report.push_str(&format!("  Network Type: {}\n", first));
                }
            }

            // SIM State â€” take first value only (avoid "LOADED,ABSENT")
            if let Ok(out) = adb.shell(&serial, &["getprop", "gsm.sim.state"]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                let first = v.split(',').next().unwrap_or(&v).trim();
                if !first.is_empty() && first != "null" {
                    report.push_str(&format!("  SIM State: {}\n", first));
                }
            }

            // IMEI â€” decode hex from service call iphonesubinfo 1 (GET_DEVICE_ID)
            if let Ok(out) = adb.shell(&serial, &["service", "call", "iphonesubinfo", "1"]) {
                let raw = String::from_utf8_lossy(&out);
                let decoded = decode_service_call_hex(&raw);
                // IMEI is 15 digits; extract only digit chars from decoded
                let imei: String = decoded.chars().filter(|c| c.is_ascii_digit()).collect();
                if imei.len() >= 14 && !imei.chars().all(|c| c == '0') {
                    report.push_str(&format!("  IMEI:          {}\n", imei));
                } else {
                    // Fallback: try getprop
                    if let Ok(out2) = adb.shell(&serial, &["getprop", "gsm.imei"]) {
                        let v = String::from_utf8_lossy(&out2).trim().to_string();
                        if !v.is_empty() && v != "null" && v != "0" {
                            report.push_str(&format!("  IMEI:          {}\n", v));
                        } else {
                            report.push_str("  IMEI:          Not accessible (requires permission)\n");
                        }
                    }
                }
            }

            // SIM Number (phone number) â€” decode hex from service call iphonesubinfo 13
            // Note: On many modern carriers (Jio, etc.), the phone number is NOT stored
            // on the SIM â€” it's assigned by the network at registration time.
            if let Ok(out) = adb.shell(&serial, &["service", "call", "iphonesubinfo", "13"]) {
                let raw = String::from_utf8_lossy(&out);
                let decoded = decode_service_call_hex(&raw);
                // Phone numbers are 4-15 digits, optionally starting with +
                // Filter out ICCID-like data (20 digits starting with 89 or 98)
                let phone: String = decoded.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
                let looks_like_phone = phone.len() >= 4 && phone.len() <= 15
                    && !phone.starts_with("89") && !phone.starts_with("98")
                    && !phone.chars().all(|c| c == '0');
                if looks_like_phone {
                    report.push_str(&format!("  SIM Number:    {}\n", phone));
                } else {
                    report.push_str("  SIM Number:    Not stored on SIM (carrier-assigned)\n");
                }
            }

            // ICCID (SIM card serial) â€” decode hex from service call iphonesubinfo 11
            let mut iccid = String::new();
            // Method 1: getprop (multiple property names)
            for prop in &["gsm.sn.1", "ril.iccid.sim1", "persist.radio.iccid", "gsm.sim.iccid"] {
                if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                    let v = String::from_utf8_lossy(&out).trim().to_string();
                    if !v.is_empty() && v != "null" && v.len() >= 10 {
                        iccid = v;
                        break;
                    }
                }
            }
            // Method 2: service call hex decode (iphonesubinfo 11 = GET_ICCID)
            if iccid.is_empty() {
                if let Ok(out) = adb.shell(&serial, &["service", "call", "iphonesubinfo", "11"]) {
                    let raw = String::from_utf8_lossy(&out);
                    let decoded = decode_service_call_hex(&raw);
                    // ICCID is 19-20 digits starting with 89
                    let digits: String = decoded.chars().filter(|c| c.is_ascii_digit()).collect();
                    // Find a substring that looks like an ICCID (starts with 89, 19-20 digits)
                    if let Some(pos) = digits.find("89") {
                        let candidate = &digits[pos..];
                        let iccid_len = candidate.len().min(20);
                        let iccid_candidate = &candidate[..iccid_len];
                        if iccid_candidate.len() >= 18 {
                            iccid = iccid_candidate.to_string();
                        }
                    } else if digits.len() >= 18 {
                        iccid = digits[..digits.len().min(20)].to_string();
                    }
                }
            }
            // Method 3: dumpsys iphonesubinfo
            if iccid.is_empty() {
                if let Ok(out) = adb.shell(&serial, &["dumpsys", "iphonesubinfo"]) {
                    let raw = String::from_utf8_lossy(&out);
                    // Look for ICCID in various formats
                    for pattern in &["ICCID: ", "mIccId=", "iccid: "] {
                        if let Some(pos) = raw.find(pattern) {
                            let rest = &raw[pos + pattern.len()..];
                            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                            if digits.len() >= 18 {
                                iccid = digits;
                                break;
                            }
                        }
                    }
                }
            }
            if !iccid.is_empty() {
                report.push_str(&format!("  ICCID (SIM):   {}\n", iccid));
            } else {
                report.push_str("  ICCID (SIM):   Not accessible\n");
            }
        }

        Ok(report)
    })?
}

/// Detect anti-forensics: app wipers, cache cleaners, evidence of tampering.
#[tauri::command]
fn detect_anti_forensics(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Anti-Forensics Detection â•â•â•\n");

        // Known wiper/cleaner apps
        let wiper_apps = [
            ("com.privacyfixer", "PrivacyFixer"),
            ("com.cleanmaster.mguard", "CleanMaster"),
            ("com.antimalware.mobile", "AntiMalware"),
            ("com.iwob.soft.delete", "SoftDelete"),
            ("com.app2sd.divya", "App2SD"),
            ("com.wipedata.pro", "WipeData Pro"),
            ("com.cyberlink.youcam", "YouCam"),
            ("com.bsafe.bsafe", "bSafe"),
            ("com.coverme.coverme", "CoverMe"),
            ("com.signal.signal_private", "Signal (private)"),
            ("org.thoughtcrime.securesms", "SecureSMS"),
            ("com.secureé€šè®¯", "SecureComm"),
        ];

        report.push_str("â”€â”€ Known Wiper/Cleaner Apps â”€â”€\n");
        let mut found_any = false;
        for (pkg, name) in &wiper_apps {
            if let Ok(out) = adb.shell(&serial, &["pm", "list", "packages", pkg]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if text.contains(pkg) {
                    report.push_str(&format!("  âš  FOUND: {} ({})\n", name, pkg));
                    found_any = true;
                }
            }
        }
        if !found_any { report.push_str("  âœ… No known wiper apps detected\n"); }

        // Check for recently cleared app data (suspicious)
        report.push_str("\nâ”€â”€ Recently Cleared App Data â”€â”€\n");
        let suspect_apps = ["com.whatsapp", "com.android.messaging", "com.android.contacts",
                           "com.google.android.dialer", "com.sec.android.app.camera"];
        for pkg in &suspect_apps {
            if let Ok(out) = adb.shell(&serial, &["dumpsys", "package", pkg, "|", "grep", "lastUpdateTime"]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() {
                    report.push_str(&format!("  {}: {}\n", pkg, text));
                }
            }
        }

        // Check for factory reset indicators
        report.push_str("\nâ”€â”€ Factory Reset Indicators â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.boot.flash.locked"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Bootloader locked: {}\n", if v == "1" { "Yes" } else { "No/Unknown" }));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.debuggable"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Debuggable: {}\n", if v == "1" { "YES (suspicious)" } else { "No" }));
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "device_provisioned"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Device provisioned: {}\n", v));
        }

        // Check for custom recovery (TWRP etc.)
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/version"]) {
            let text = String::from_utf8_lossy(&out).to_lowercase();
            if text.contains("twrp") || text.contains("orangefox") || text.contains("pitchblack") {
                report.push_str("  âš  Custom recovery detected!\n");
            }
        }

        Ok(report)
    })?
}

/// Collect dual SIM information: both slots, per-SIM details, subscription data.
/// Parses raw Android properties into a clean forensic report.
#[tauri::command]
fn collect_dual_sim_info(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Dual SIM Analysis â•â•â•\n\n");

        // â”€â”€ 1. SIM slot count (try multiple properties) â”€â”€
        let mut sim_count = 0;
        for prop in &["ro.telephony.sim.count", "persist.radio.multisim.config", "ro.telephony.default_network"] {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if let Some(n) = v.chars().find(|c| c.is_ascii_digit()) {
                    sim_count = n.to_digit(10).unwrap_or(1) as usize;
                    if sim_count > 0 { break; }
                }
            }
        }
        if sim_count == 0 { sim_count = 1; }
        report.push_str(&format!("SIM slots detected: {}\n", sim_count));

        // â”€â”€ 2. Per-slot info â”€â”€
        // Android stores multi-SIM info as comma-separated values:
        // gsm.sim.state = "LOADED,ABSENT" â†’ slot1=LOADED, slot2=ABSENT
        let get_multi_prop = |prop: &str| -> Vec<String> {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if v.is_empty() || v == "null" || v == "unknown" {
                    return vec![];
                }
                v.split(',').map(|s| s.trim().to_string()).collect()
            } else {
                vec![]
            }
        };

        let states = get_multi_prop("gsm.sim.state");
        let operators = get_multi_prop("gsm.sim.operator.alpha");
        let mcc_mncs = get_multi_prop("gsm.sim.operator.numeric");
        let countries = get_multi_prop("gsm.sim.operator.iso-country");

        for slot in 0..sim_count.max(2) {
            report.push_str(&format!("\nâ”€â”€ SIM Slot {} â”€â”€\n", slot + 1));
            let mut has_data = false;

            let state = states.get(slot).map(|s| s.as_str()).unwrap_or("ABSENT");
            let operator = operators.get(slot).map(|s| s.as_str()).unwrap_or("");
            let mcc_mnc = mcc_mncs.get(slot).map(|s| s.as_str()).unwrap_or("");
            let country = countries.get(slot).map(|s| s.as_str()).unwrap_or("");

            if state != "ABSENT" && !state.is_empty() {
                has_data = true;
            }

            report.push_str(&format!("  State:    {}\n", state));
            if !operator.is_empty() {
                report.push_str(&format!("  Operator: {}\n", operator));
                has_data = true;
            }
            if !mcc_mnc.is_empty() {
                report.push_str(&format!("  MCC/MNC:  {}\n", mcc_mnc));
                has_data = true;
            }
            if !country.is_empty() {
                report.push_str(&format!("  Country:  {}\n", country));
                has_data = true;
            }

            // ICCID per slot
            for prop in &[format!("gsm.sn.{}", slot + 1), format!("ril.iccid.sim{}", slot + 1)] {
                if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                    let v = String::from_utf8_lossy(&out).trim().to_string();
                    if !v.is_empty() && v != "null" && v.len() >= 10 {
                        report.push_str(&format!("  ICCID:    {}\n", v));
                        has_data = true;
                        break;
                    }
                }
            }

            if !has_data && slot > 0 {
                report.push_str("  (empty or no SIM present)\n");
            }
        }

        // â”€â”€ 3. Subscription Info (parsed) â”€â”€
        report.push_str("\nâ”€â”€ Active Subscriptions â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "iphonesubinfo"]) {
            let text = String::from_utf8_lossy(&out);
            let mut in_section = false;
            for line in text.lines() {
                let l = line.trim();
                // Extract key info lines
                if l.starts_with("Phone Sub Info") || l.starts_with("Subscription Info") {
                    in_section = true;
                    continue;
                }
                if in_section && (l.contains("Device ID") || l.contains("Line 1 Number")
                    || l.contains("ICCID") || l.contains("IMS") || l.contains("Group Id"))
                {
                    report.push_str(&format!("  {}\n", l));
                }
                if in_section && l.is_empty() { in_section = false; }
            }
        }

        // Parse subscription list from content provider
        if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri",
            "content://telephony/subscription",
            "--projection", "_id:display_name:carrier_name:icc_id:subscription_type:is_opportunistic"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_sub = false;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Row") || l.contains("_id=") {
                    report.push_str(&format!("  {}\n", l));
                    found_sub = true;
                }
            }
            if !found_sub {
                report.push_str("  No subscription records accessible via content provider\n");
            }
        }

        // â”€â”€ 4. Default SIM settings â”€â”€
        report.push_str("\nâ”€â”€ Default SIM Settings â”€â”€\n");
        let sim_settings = [
            ("Voice call default", "voice_calls_subscription"),
            ("SMS default", "sms_subscription"),
            ("Data default", "data_subscription"),
        ];
        for (label, setting) in &sim_settings {
            if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", setting]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() && v != "null" {
                    let sub_name = match v.as_str() {
                        "0" => "SIM 1",
                        "1" => "SIM 2",
                        other => other,
                    };
                    report.push_str(&format!("  {}: {}\n", label, sub_name));
                }
            }
        }

        // â”€â”€ 5. SIM lock status â”€â”€
        report.push_str("\nâ”€â”€ SIM Lock Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "gsm.sim.state"]) {
            let raw = String::from_utf8_lossy(&out).trim().to_string();
            for (i, state) in raw.split(',').enumerate() {
                let state = state.trim();
                let lock_status = match state {
                    "READY" | "LOADED" => "Unlocked & Ready",
                    "ABSENT" => "No SIM present",
                    "NOT_READY" => "SIM not ready",
                    "PIN_REQUIRED" => "PIN locked",
                    "PUK_REQUIRED" => "PUK locked",
                    "PERM_DISABLED" => "Permanently disabled",
                    other => other,
                };
                report.push_str(&format!("  SIM {}: {}\n", i + 1, lock_status));
            }
        }

        Ok(report)
    })?
}

/// Collect timezone fingerprint: detect if timezone was manually changed (tampering indicator).
#[tauri::command]
fn collect_timezone_fingerprint(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Timezone Fingerprint â•â•â•\n\n");

        // Current timezone
        if let Ok(out) = adb.shell(&serial, &["getprop", "persist.sys.timezone"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Current timezone: {}\n", v));
        }

        // Auto timezone setting
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "auto_time_zone"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            let auto = match v.as_str() {
                "1" => "YES (automatic)",
                "0" => "NO (manual â€” potential tampering indicator)",
                _ => &v,
            };
            report.push_str(&format!("  Auto timezone: {}\n", auto));
        }

        // Auto time (NTP)
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "auto_time"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            let auto = match v.as_str() {
                "1" => "YES (NTP sync)",
                "0" => "NO (manual time â€” suspicious)",
                _ => &v,
            };
            report.push_str(&format!("  Auto time: {}\n", auto));
        }

        // Device uptime vs elapsed realtime (mismatch = time was changed)
        report.push_str("\nâ”€â”€ Uptime Analysis â”€â”€\n");
        if let Ok(uptime) = adb.shell(&serial, &["cat", "/proc/uptime"]) {
            let v = String::from_utf8_lossy(&uptime).trim().to_string();
            let secs = v.split_whitespace().next().unwrap_or("?");
            if let Ok(s) = secs.parse::<f64>() {
                let hours = (s / 3600.0) as u64;
                let mins = ((s % 3600.0) / 60.0) as u64;
                report.push_str(&format!("  Uptime: {}s ({}h {}m)\n", secs, hours, mins));
            } else {
                report.push_str(&format!("  Uptime: {}s\n", secs));
            }
        }
        if let Ok(elapsed) = adb.shell(&serial, &["dumpsys", "alarm", "|", "grep", "Elapsed"]) {
            let v = String::from_utf8_lossy(&elapsed).trim().to_string();
            // Parse "Elapsed realtime: 12345ms" or similar
            let ms = v.split(|c: char| c == ':' || c == '=').nth(1)
                .map(|s| s.trim().trim_end_matches("ms").trim())
                .unwrap_or("?");
            if let Ok(m) = ms.parse::<u64>() {
                let secs = m / 1000;
                let hours = secs / 3600;
                let mins = (secs % 3600) / 60;
                report.push_str(&format!("  Elapsed realtime: {}ms ({}h {}m)\n", m, hours, mins));
            } else {
                report.push_str(&format!("  Elapsed realtime: {}\n", ms));
            }
        }

        // NITZ (Network Identity and Time Zone) â€” parsed
        report.push_str("\nâ”€â”€ NITZ (Network Time) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "telephony.registry", "|", "grep", "-i", "nitz"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_nitz = false;
            for line in text.lines().take(5) {
                let l = line.trim();
                if !l.is_empty() {
                    // Extract key NITZ fields
                    if l.contains("mTime") || l.contains("mZone") || l.contains("nitz") {
                        report.push_str(&format!("  {}\n", l));
                        found_nitz = true;
                    }
                }
            }
            if !found_nitz { report.push_str("  No NITZ data available\n"); }
        }

        // Locale
        if let Ok(out) = adb.shell(&serial, &["getprop", "persist.sys.locale"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() { report.push_str(&format!("\n  Locale: {}\n", v)); }
        }

        Ok(report)
    })?
}

/// Analyze device storage: parsed breakdown of what's using space.
#[tauri::command]
fn analyze_storage(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Storage Analysis â•â•â•\n\n");

        // Helper: parse `df` output into structured fields
        fn parse_df(text: &str) -> String {
            let mut out = String::new();
            for line in text.lines().skip(1) { // skip header
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    let filesystem = parts[0];
                    let total = parts[1];
                    let used = parts[2];
                    let free = parts[3];
                    let pct = parts[4];
                    let mount = parts[5];
                    out.push_str(&format!("  Mount: {}\n    Filesystem: {}\n    Total: {} | Used: {} | Free: {} | Use%: {}\n",
                        mount, filesystem, total, used, free, pct));
                }
            }
            if out.is_empty() { out = "  Could not parse df output\n".into(); }
            out
        }

        // /data partition
        report.push_str("â”€â”€ /data Partition â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["df", "/data"]) {
            report.push_str(&parse_df(&String::from_utf8_lossy(&out)));
        }

        // /sdcard partition
        report.push_str("\nâ”€â”€ /sdcard Partition â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["df", "/sdcard"]) {
            report.push_str(&parse_df(&String::from_utf8_lossy(&out)));
        }

        // Top space consumers on sdcard (parsed)
        report.push_str("\nâ”€â”€ Top Space Consumers (/sdcard) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["du", "-sh", "/sdcard/*", "2>/dev/null", "|", "sort", "-rh", "|", "head", "-20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let size = parts[0];
                    let path = parts[1..].join(" ");
                    let folder = path.rsplit('/').next().unwrap_or(&path);
                    report.push_str(&format!("  {:>8}  {}\n", size, folder));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No data accessible\n"); }
        }

        // Per-app storage from dumpsys diskstats (parsed)
        report.push_str("\nâ”€â”€ App Storage (Top Consumers) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "diskstats"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                // Lines like: "com.whatsapp: 150MB" or similar formats
                let l = line.trim();
                if l.contains("MB") || l.contains("GB") || l.contains("KB") {
                    let parts: Vec<&str> = l.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        report.push_str(&format!("  {}: {}\n", parts[0].trim(), parts[1].trim()));
                        count += 1;
                        if count >= 20 { break; }
                    }
                }
            }
            if count == 0 { report.push_str("  No app storage data available\n"); }
        }

        // Total video/photo count
        report.push_str("\nâ”€â”€ Media File Counts â”€â”€\n");
        for (label, ext) in &[("Videos", "*.mp4"), ("Photos (DCIM)", "*.jpg"), ("Audio", "*.mp3")] {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("find /sdcard -name '{}' 2>/dev/null | wc -l", ext)]) {
                let raw = String::from_utf8_lossy(&out);
                let n = raw.trim();
                report.push_str(&format!("  {}: {} files\n", label, n));
            }
        }

        Ok(report)
    })?
}

/// Recover deleted voice memos/recordings from the device.
#[tauri::command]
fn recover_voice_memos(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Voice Memo Recovery â•â•â•\n\n");

        // Common recording paths
        let recording_paths = [
            "/sdcard/Recordings",
            "/sdcard/Record",
            "/sdcard/VoiceRecorder",
            "/sdcard/Music/Recordings",
            "/sdcard/Android/data/com.sec.android.app.voicenote/files",
            "/sdcard/Android/data/com.google.android.apps.recorder/files",
        ];

        let mut total_found = 0;
        for path in &recording_paths {
            if let Ok(out) = adb.shell(&serial, &["ls", "-la", path]) {
                let text = String::from_utf8_lossy(&out);
                if !text.contains("No such file") && !text.trim().is_empty() {
                    report.push_str(&format!("â”€â”€ {} â”€â”€\n", path));
                    let mut file_count = 0;
                    for line in text.lines() {
                        let l = line.trim();
                        // Skip "total N" lines and directory entries
                        if l.starts_with("total") || l.starts_with("d") { continue; }
                        // Parse: -rw-rw---- 1 user group 12345 2024-01-01 12:00 file.m4a
                        let parts: Vec<&str> = l.split_whitespace().collect();
                        if parts.len() >= 6 {
                            let size = parts.get(parts.len() - 3).unwrap_or(&"?");
                            let date = parts.get(parts.len() - 2).unwrap_or(&"?");
                            let time = parts.get(parts.len() - 1).unwrap_or(&"?");
                            // The filename is the last meaningful part
                            // Actually in `ls -la`, the format is: perms links user group size date time name
                            // But size/date/time positions vary. Let's just extract the name from the path.
                            let name = l.rsplitn(2, ' ').last().unwrap_or(l);
                            let size_str = if size.parse::<u64>().is_ok() {
                                let s = size.parse::<u64>().unwrap_or(0);
                                if s > 1_000_000 { format!("{:.1} MB", s as f64 / 1_000_000.0) }
                                else if s > 1000 { format!("{:.1} KB", s as f64 / 1000.0) }
                                else { format!("{} B", s) }
                            } else { "?".into() };
                            report.push_str(&format!("  {} | {} | {} {}\n", name, size_str, date, time));
                            file_count += 1;
                        }
                    }
                    report.push_str(&format!("  ({} files)\n\n", file_count));
                    total_found += file_count;
                }
            }
        }

        // Also check for audio files in Downloads
        report.push_str("â”€â”€ Audio Files in /sdcard/Download â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["find", "/sdcard/Download", "-name", "*.m4a", "-o", "-name", "*.mp3", "-o", "-name", "*.aac", "-o", "-name", "*.ogg", "-o", "-name", "*.wav", "-o", "-name", "*.amr", "2>/dev/null"]) {
            let text = String::from_utf8_lossy(&out);
            let mut dl_count = 0;
            for line in text.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    report.push_str(&format!("  {}\n", name));
                    dl_count += 1;
                }
            }
            if dl_count == 0 { report.push_str("  No audio files found\n"); }
            total_found += dl_count;
        }

        report.push_str(&format!("\nTotal files found: {}\n", total_found));
        report.push_str(&format!("Output directory: {}\n", output_dir));

        Ok(report)
    })?
}

/// Extract saved passwords from Chrome browser (requires root or debuggable).
#[tauri::command]
fn extract_saved_passwords(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Saved Passwords â•â•â•\n\n");
        report.push_str("Note: Chrome passwords are encrypted â€” this shows metadata only\n\n");

        // Check if Chrome Login Data exists
        report.push_str("â”€â”€ Chrome Browser â”€â”€\n");
        let chrome_db = "/data/data/com.android.chrome/app_chrome/Default/Login Data";
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}'", chrome_db)]) {
            let text = String::from_utf8_lossy(&out);
            if text.contains("Login Data") {
                // Parse file size from ls output
                let parts: Vec<&str> = text.lines().next().unwrap_or("").split_whitespace().collect();
                let size = parts.get(4).unwrap_or(&"?");
                report.push_str(&format!("  Login Data DB: Found ({} bytes)\n", size));
                report.push_str("  Status: Passwords are encrypted (requires DB key to decrypt)\n");
            } else {
                report.push_str("  Login Data: Not accessible (requires root)\n");
            }
        } else {
            report.push_str("  Login Data: Not accessible\n");
        }

        // Samsung Internet
        report.push_str("\nâ”€â”€ Samsung Internet â”€â”€\n");
        let samsung_db = "/data/data/com.sec.android.app.sbrowser/databases/LoginData.db";
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}'", samsung_db)]) {
            let text = String::from_utf8_lossy(&out);
            if text.contains("LoginData") {
                let parts: Vec<&str> = text.lines().next().unwrap_or("").split_whitespace().collect();
                let size = parts.get(4).unwrap_or(&"?");
                report.push_str(&format!("  LoginData DB: Found ({} bytes)\n", size));
            } else {
                report.push_str("  Not installed or not accessible\n");
            }
        } else {
            report.push_str("  Not installed\n");
        }

        // Autofill service â€” parse for meaningful info
        report.push_str("\nâ”€â”€ Autofill Service â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "autofill"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_info = false;
            for line in text.lines() {
                let l = line.trim();
                // Extract meaningful autofill info
                if l.contains("Current service") || l.contains("serviceId") || l.contains("filledIds") {
                    report.push_str(&format!("  {}\n", l));
                    found_info = true;
                }
                if l.starts_with("mService") || l.contains("compatibility") {
                    report.push_str(&format!("  {}\n", l));
                    found_info = true;
                }
            }
            if !found_info { report.push_str("  No autofill service data accessible\n"); }
        }

        // Check for saved passwords in settings
        report.push_str("\nâ”€â”€ Password Manager Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "autofill_service"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Autofill service: {}\n", v));
            } else {
                report.push_str("  No autofill service configured\n");
            }
        }

        report.push_str("\nNote: To decrypt saved passwords, pull the Login Data DB and use Chrome's encryption key.\n");

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// GPS / LOCATION TIMELINE
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Collect GPS/location history from the device.
/// Queries multiple sources and parses raw output into clean forensic report.
#[tauri::command]
fn collect_gps_timeline(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• GPS / Location Timeline â•â•â•\n\n");

        // 1. Last known location â€” parsed from dumpsys location
        report.push_str("â”€â”€ Last Known Location â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "location"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_loc = false;
            for line in text.lines() {
                // Look for Location[ lines like: Location[gps 28.6139,77.2090 acc=20 t=...]
                if line.contains("Location[") {
                    // Parse: Location[provider lat,lon acc=... t=...]
                    if let Some(start) = line.find("Location[") {
                        let rest = &line[start + 9..];
                        if let Some(end) = rest.find(']') {
                            let loc_data = &rest[..end];
                            let parts: Vec<&str> = loc_data.split_whitespace().collect();
                            if parts.len() >= 2 {
                                let provider = parts[0];
                                let coords = parts[1];
                                let accuracy = parts.iter().find(|p| p.starts_with("acc=")).map(|p| &p[4..]).unwrap_or("?");
                                let time = parts.iter().find(|p| p.starts_with("t=")).map(|p| &p[2..]).unwrap_or("?");
                                report.push_str(&format!("  Provider:   {}\n", provider));
                                report.push_str(&format!("  Coords:     {}\n", coords));
                                report.push_str(&format!("  Accuracy:   {} m\n", accuracy));
                                report.push_str(&format!("  Timestamp:  {}\n", time));
                                found_loc = true;
                            }
                        }
                    }
                }
            }
            if !found_loc { report.push_str("  No last known location available\n"); }

            // Provider status
            report.push_str("\nâ”€â”€ Location Providers â”€â”€\n");
            for line in text.lines() {
                let l = line.trim();
                if (l.contains("Provider") || l.contains("provider")) && (l.contains("gps") || l.contains("network") || l.contains("fused") || l.contains("passive")) {
                    // Clean up provider info
                    if l.contains("isAvailable") || l.contains("enabled") || l.contains("Status") {
                        report.push_str(&format!("  {}\n", l));
                    }
                }
            }
        }

        // 2. Location settings
        report.push_str("\nâ”€â”€ Location Settings â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "location_providers_allowed"]) {
            report.push_str(&format!("  Providers: {}\n", String::from_utf8_lossy(&out).trim()));
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "location_mode"]) {
            let raw = String::from_utf8_lossy(&out);
            let trimmed = raw.trim();
            let mode = match trimmed {
                "0" => "OFF",
                "1" => "GPS Only",
                "2" => "Battery Saving (Network)",
                "3" => "High Accuracy",
                other => other,
            };
            report.push_str(&format!("  Mode: {}\n", mode));
        }

        // 3. GPS satellite status â€” parsed from dumpsys gps
        report.push_str("\nâ”€â”€ GPS Satellite Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "gps"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_status = false;
            for line in text.lines() {
                let l = line.trim();
                // Extract key GPS status lines
                if l.contains("svs=") || l.contains("fixes=") || l.contains("ttff=") || l.contains("last_fix=") {
                    report.push_str(&format!("  {}\n", l));
                    found_status = true;
                }
                if l.contains("Status") || l.contains("started") || l.contains("stopped") {
                    report.push_str(&format!("  {}\n", l));
                    found_status = true;
                }
            }
            if !found_status { report.push_str("  No GPS status data available\n"); }
        }

        // 4. Recent location requests â€” parsed from dumpsys location
        report.push_str("\nâ”€â”€ Recent Location Requests â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "location"]) {
            let text = String::from_utf8_lossy(&out);
            let mut req_count = 0;
            for line in text.lines() {
                let l = line.trim();
                // Parse request records: "Request[...]" or "Record:" lines
                if l.contains("Request[") || (l.contains("Record") && l.contains("package=")) {
                    // Extract package name
                    if let Some(pkg_start) = l.find("package=") {
                        let rest = &l[pkg_start + 8..];
                        let pkg = rest.split(|c: char| c == ' ' || c == ']' || c == ',').next().unwrap_or("?");
                        let interval = l.split("interval=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("?");
                        report.push_str(&format!("  {} (interval: {})\n", pkg, interval));
                        req_count += 1;
                        if req_count >= 20 {
                            report.push_str("  ... (truncated)\n");
                            break;
                        }
                    }
                }
            }
            if req_count == 0 { report.push_str("  No recent location requests found\n"); }
        }

        // 5. Google Maps cache
        report.push_str("\nâ”€â”€ Google Maps Cache â”€â”€\n");
        let maps_cache = "/data/data/com.google.android.apps.maps/files/maps_debug";
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", maps_cache)]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                // Parse ls output for file details
                for line in text.lines() {
                    if !line.starts_with("total") && !line.trim().is_empty() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 6 {
                            let name = parts.last().unwrap_or(&"?");
                            let size = parts.get(4).unwrap_or(&"?");
                            report.push_str(&format!("  {} ({} bytes)\n", name, size));
                        }
                    }
                }
            } else { report.push_str("  Maps cache not accessible (requires root)\n"); }
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// WHATSAPP CHAT VIEWER
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Extract and display WhatsApp chat messages.
/// Pulls the msgstore.db and wa.db from the device, parses messages,
/// and returns a formatted chat log.
#[tauri::command]
fn extract_whatsapp_chats(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• WhatsApp Chat Extract â•â•â•\n\n");

        // Try to pull msgstore.db (may need root)
        let db_paths = [
            "/data/data/com.whatsapp/databases/msgstore.db",
            "/sdcard/WhatsApp/Databases/msgstore.db",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db",
        ];

        let local_db = format!("{}\\msgstore.db", output_dir);
        let mut found_db = false;

        for path in &db_paths {
            let result = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}'", path)]);
            if let Ok(out) = result {
                let text = String::from_utf8_lossy(&out);
                if text.contains("msgstore.db") {
                    report.push_str(&format!("Found: {}\n", path));
                    // Try to pull
                    let pull_result = std::process::Command::new("adb")
                        .args(["-s", &serial, "pull", path, &local_db])
                        .output();
                    if let Ok(output) = pull_result {
                        if output.status.success() && std::path::Path::new(&local_db).exists() {
                            found_db = true;
                            report.push_str(&format!("Pulled to: {}\n", local_db));
                            break;
                        }
                    }
                }
            }
        }

        // Also check for encrypted backup
        let crypt_paths = [
            "/sdcard/WhatsApp/Databases/msgstore.db.crypt14",
            "/sdcard/WhatsApp/Databases/msgstore.db.crypt15",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/msgstore.db.crypt14",
        ];
        for path in &crypt_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}'", path)]) {
                let text = String::from_utf8_lossy(&out);
                if text.contains("crypt") {
                    report.push_str(&format!("\nEncrypted backup found: {}\n", path));
                    report.push_str("Use WhatsApp Key Database Extractor to decrypt.\n");
                }
            }
        }

        if found_db {
            // Parse the database
            match rusqlite::Connection::open_with_flags(&local_db, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE) {
                Ok(conn) => {
                    // Try to read messages
                    let sql = "SELECT m.key_remote_jid, m.data, m.timestamp, m.from_me, \
                               COALESCE(c.subject, c.raw_contact_id) as contact_name \
                               FROM message m \
                               LEFT JOIN chat c ON m.key_remote_jid = c.jid \
                               WHERE m.data IS NOT NULL AND m.data != '' \
                               ORDER BY m.timestamp DESC LIMIT 200";

                    match conn.prepare(sql) {
                        Ok(mut stmt) => {
                            let rows = stmt.query_map([], |row| {
                                Ok((
                                    row.get::<_, String>(0)?,
                                    row.get::<_, String>(1)?,
                                    row.get::<_, i64>(2)?,
                                    row.get::<_, bool>(3)?,
                                    row.get::<_, Option<String>>(4)?,
                                ))
                            });
                            let mut count = 0;
                            if let Ok(rows) = rows {
                                for row in rows.flatten() {
                                    let (jid, data, ts, from_me, name) = row;
                                    let time = humantime_format((ts / 1000) as u64);
                                    let direction = if from_me { "â†’" } else { "â†" };
                                    let display_name = name.unwrap_or_else(|| jid.split('@').next().unwrap_or(&jid).to_string());
                                    report.push_str(&format!("[{}] {} {}: {}\n", time, direction, display_name, data));
                                    count += 1;
                                }
                            }
                            report.push_str(&format!("\n{} messages extracted\n", count));
                        }
                        Err(e) => {
                            report.push_str(&format!("\nDB query error (schema may differ): {}\n", e));
                            report.push_str("Try pulling the DB and analyzing with DB browser.\n");
                        }
                    }
                }
                Err(e) => {
                    report.push_str(&format!("\nCannot open DB: {}\n", e));
                    report.push_str("The database may be encrypted. Use the WhatsApp Key Extractor tool.\n");
                }
            }
        } else {
            report.push_str("\nCould not access WhatsApp database.\n");
            report.push_str("Options:\n");
            report.push_str("1. Root access required for /data/data/ path\n");
            report.push_str("2. Check if WhatsApp backup exists on SD card\n");
            report.push_str("3. Use 'WhatsApp Key Database Extractor' tool for encrypted DBs\n");
        }

        // Also pull wa.db for contact list
        report.push_str("\nâ”€â”€ WhatsApp Contact Summary â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "package", "com.whatsapp", "|", "grep", "-i", "versionName"]) {
            report.push_str(&format!("WA Version: {}\n", String::from_utf8_lossy(&out).trim()));
        }
        if let Ok(out) = adb.shell(&serial, &["stat", "-c", "%s", "/sdcard/WhatsApp/Databases/msgstore.db"]) {
            report.push_str(&format!("msgstore.db size: {} bytes\n", String::from_utf8_lossy(&out).trim()));
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// EMAIL EXTRACTION (Gmail / Outlook)
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Extract email account metadata and cached data from Gmail and Outlook.
#[tauri::command]
fn extract_email_accounts(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Email Account Extraction â•â•â•\n\n");

        // Parse all accounts from dumpsys account â€” extract just account names and types
        report.push_str("â”€â”€ Registered Accounts â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "account"]) {
            let text = String::from_utf8_lossy(&out);
            let mut account_count = 0;
            let mut gmail_accounts: Vec<String> = Vec::new();
            let mut outlook_accounts: Vec<String> = Vec::new();
            let mut other_accounts: Vec<String> = Vec::new();

            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("Account {") {
                    // Parse: Account {name=email@domain type=com.google}
                    let name = l.split("name=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == '}').next())
                        .unwrap_or("?");
                    let acct_type = l.split("type=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == '}').next())
                        .unwrap_or("?");

                    let entry = format!("{} ({})", name, acct_type);
                    if acct_type.contains("com.google") {
                        gmail_accounts.push(entry);
                    } else if acct_type.contains("com.microsoft") {
                        outlook_accounts.push(entry);
                    } else {
                        other_accounts.push(entry);
                    }
                    account_count += 1;
                }
            }

            if !gmail_accounts.is_empty() {
                report.push_str("  Gmail:\n");
                for a in &gmail_accounts { report.push_str(&format!("    {}\n", a)); }
            }
            if !outlook_accounts.is_empty() {
                report.push_str("  Outlook:\n");
                for a in &outlook_accounts { report.push_str(&format!("    {}\n", a)); }
            }
            if !other_accounts.is_empty() {
                report.push_str("  Other:\n");
                for a in other_accounts.iter().take(10) { report.push_str(&format!("    {}\n", a)); }
                if other_accounts.len() > 10 { report.push_str(&format!("    ... and {} more\n", other_accounts.len() - 10)); }
            }
            report.push_str(&format!("  Total accounts: {}\n", account_count));
        }

        // Gmail DB info â€” parsed
        report.push_str("\nâ”€â”€ Gmail Database Info â”€â”€\n");
        let gmail_paths = [
            "/data/data/com.google.android.gm/databases/mailstore.db",
            "/data/data/com.google.android.gm/databases/mailstore.1.db",
        ];
        let mut found_gmail_db = false;
        for path in &gmail_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() {
                    let parts: Vec<&str> = text.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let size = parts[4];
                        report.push_str(&format!("  {} ({} bytes)\n", path.rsplit('/').next().unwrap_or(path), size));
                        found_gmail_db = true;
                    }
                }
            }
        }
        if !found_gmail_db { report.push_str("  Gmail DB not accessible (requires root)\n"); }

        // Outlook accounts â€” parsed
        report.push_str("\nâ”€â”€ Outlook / Hotmail â”€â”€\n");
        let outlook_paths = [
            "/data/data/com.microsoft.office.outlook/databases/",
            "/data/data/com.microsoft.office.outlook/files/",
        ];
        let mut found_outlook = false;
        for path in &outlook_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() {
                    report.push_str(&format!("  {}:\n", path.rsplit('/').nth(1).unwrap_or(path)));
                    for line in text.lines() {
                        if !line.starts_with("total") && !line.trim().is_empty() {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            if parts.len() >= 5 {
                                let name = parts.last().unwrap_or(&"?");
                                let size = parts[4];
                                report.push_str(&format!("    {} ({} bytes)\n", name, size));
                            }
                        }
                    }
                    found_outlook = true;
                }
            }
        }
        if !found_outlook { report.push_str("  Outlook not installed or not accessible\n"); }

        // Email sync status â€” parsed
        report.push_str("\nâ”€â”€ Email Sync Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "content", "|", "grep", "-i", "gmail"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_sync = false;
            for line in text.lines().take(10) {
                let l = line.trim();
                if l.contains("authority") || l.contains("syncable") || l.contains("sync") {
                    report.push_str(&format!("  {}\n", l));
                    found_sync = true;
                }
            }
            if !found_sync { report.push_str("  No Gmail sync data accessible\n"); }
        } else {
            report.push_str("  Could not query sync status\n");
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// BROWSER HISTORY VIEWER
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Extract browser history, bookmarks, and open tabs from Chrome and other browsers.
#[tauri::command]
fn extract_browser_history(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Browser History Extraction â•â•â•\n\n");

        // Chrome History via content provider (no root needed for some data)
        report.push_str("â”€â”€ Chrome Browser Data â”€â”€\n");

        // Try to pull Chrome History DB
        let chrome_history = "/data/data/com.android.chrome/app_chrome/Default/History";
        let local_history = "D:\\recovery-tool\\workspace\\chrome_history.db";

        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}'", chrome_history)]) {
            let text = String::from_utf8_lossy(&out);
            if text.contains("History") {
                report.push_str("  Chrome History DB: Found\n");
                let pull = std::process::Command::new("adb")
                    .args(["-s", &serial, "pull", chrome_history, local_history])
                    .output();
                if let Ok(output) = pull {
                    if output.status.success() && std::path::Path::new(local_history).exists() {
                        if let Ok(conn) = rusqlite::Connection::open_with_flags(local_history, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE) {
                            let sql = "SELECT url, title, visit_count, last_visit_time FROM urls ORDER BY last_visit_time DESC LIMIT 100";
                            if let Ok(mut stmt) = conn.prepare(sql) {
                                let rows = stmt.query_map([], |row| {
                                    Ok((
                                        row.get::<_, String>(0)?,
                                        row.get::<_, Option<String>>(1)?,
                                        row.get::<_, i32>(2)?,
                                        row.get::<_, i64>(3)?,
                                    ))
                                });
                                if let Ok(rows) = rows {
                                    let mut count = 0;
                                    for row in rows.flatten() {
                                        let (url, title, visits, _last_visit) = row;
                                        let t = title.unwrap_or_default();
                                        let domain = url.split('/').nth(2).unwrap_or(&url);
                                        report.push_str(&format!("  [{}x] {} â€” {}\n", visits, domain, if t.is_empty() { &url } else { &t }));
                                        count += 1;
                                    }
                                    report.push_str(&format!("\n  {} URLs extracted\n", count));
                                }
                            }
                        }
                    } else {
                        report.push_str("  Could not pull History DB (requires root)\n");
                    }
                }
            } else {
                report.push_str("  Chrome History DB: Not found\n");
            }
        } else {
            report.push_str("  Chrome: Not accessible\n");
        }

        // Samsung Internet â€” parsed
        report.push_str("\nâ”€â”€ Samsung Internet â”€â”€\n");
        let samsung_history = "/data/data/com.sec.android.app.sbrowser/databases/browser.db";
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", samsung_history)]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                let parts: Vec<&str> = text.split_whitespace().collect();
                if parts.len() >= 5 {
                    report.push_str(&format!("  browser.db: {} bytes (requires root to pull)\n", parts[4]));
                }
            } else {
                report.push_str("  Not installed\n");
            }
        } else {
            report.push_str("  Not installed\n");
        }

        // Installed browsers
        report.push_str("\nâ”€â”€ Installed Browsers â”€â”€\n");
        let browsers = [
            ("com.android.chrome", "Chrome"),
            ("com.sec.android.app.sbrowser", "Samsung Internet"),
            ("org.mozilla.firefox", "Firefox"),
            ("com.opera.browser", "Opera"),
            ("com.brave.browser", "Brave"),
            ("com.microsoft.emmx", "Edge"),
        ];
        let mut found_any = false;
        for (pkg, name) in &browsers {
            if let Ok(out) = adb.shell(&serial, &["pm", "path", pkg]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if text.contains("base.apk") {
                    // Try to get version
                    let version = if let Ok(vout) = adb.shell(&serial, &["dumpsys", "package", pkg, "|", "grep", "versionName"]) {
                        String::from_utf8_lossy(&vout).trim()
                            .split("versionName=").nth(1)
                            .unwrap_or("?")
                            .to_string()
                    } else { "?".into() };
                    report.push_str(&format!("  {} {} (v{})\n", "âœ“", name, version));
                    found_any = true;
                }
            }
        }
        if !found_any { report.push_str("  No browsers detected\n"); }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// VIDEO METADATA EXTRACTION
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Extract metadata from video files on the device.
#[tauri::command]
fn extract_video_metadata(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Video Metadata Extraction â•â•â•\n\n");

        // Query MediaStore for video files â€” parsed
        report.push_str("â”€â”€ Video Files (MediaStore) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri", "content://media/external/video/media",
            "--projection", "_id:_data:display_name:size:date_added:date_modified:duration:width:height:mime_type"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Row") {
                    // Parse: Row: 0 _id=123, _data=/sdcard/..., display_name=VID_..., size=12345, ...
                    let name = l.split("display_name=").nth(1)
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("?");
                    let size = l.split("size=").nth(1)
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("?");
                    let duration = l.split("duration=").nth(1)
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("?");
                    let width = l.split("width=").nth(1)
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("?");
                    let height = l.split("height=").nth(1)
                        .and_then(|s| s.split(',').next())
                        .unwrap_or("?");

                    let size_str = if let Ok(s) = size.parse::<u64>() {
                        if s > 1_000_000 { format!("{:.1} MB", s as f64 / 1_000_000.0) }
                        else if s > 1000 { format!("{:.1} KB", s as f64 / 1000.0) }
                        else { format!("{} B", s) }
                    } else { size.to_string() };

                    let dur_str = if let Ok(d) = duration.parse::<u64>() {
                        let secs = d / 1000;
                        format!("{}:{:02}", secs / 60, secs % 60)
                    } else { duration.to_string() };

                    report.push_str(&format!("  {} | {} | {} | {}x{}\n", name, size_str, dur_str, width, height));
                    count += 1;
                    if count >= 30 { break; }
                }
            }
            if count == 0 { report.push_str("  No videos in MediaStore\n"); }
            else { report.push_str(&format!("  ({} videos)\n", count)); }
        }

        // DCIM camera videos â€” parsed
        report.push_str("\nâ”€â”€ DCIM Camera Videos â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard/DCIM -name '*.mp4' -o -name '*.3gp' -o -name '*.mkv' -o -name '*.avi' 2>/dev/null | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            let mut vid_count = 0;
            for line in text.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    // Get file size
                    let size = if let Ok(stat) = adb.shell(&serial, &["stat", "-c", "%s", path]) {
                        let s = String::from_utf8_lossy(&stat).trim().parse::<u64>().unwrap_or(0);
                        if s > 1_000_000 { format!("{:.1} MB", s as f64 / 1_000_000.0) }
                        else if s > 1000 { format!("{:.1} KB", s as f64 / 1000.0) }
                        else { format!("{} B", s) }
                    } else { "?".into() };
                    report.push_str(&format!("  {} ({})\n", name, size));
                    vid_count += 1;
                }
            }
            if vid_count == 0 { report.push_str("  No DCIM videos found\n"); }
        }

        // Total video storage
        report.push_str("\nâ”€â”€ Video Storage Summary â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -name '*.mp4' -o -name '*.3gp' -o -name '*.mkv' 2>/dev/null | wc -l"]) {
            report.push_str(&format!("  Total video files: {}\n", String::from_utf8_lossy(&out).trim()));
        }
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard \\( -name '*.mp4' -o -name '*.3gp' -o -name '*.mkv' \\) -exec du -ch {} + 2>/dev/null | tail -1"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                report.push_str(&format!("  Total size: {}\n", text));
            }
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// APP TIMELINE CORRELATION
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Build a correlated timeline of app usage events across all installed apps.
#[tauri::command]
fn extract_app_timeline(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• App Timeline Correlation â•â•â•\n\n");

        // Usage stats â€” parsed from dumpsys usagestats
        report.push_str("â”€â”€ App Usage Statistics (Top 30) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "usagestats"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("package=") {
                    let pkg = l.split("package=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == ',').next())
                        .unwrap_or("?");
                    let fg_time = l.split("totalTimeInForeground=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == ',').next())
                        .unwrap_or("?");
                    let last_used = l.split("lastTimeUsed=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == ',').next())
                        .unwrap_or("?");

                    // Convert ms to human readable
                    let fg_str = if let Ok(ms) = fg_time.parse::<u64>() {
                        if ms > 3600000 { format!("{:.1} hrs", ms as f64 / 3600000.0) }
                        else if ms > 60000 { format!("{} min", ms / 60000) }
                        else { format!("{} sec", ms / 1000) }
                    } else { fg_time.to_string() };

                    report.push_str(&format!("  {} | foreground: {} | last: {}\n", pkg, fg_str, last_used));
                    count += 1;
                    if count >= 30 { break; }
                }
            }
            if count == 0 { report.push_str("  No usage statistics available\n"); }
        }

        // App install timeline â€” parsed
        report.push_str("\nâ”€â”€ App Install Timeline (First 30) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["pm", "list", "packages", "-f", "-i", "-U"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            let total = text.lines().count();
            for line in text.lines() {
                // Format: package:/path/to/base.apk=com.package.name installer=... uid=...
                if let Some(pkg_start) = line.find('=') {
                    let rest = &line[pkg_start + 1..];
                    let pkg = rest.split(' ').next().unwrap_or(rest);
                    let installer = line.split("installer=").nth(1)
                        .and_then(|s| s.split(' ').next())
                        .unwrap_or("unknown");
                    let installer_name = match installer {
                        "com.android.vending" => "Play Store",
                        "com.samsung.android.app.galaxystore" => "Galaxy Store",
                        "null" | "unknown" => "sideloaded",
                        other => other,
                    };
                    report.push_str(&format!("  {} (via {})\n", pkg, installer_name));
                    count += 1;
                    if count >= 30 {
                        if total > 30 { report.push_str(&format!("  ... and {} more\n", total - 30)); }
                        break;
                    }
                }
            }
        }

        // Running services â€” parsed
        report.push_str("\nâ”€â”€ Currently Running Services â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "activity", "services", "|", "grep", "ServiceRecord"]) {
            let text = String::from_utf8_lossy(&out);
            let mut svc_count = 0;
            for line in text.lines() {
                let l = line.trim();
                // Extract service package from ServiceRecord line
                if let Some(pkg_start) = l.find("app=AppRecord{") {
                    let rest = &l[pkg_start + 14..];
                    let pkg = rest.split_whitespace().next().unwrap_or("?");
                    report.push_str(&format!("  {}\n", pkg));
                    svc_count += 1;
                    if svc_count >= 20 { break; }
                } else if l.contains("ServiceRecord") {
                    // Alternative format
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    if parts.len() >= 3 {
                        report.push_str(&format!("  {}\n", parts[2]));
                        svc_count += 1;
                        if svc_count >= 20 { break; }
                    }
                }
            }
            if svc_count == 0 { report.push_str("  No running services found\n"); }
        }

        // Background restrictions
        report.push_str("\nâ”€â”€ Background Restrictions â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "background_restrictions"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            let status = match v.as_str() {
                "0" => "Disabled (no restrictions)",
                "1" => "Enabled",
                other => other,
            };
            report.push_str(&format!("  Status: {}\n", status));
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// CLOUD ACCOUNT EXTRACTION
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Extract cloud account information: Google, Samsung, Xiaomi, Huawei, etc.
#[tauri::command]
fn extract_cloud_accounts(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Cloud Account Extraction â•â•â•\n\n");

        // Parse all accounts from dumpsys account â€” extract names and types
        report.push_str("â”€â”€ Registered Accounts â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "account"]) {
            let text = String::from_utf8_lossy(&out);
            let mut accounts: Vec<(String, String)> = Vec::new(); // (name, type)

            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("Account {") {
                    let name = l.split("name=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == '}').next())
                        .unwrap_or("?")
                        .to_string();
                    let acct_type = l.split("type=").nth(1)
                        .and_then(|s| s.split(|c: char| c == ' ' || c == '}').next())
                        .unwrap_or("?")
                        .to_string();
                    accounts.push((name, acct_type));
                }
            }

            // Group by type
            let mut google: Vec<&str> = Vec::new();
            let mut samsung: Vec<&str> = Vec::new();
            let mut xiaomi: Vec<&str> = Vec::new();
            let mut other: Vec<(&str, &str)> = Vec::new();

            for (name, acct_type) in &accounts {
                if acct_type.contains("com.google") {
                    google.push(name);
                } else if acct_type.contains("com.osp.app.signin") || acct_type.contains("samsung") {
                    samsung.push(name);
                } else if acct_type.contains("xiaomi") || acct_type.contains("com.mi") {
                    xiaomi.push(name);
                } else {
                    other.push((name, acct_type));
                }
            }

            if !google.is_empty() {
                report.push_str("  Google:\n");
                for a in &google { report.push_str(&format!("    {}\n", a)); }
            }
            if !samsung.is_empty() {
                report.push_str("  Samsung:\n");
                for a in &samsung { report.push_str(&format!("    {}\n", a)); }
            }
            if !xiaomi.is_empty() {
                report.push_str("  Xiaomi:\n");
                for a in &xiaomi { report.push_str(&format!("    {}\n", a)); }
            }
            if !other.is_empty() {
                report.push_str("  Other:\n");
                for (name, atype) in other.iter().take(10) {
                    report.push_str(&format!("    {} ({})\n", name, atype));
                }
            }
            report.push_str(&format!("  Total: {} accounts\n", accounts.len()));
        }

        // Cloud backup settings â€” parsed
        report.push_str("\nâ”€â”€ Cloud Backup Settings â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "backup_enabled"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Google Backup: {}\n", match v.as_str() {
                "1" => "Enabled",
                "0" => "Disabled",
                other => other,
            }));
        }
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "backup_transport"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Backup Transport: {}\n", v));
            }
        }

        // Auto-sync status â€” parsed
        report.push_str("\nâ”€â”€ Auto-Sync Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "sync_enabled"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Global sync: {}\n", match v.as_str() {
                "1" => "Enabled",
                "0" => "Disabled",
                other => other,
            }));
        }
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "syncmanager", "|", "grep", "-E", "connectivity|sync"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines().take(10) {
                let l = line.trim();
                if l.contains("connectivity") || l.contains("sync") {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No sync manager data accessible\n"); }
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// ENCRYPTED BACKUP ANALYSIS
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Analyze encrypted backups found on the device (WhatsApp, Signal, etc.)
#[tauri::command]
fn analyze_encrypted_backups(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Encrypted Backup Analysis â•â•â•\n\n");

        // WhatsApp encrypted backups â€” parsed
        report.push_str("â”€â”€ WhatsApp Backups â”€â”€\n");
        let wa_backup_paths = [
            "/sdcard/WhatsApp/Databases/",
            "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases/",
        ];
        let mut found_wa = false;
        for path in &wa_backup_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("crypt") {
                        // Parse: -rw-rw---- 1 user group 12345 2024-01-01 12:00 msgstore.db.crypt14
                        let parts: Vec<&str> = l.split_whitespace().collect();
                        if parts.len() >= 6 {
                            let name = parts.last().unwrap_or(&"?");
                            let size = parts.get(4).unwrap_or(&"?");
                            let date = parts.get(5).unwrap_or(&"?");
                            let time = parts.get(6).unwrap_or(&"?");
                            let enc_type = if name.contains("crypt14") { "AES-256 (crypt14)" }
                                else if name.contains("crypt15") { "AES-256 (crypt15)" }
                                else { "encrypted" };
                            report.push_str(&format!("  {} | {} | {} {} | {}\n", name, size, date, time, enc_type));
                            found_wa = true;
                        }
                    }
                }
            }
        }
        if !found_wa { report.push_str("  No WhatsApp encrypted backups found\n"); }

        // WhatsApp key files â€” parsed
        report.push_str("\nâ”€â”€ WhatsApp Key Files â”€â”€\n");
        let key_paths = [
            "/data/data/com.whatsapp/files/key",
            "/sdcard/WhatsApp/.k",
            "/data/data/com.whatsapp/files/key.key",
        ];
        let mut found_key = false;
        for path in &key_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() && !text.contains("No such") {
                    let parts: Vec<&str> = text.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let size = parts[4];
                        report.push_str(&format!("  Found: {} ({} bytes)\n", path, size));
                        found_key = true;
                    }
                }
            }
        }
        if !found_key { report.push_str("  No key files found (requires root for /data/data/)\n"); }

        // Signal backups â€” parsed
        report.push_str("\nâ”€â”€ Signal Backups â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -path '*signal*' -name '*.backup' 2>/dev/null | head -10"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                for line in text.lines() {
                    let name = line.rsplit('/').next().unwrap_or(line);
                    report.push_str(&format!("  {}\n", name));
                }
            } else {
                report.push_str("  No Signal backups found on SD card\n");
            }
        }

        // Telegram backups â€” parsed
        report.push_str("\nâ”€â”€ Telegram Backups â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -path '*Telegram*' -name '*.tg' 2>/dev/null | head -10"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                for line in text.lines() {
                    let name = line.rsplit('/').next().unwrap_or(line);
                    report.push_str(&format!("  {}\n", name));
                }
            } else {
                report.push_str("  No Telegram backups found on SD card\n");
            }
        }

        // Android full backup â€” parsed
        report.push_str("\nâ”€â”€ Android Full Backup â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -name '*.ab' 2>/dev/null | head -10"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                for line in text.lines() {
                    let name = line.rsplit('/').next().unwrap_or(line);
                    report.push_str(&format!("  {}\n", name));
                }
            } else {
                report.push_str("  No Android backup files found\n");
            }
        }

        // Encryption status â€” parsed
        report.push_str("\nâ”€â”€ Device Encryption â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.state"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  State: {}\n", if v.is_empty() { "unknown" } else { &v }));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.type"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Type: {}\n", if v.is_empty() { "unknown" } else { &v }));
        }

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// AUTOMATED PDF REPORT (text-based for now)
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Generate a comprehensive forensic report in text format.
/// Collects data from all available sources and compiles into a single document.
#[tauri::command]
fn generate_pdf_report(serial: String, output_dir: String, case_id: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default().as_secs();

        let report_path = format!("{}\\forensic_report_{}_{}.txt", output_dir, case_id, timestamp);

        let mut report = String::new();
        report.push_str("â•”â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•—\n");
        report.push_str("â•‘          RECOVERYKIT FORENSIC REPORT                        â•‘\n");
        report.push_str("â•šâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        report.push_str(&format!("Case ID: {}\n", case_id));
        report.push_str(&format!("Generated: {}\n", humantime_format(timestamp)));
        report.push_str(&format!("Device Serial: {}\n\n", serial));

        // Device Info
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  DEVICE INFORMATION\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        let props: &[(&str, &[&str])] = &[
            ("Brand", &["getprop", "ro.product.brand"]),
            ("Model", &["getprop", "ro.product.model"]),
            ("Android Version", &["getprop", "ro.build.version.release"]),
            ("SDK Level", &["getprop", "ro.build.version.sdk"]),
            ("Build Date", &["getprop", "ro.build.date"]),
            ("Security Patch", &["getprop", "ro.build.version.security_patch"]),
            ("IMEI", &["service", "call", "iphonesubinfo", "1"]),
            ("Serial", &["getprop", "ro.serialno"]),
        ];

        for (label, cmd) in props {
            if let Ok(out) = adb.shell(&serial, cmd) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                // Special handling for IMEI (service call returns hex)
                if *label == "IMEI" {
                    // Try to decode hex from service call
                    let mut decoded = String::new();
                    for word in text.split_whitespace() {
                        if word.len() == 8 && word.chars().all(|c| c.is_ascii_hexdigit()) {
                            for i in (0..8).step_by(2) {
                                if let Ok(byte) = u8::from_str_radix(&word[i..i+2], 16) {
                                    if byte >= 0x20 && byte < 0x7f { decoded.push(byte as char); }
                                }
                            }
                        }
                    }
                    let imei: String = decoded.chars().filter(|c| c.is_ascii_digit()).collect();
                    if imei.len() >= 14 && !imei.chars().all(|c| c == '0') {
                        report.push_str(&format!("{}: {}\n", label, imei));
                    } else {
                        // Fallback to getprop
                        if let Ok(out2) = adb.shell(&serial, &["getprop", "gsm.imei"]) {
                            let v = String::from_utf8_lossy(&out2).trim().to_string();
                            if !v.is_empty() && v != "null" {
                                report.push_str(&format!("{}: {}\n", label, v));
                            } else {
                                report.push_str(&format!("{}: Not accessible\n", label));
                            }
                        }
                    }
                } else {
                    report.push_str(&format!("{}: {}\n", label, text));
                }
            }
        }

        // Lock Status
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  SECURITY STATUS\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "lockscreen.password_type"]) {
            let raw = String::from_utf8_lossy(&out);
            let trimmed = raw.trim();
            let lock_type = match trimmed {
                "0" => "None",
                "65536" => "Pattern",
                "131072" => "PIN",
                "262144" => "Password",
                other => other,
            };
            report.push_str(&format!("Lock Type: {}\n", lock_type));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.state"]) {
            report.push_str(&format!("Encryption: {}\n", String::from_utf8_lossy(&out).trim()));
        }

        // Installed Apps
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  INSTALLED APPLICATIONS\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        if let Ok(out) = adb.shell(&serial, &["pm", "list", "packages", "-3"]) {
            let text = String::from_utf8_lossy(&out);
            let apps: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
            report.push_str(&format!("User apps: {}\n", apps.len()));
            for app in apps.iter().take(50) {
                report.push_str(&format!("  {}\n", app.replace("package:", "")));
            }
            if apps.len() > 50 {
                report.push_str(&format!("  ... and {} more\n", apps.len() - 50));
            }
        }

        // Accounts
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  REGISTERED ACCOUNTS\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        if let Ok(out) = adb.shell(&serial, &["dumpsys", "account", "|", "grep", "Account {"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                report.push_str(&format!("  {}\n", line.trim()));
            }
        }

        // Network Info
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  NETWORK INFORMATION\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        if let Ok(out) = adb.shell(&serial, &["dumpsys", "wifi", "|", "grep", "mWifiInfo"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            // Parse WiFi info: extract SSID and signal
            let ssid = text.split("SSID:").nth(1)
                .and_then(|s| s.split(|c: char| c == ',' || c == ' ').next())
                .unwrap_or("?");
            let signal = text.split("RSSI:").nth(1)
                .and_then(|s| s.split(|c: char| c == ',' || c == ' ').next())
                .unwrap_or("?");
            report.push_str(&format!("WiFi: SSID={} Signal={} dBm\n", ssid, signal));
        }
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "connectivity", "|", "grep", "-i", "active"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_active = false;
            for line in text.lines().take(5) {
                let l = line.trim();
                if l.contains("Active") || l.contains("active") || l.contains("NetworkAgentInfo") {
                    // Extract network type
                    let net_type = l.split("[").nth(1)
                        .and_then(|s| s.split("]").next())
                        .unwrap_or(l);
                    report.push_str(&format!("Active network: {}\n", net_type));
                    found_active = true;
                    break;
                }
            }
            if !found_active { report.push_str("No active network detected\n"); }
        }

        // Storage
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  STORAGE OVERVIEW\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        if let Ok(out) = adb.shell(&serial, &["df", "/data"]) {
            let text = String::from_utf8_lossy(&out);
            // Parse df output: skip header, show key fields
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    report.push_str(&format!("/data: Total={} Used={} Free={} Use%={}\n", parts[1], parts[2], parts[3], parts[4]));
                }
            }
        }
        if let Ok(out) = adb.shell(&serial, &["df", "/sdcard"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    report.push_str(&format!("/sdcard: Total={} Used={} Free={} Use%={}\n", parts[1], parts[2], parts[3], parts[4]));
                }
            }
        }

        // Anti-forensics check
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str("  ANTI-FORENSICS CHECK\n");
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n\n");

        let wiper_apps = [
            "com.sunbeach.wiper", "com.zapper", "com.cleaner.master",
            "com.app.remover", "com.secure.wipe",
        ];
        let mut found_wipers = Vec::new();
        for app in &wiper_apps {
            if let Ok(out) = adb.shell(&serial, &["pm", "path", app]) {
                if String::from_utf8_lossy(&out).contains("base.apk") {
                    found_wipers.push(*app);
                }
            }
        }
        if found_wipers.is_empty() {
            report.push_str("No known wiper apps detected\n");
        } else {
            report.push_str("WARNING: Wiper apps detected:\n");
            for app in &found_wipers {
                report.push_str(&format!("  âš  {}\n", app));
            }
        }

        // Footer
        report.push_str("\nâ•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");
        report.push_str(&format!("  Report generated by RecoveryKit Forensic Suite\n"));
        report.push_str(&format!("  Timestamp: {}\n", humantime_format(timestamp)));
        report.push_str("â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•\n");

        // Write to file
        std::fs::write(&report_path, &report)
            .map_err(|e| format!("Failed to write report: {}", e))?;

        report.push_str(&format!("\nReport saved to: {}\n", report_path));
        Ok(report)
    })?
}

/// Start live logcat streaming. Emits "logcat-line" events.
#[tauri::command]
async fn start_logcat(serial: String, app_handle: tauri::AppHandle) -> Result<String, String> {
    let adb = adb::AdbClient::default();
    let serial_clone = serial.clone();
    tokio::task::spawn_blocking(move || {
        // Use logcat -T 1 to start from current time, no buffer replay
        let result = std::process::Command::new("adb")
            .args(["-s", &serial_clone, "logcat", "-T", "1", "-v", "brief"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn();
        match result {
            Ok(mut child) => {
                use std::io::BufRead;
                if let Some(stdout) = child.stdout.take() {
                    let reader = std::io::BufReader::new(stdout);
                    for line in reader.lines().flatten().take(500) {
                        let _ = app_handle.emit("logcat-line", &line);
                    }
                }
                let _ = child.kill();
                Ok::<_, String>("Logcat stream ended (500 lines captured)".to_string())
            }
            Err(e) => Err(format!("Failed to start logcat: {}", e)),
        }
    }).await.map_err(|e| e.to_string())?
}

/// Get a snapshot of recent logcat lines.
#[tauri::command]
fn get_logcat_snapshot(serial: String, lines: Option<u32>) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let count = lines.unwrap_or(100).min(500);
        let out = adb.shell(&serial, &["logcat", "-d", "-t", &count.to_string(), "-v", "brief"])
            .map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&out).to_string())
    })?
}

/// Search across all extracted data files for a keyword.
#[tauri::command]
fn global_keyword_search(data_dir: String, keyword: String) -> Result<String, String> {
    run_blocking(move || {
        let mut hits = Vec::new();
        let keyword_lower = keyword.to_lowercase();

        fn search_dir(dir: &std::path::Path, keyword_lower: &str, hits: &mut Vec<String>) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        search_dir(&path, keyword_lower, hits);
                    } else if path.is_file() {
                        // Search text files
                        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                            let searchable = ["txt", "json", "xml", "csv", "html", "log", "db", "sql", "vcf"];
                            if searchable.contains(&ext) {
                                if let Ok(content) = std::fs::read_to_string(&path) {
                                    let content_lower = content.to_lowercase();
                                    let count = content_lower.matches(keyword_lower).count();
                                    if count > 0 {
                                        let rel = path.file_name().unwrap_or_default().to_string_lossy();
                                        hits.push(format!("{} ({} matches) â€” {}", rel, count, path.display()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let dir_path = std::path::Path::new(&data_dir);
        if !dir_path.exists() {
            return Err(format!("Directory does not exist: {}", data_dir));
        }
        search_dir(dir_path, &keyword_lower, &mut hits);

        if hits.is_empty() {
            Ok(format!("No matches for '{}' in {}", keyword, data_dir))
        } else {
            let mut result = format!("Found '{}' in {} files:\n\n", keyword, hits.len());
            for hit in &hits {
                result.push_str(hit);
                result.push('\n');
            }
            Ok(result)
        }
    })?
}

/// Build a usage timeline from device activity.
#[tauri::command]
fn get_usage_timeline(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut timeline = Vec::new();

        // 1. Recent app usage from usagestats
        let usage_out = adb.shell(&serial, &["dumpsys", "usagestats", "--sort"]);
        if let Ok(usage_bytes) = usage_out {
            let usage_text = String::from_utf8_lossy(&usage_bytes);
            for line in usage_text.lines().take(30) {
                let l = line.trim();
                if l.contains("com.") || l.contains("android.") {
                    timeline.push(format!("[App Activity] {}", l));
                }
            }
        }

        // 2. Recent call log
        let call_out = adb.shell(&serial, &["content", "query", "--uri", "content://call_log/calls", "--projection", "number:date:duration:type", "--sort", "date DESC"]);
        if let Ok(call_bytes) = call_out {
            let call_text = String::from_utf8_lossy(&call_bytes);
            for line in call_text.lines().take(10) {
                if line.contains("Row") {
                    timeline.push(format!("[Call] {}", line.trim()));
                }
            }
        }

        // 3. Recent SMS
        let sms_out = adb.shell(&serial, &["content", "query", "--uri", "content://sms/", "--projection", "address:date:body", "--sort", "date DESC"]);
        if let Ok(sms_bytes) = sms_out {
            let sms_text = String::from_utf8_lossy(&sms_bytes);
            for line in sms_text.lines().take(10) {
                if line.contains("Row") {
                    timeline.push(format!("[SMS] {}", line.trim()));
                }
            }
        }

        // 4. Last boot time
        let boot_out = adb.shell(&serial, &["getprop", "ro.boottime.init"]);
        if let Ok(boot_bytes) = boot_out {
            let boot = String::from_utf8_lossy(&boot_bytes).trim().to_string();
            if !boot.is_empty() {
                timeline.push(format!("[Boot] Last init: {}ns", boot));
            }
        }

        // 5. Uptime
        let up_out = adb.shell(&serial, &["uptime"]);
        if let Ok(up_bytes) = up_out {
            let up = String::from_utf8_lossy(&up_bytes).trim().to_string();
            timeline.push(format!("[Uptime] {}", up));
        }

        if timeline.is_empty() {
            Ok("No timeline data available".to_string())
        } else {
            Ok(timeline.join("\n"))
        }
    })?
}

/// Generate a forensic report as HTML.
#[tauri::command]
fn generate_forensic_report(case_id: String, device_serial: String, data_dir: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let report_path = std::path::Path::new(&output_dir).join(format!("forensic_report_{}.html", case_id));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();

        // Collect file listing from data_dir
        let mut file_list = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                file_list.push(format!("<tr><td>{}</td><td>{} bytes</td></tr>", name, size));
            }
        }

        let html = format!(r#"<!DOCTYPE html>
<html><head><title>Forensic Report â€” Case {}</title>
<style>
body {{ font-family: 'Segoe UI', sans-serif; max-width: 900px; margin: 40px auto; padding: 20px; background: #f8f9fa; }}
h1 {{ color: #1a365d; border-bottom: 3px solid #e53e3e; padding-bottom: 10px; }}
h2 {{ color: #2d3748; margin-top: 30px; }}
table {{ border-collapse: collapse; width: 100%; margin: 10px 0; }}
th, td {{ border: 1px solid #cbd5e0; padding: 8px 12px; text-align: left; }}
th {{ background: #edf2f7; font-weight: 600; }}
.meta {{ color: #718096; font-size: 0.9em; }}
.footer {{ margin-top: 40px; padding-top: 20px; border-top: 1px solid #cbd5e0; color: #a0aec0; font-size: 0.8em; }}
</style></head>
<body>
<h1>ðŸ” Forensic Examination Report</h1>
<h2>Case Information</h2>
<table>
<tr><th>Case ID</th><td>{}</td></tr>
<tr><th>Device Serial</th><td>{}</td></tr>
<tr><th>Examination Date</th><td>{}</td></tr>
<tr><th>Examiner</th><td>RecoveryKit Automated</td></tr>
</table>

<h2>Evidence Files</h2>
<table>
<tr><th>File Name</th><th>Size</th></tr>
{}
</table>

<h2>Chain of Custody</h2>
<p class="meta">This report was generated automatically by RecoveryKit Forensic Tool.
All evidence files are stored in: {}</p>

<div class="footer">
<p>Generated by RecoveryKit v0.1.0 â€” Read-only forensic examination tool.</p>
<p>Report timestamp: {}</p>
</div>
</body></html>"#,
            case_id, case_id, device_serial, now, file_list.join("\n"), data_dir, now);

        std::fs::write(&report_path, &html).map_err(|e| e.to_string())?;
        Ok(format!("Report saved to {}", report_path.display()))
    })?
}

/// Get app permissions audit.
#[tauri::command]
fn audit_app_permissions(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        // Get list of installed third-party apps
        let pkgs_out = adb.shell(&serial, &["pm", "list", "packages", "-3"])
            .map_err(|e| e.to_string())?;
        let pkgs_text = String::from_utf8_lossy(&pkgs_out);
        let mut audit = Vec::new();

        for line in pkgs_text.lines().take(20) {
            let pkg = line.trim_start_matches("package:").trim();
            if pkg.is_empty() { continue; }
            // Get granted permissions
            let perms_out = adb.shell(&serial, &["dumpsys", "package", pkg]);
            if let Ok(perms_bytes) = perms_out {
                let perms_text = String::from_utf8_lossy(&perms_bytes);
                let mut dangerous = Vec::new();
                for pline in perms_text.lines() {
                    let l = pline.trim();
                    if l.contains("granted=true") && (l.contains("CAMERA") || l.contains("RECORD_AUDIO") || l.contains("ACCESS_FINE_LOCATION") || l.contains("READ_CONTACTS") || l.contains("READ_SMS") || l.contains("READ_CALL_LOG")) {
                        let perm = l.split_whitespace().next().unwrap_or(l);
                        dangerous.push(perm.to_string());
                    }
                }
                if !dangerous.is_empty() {
                    audit.push(format!("{}: {}", pkg, dangerous.join(", ")));
                }
            }
        }

        if audit.is_empty() {
            Ok("No dangerous permissions found (or no third-party apps)".to_string())
        } else {
            Ok(format!("Permission Audit (top 20 apps):\n{}", audit.join("\n")))
        }
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
//  SECURITY & FORENSICS â€” Spyware, Cloning, Bluetooth, Network
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// Detect potentially malicious/spyware apps on the device.
#[tauri::command]
fn detect_spyware(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut flags = Vec::new();

        // Get all third-party packages
        let pkgs_out = adb.shell(&serial, &["pm", "list", "packages", "-3", "-f"])
            .map_err(|e| e.to_string())?;
        let pkgs_text = String::from_utf8_lossy(&pkgs_out);

        for line in pkgs_text.lines() {
            let pkg = line.trim_start_matches("package:").trim();
            if pkg.is_empty() { continue; }
            let pkg_name = pkg.split('=').last().unwrap_or(pkg).trim();

            let info_out = adb.shell(&serial, &["dumpsys", "package", pkg_name]);
            if let Ok(info_bytes) = info_out {
                let info = String::from_utf8_lossy(&info_bytes);
                let mut reasons = Vec::new();

                // Check 1: Hidden from launcher (no launcher activity)
                if !info.contains("android.intent.category.LAUNCHER") {
                    reasons.push("no launcher icon");
                }

                // Check 2: Has dangerous permissions
                let dangerous_perms = ["CAMERA", "RECORD_AUDIO", "ACCESS_FINE_LOCATION",
                    "READ_CONTACTS", "READ_SMS", "READ_CALL_LOG", "READ_PHONE_STATE"];
                let mut has_dangerous = false;
                for perm in &dangerous_perms {
                    if info.contains(perm) && info.contains("granted=true") {
                        has_dangerous = true;
                        break;
                    }
                }

                // Check 3: Runs background services
                if info.contains("Service {") && info.contains("running=true") {
                    reasons.push("running background service");
                }

                // Check 4: Installed from unknown source (not Play Store)
                if !info.contains("com.android.vending") && !info.contains("com.google.android.packageinstaller") {
                    if info.contains("installerPackageName") {
                        reasons.push("sideloaded");
                    }
                }

                // Check 5: Has system overlay permission (can draw over other apps)
                if info.contains("SYSTEM_ALERT_WINDOW") && info.contains("granted=true") {
                    reasons.push("overlay permission");
                }

                // Check 6: Device admin
                if info.contains("DeviceAdminReceiver") || info.contains("device-admin") {
                    reasons.push("device admin");
                }

                // Check 7: Accessibility service
                if info.contains("AccessibilityService") {
                    reasons.push("accessibility service");
                }

                if has_dangerous && !reasons.is_empty() {
                    flags.push(format!("âš  {} â€” {}", pkg_name, reasons.join(", ")));
                }
            }
        }

        if flags.is_empty() {
            Ok("No suspicious apps detected".to_string())
        } else {
            Ok(format!("Found {} potentially suspicious apps:\n\n{}", flags.len(), flags.join("\n")))
        }
    })?
}

/// Detect linked/cloned devices for WhatsApp, Telegram, etc.
#[tauri::command]
fn detect_linked_devices(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut results = Vec::new();

        // WhatsApp Web sessions
        let wa_out = adb.shell(&serial, &["cat", "/data/data/com.whatsapp/shared_prefs/com.whatsapp_preferences_light.xml"]);
        if let Ok(wa_bytes) = wa_out {
            let wa_text = String::from_utf8_lossy(&wa_bytes);
            if wa_text.contains("Companion") || wa_text.contains("linked") || wa_text.contains("web") {
                results.push("WhatsApp: Linked device sessions found in preferences".to_string());
            }
            // Check for companion device registration
            if wa_text.contains("companion") || wa_text.contains("secret_key") {
                results.push("WhatsApp: Companion device (linked phone) detected".to_string());
            }
        }

        // Telegram sessions
        let tg_out = adb.shell(&serial, &["ls", "/data/data/org.telegram.messenger/files/"]);
        if let Ok(tg_bytes) = tg_out {
            let tg_text = String::from_utf8_lossy(&tg_bytes);
            if tg_text.contains("cache4") || tg_text.contains("account") {
                results.push("Telegram: Active session data found".to_string());
            }
        }

        // Check all active accounts on device
        let acc_out = adb.shell(&serial, &["dumpsys", "account"]);
        if let Ok(acc_bytes) = acc_out {
            let acc_text = String::from_utf8_lossy(&acc_bytes);
            let mut accounts = Vec::new();
            for line in acc_text.lines() {
                let l = line.trim();
                if l.contains("Account {") {
                    accounts.push(l.to_string());
                }
            }
            if !accounts.is_empty() {
                results.push(format!("Active accounts ({}):\n{}", accounts.len(), accounts.join("\n")));
            }
        }

        // Google account linked devices via device_policy
        let dp_out = adb.shell(&serial, &["dumpsys", "device_policy"]);
        if let Ok(dp_bytes) = dp_out {
            let dp_text = String::from_utf8_lossy(&dp_bytes);
            for line in dp_text.lines() {
                if line.contains("mDeviceOwner") || line.contains("ProfileOwner") {
                    results.push(format!("Device policy: {}", line.trim()));
                }
            }
        }

        // Check for active Bluetooth connections (nearby devices)
        let bt_out = adb.shell(&serial, &["dumpsys", "bluetooth_manager"]);
        if let Ok(bt_bytes) = bt_out {
            let bt_text = String::from_utf8_lossy(&bt_bytes);
            for line in bt_text.lines() {
                if line.contains("address=") || line.contains("name=") {
                    results.push(format!("BT: {}", line.trim()));
                }
            }
        }

        if results.is_empty() {
            Ok("No linked devices or sessions detected".to_string())
        } else {
            Ok(results.join("\n"))
        }
    })?
}

/// Get Bluetooth paired devices and connection history.
#[tauri::command]
fn get_bluetooth_forensics(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Bluetooth Forensics ═══\n\n");

        // Adapter info
        report.push_str("── Adapter Information ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_manager | head -40"]) {
            let text = String::from_utf8_lossy(&out);
            let mut adapter_name = String::new();
            let mut adapter_addr = String::new();
            let mut bt_enabled = false;
            let mut state = String::new();

            for line in text.lines() {
                let l = line.trim();
                if l.contains("mAdapter:") {
                    // Extract adapter address
                    if let Some(addr) = l.split("address=").nth(1).and_then(|s| s.split(|c: char| c == ',' || c == ' ').next()) {
                        adapter_addr = addr.to_string();
                    }
                }
                if l.contains("mAdapterConnectionState") {
                    state = l.split('=').nth(1).unwrap_or("").trim().to_string();
                }
                if l.contains("mEnable") && l.contains("true") { bt_enabled = true; }
                if l.contains("BluetoothAdapter") && l.contains("name=") {
                    adapter_name = l.split("name=").nth(1).and_then(|s| s.split(|c: char| c == ',' || c == ' ').next()).unwrap_or("").to_string();
                }
            }
            if !adapter_addr.is_empty() { report.push_str(&format!("  MAC Address: {}\n", adapter_addr)); }
            if !adapter_name.is_empty() { report.push_str(&format!("  Device Name: {}\n", adapter_name)); }
            report.push_str(&format!("  Bluetooth: {}\n", if bt_enabled { "Enabled" } else { "Disabled" }));
            if !state.is_empty() { report.push_str(&format!("  Connection State: {}\n", state)); }
        }

        // Local adapter name from settings
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "bluetooth_name"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                report.push_str(&format!("  Bluetooth Name (settings): {}\n", v));
            }
        }

        // Paired/bonded devices — deep parse
        report.push_str("\n── Paired Devices ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_manager"]) {
            let text = String::from_utf8_lossy(&out);
            let mut in_paired = false;
            let mut paired_count = 0;

            for line in text.lines() {
                let l = line.trim();

                // Detect mPairedDevices section
                if l.contains("mPairedDevices") { in_paired = true; continue; }
                if in_paired && (l.starts_with("mConnectedDevices") || l.starts_with("mBondedDevices") && !l.contains("mPairedDevices")) { in_paired = false; }

                if in_paired && l.contains("BluetoothDevice{") {
                    // Parse: BluetoothDevice{XX:XX:XX:XX:XX:XX} name=DeviceName
                    let addr = l.split('{').nth(1).and_then(|s| s.split('}').next()).unwrap_or("?");
                    let name = l.split("name=").nth(1).and_then(|s| s.split(|c: char| c == ',' || c == ' ' || c == '}').next()).unwrap_or("Unknown");
                    report.push_str(&format!("  {} ({})\n", if name != "Unknown" { name } else { addr }, addr));
                    paired_count += 1;
                }
            }
            if paired_count == 0 {
                // Fallback: try settings secure bluetooth_discovered_devices
                report.push_str("  Parsing from settings...\n");
                if let Ok(out2) = adb.shell(&serial, &["sh", "-c", "settings get secure bluetooth_discovered_devices 2>/dev/null"]) {
                    let text2 = String::from_utf8_lossy(&out2).trim().to_string();
                    if !text2.is_empty() && text2 != "null" {
                        for dev in text2.split(";;") {
                            if !dev.is_empty() {
                                let parts: Vec<&str> = dev.split('|').collect();
                                if parts.len() >= 2 {
                                    report.push_str(&format!("  {} ({})\n", parts.get(1).unwrap_or(&"Unknown"), parts.get(0).unwrap_or(&"?")));
                                    paired_count += 1;
                                }
                            }
                        }
                    }
                }
                if paired_count == 0 { report.push_str("  No paired devices found\n"); }
            }
            report.push_str(&format!("  Total paired: {}\n", paired_count));
        }

        // Currently connected devices
        report.push_str("\n── Connected Devices ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_manager | grep -A 2 'mConnectedDevices'"]) {
            let text = String::from_utf8_lossy(&out);
            let mut connected = false;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("mConnectedDevices") { connected = true; continue; }
                if connected && l.contains("BluetoothDevice{") {
                    let addr = l.split('{').nth(1).and_then(|s| s.split('}').next()).unwrap_or("?");
                    let name = l.split("name=").nth(1).and_then(|s| s.split(|c: char| c == ',' || c == ' ').next()).unwrap_or("Unknown");
                    report.push_str(&format!("  {} ({}) — CONNECTED\n", if name != "Unknown" { name } else { addr }, addr));
                }
            }
        }

        // Bond states
        report.push_str("\n── Bond States ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth | grep -E 'address=|bondState|type=' | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("bondState") || l.contains("address=") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // A2DP codec info
        report.push_str("\n── A2DP Audio Codec ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_a2dp 2>/dev/null | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("codec") || l.contains("Codec") || l.contains("active") || l.contains("device") || l.contains("mDevice") || l.contains("LDAC") || l.contains("AAC") || l.contains("SBC") || l.contains("aptX")) {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No A2DP codec info (no audio device connected)\n"); }
        }

        // Bluetooth LE scan results / nearby
        report.push_str("\n── BLE / Nearby Devices ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_manager | grep -E 'ScanResult|BLE|LeScan|lastSeen' | head -15"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("ScanResult") || l.contains("address") || l.contains("rssi")) {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No BLE scan results cached\n"); }
        }

        // Bluetooth profiles in use
        report.push_str("\n── Active Profiles ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys bluetooth_manager | grep -E 'Profile|A2dpService|HeadsetService|HidService|PanService|MapService|PbapService' | head -15"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("connected") || l.contains("Connected") || l.contains("mProfile") || l.contains("Service")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        Ok(report)
    })?
}

/// Get active network connections (TCP/UDP).
#[tauri::command]
fn get_network_connections(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Network Connection Forensics ═══\n\n");

        // Helper: decode hex IP from /proc/net/tcp (little-endian u32 → dotted quad)
        let decode_hex_ip = |hex: &str| -> String {
            if hex.len() != 8 { return hex.to_string(); }
            let addr = u32::from_str_radix(hex, 16).unwrap_or(0);
            format!("{}.{}.{}.{}", addr & 0xFF, (addr >> 8) & 0xFF, (addr >> 16) & 0xFF, (addr >> 24) & 0xFF)
        };
        let decode_hex_port = |hex: &str| -> u16 {
            u16::from_str_radix(hex, 16).unwrap_or(0)
        };
        let decode_ip_port = |combined: &str| -> (String, u16) {
            let parts: Vec<&str> = combined.split(':').collect();
            if parts.len() == 2 {
                (decode_hex_ip(parts[0]), decode_hex_port(parts[1]))
            } else {
                (combined.to_string(), 0)
            }
        };
        let state_name = |hex: &str| -> &str {
            match hex {
                "01" => "ESTABLISHED", "02" => "SYN_SENT", "03" => "SYN_RECV",
                "04" => "FIN_WAIT1", "05" => "FIN_WAIT2", "06" => "TIME_WAIT",
                "07" => "CLOSE", "08" => "CLOSE_WAIT", "09" => "LAST_ACK",
                "0A" => "LISTEN", "0B" => "CLOSING", _ => "UNKNOWN"
            }
        };

        // Build UID→package mapping
        let mut uid_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys package | grep -E 'userId|Uid|packageName' | head -500"]) {
            let text = String::from_utf8_lossy(&out);
            let mut current_pkg = String::new();
            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("Package [") {
                    current_pkg = l.split('[').nth(1).and_then(|s| s.split(']').next()).unwrap_or("").to_string();
                } else if l.starts_with("userId=") || l.starts_with("uid=") {
                    let uid_val = l.split('=').nth(1).and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next()).unwrap_or("").to_string();
                    if !uid_val.is_empty() && !current_pkg.is_empty() {
                        uid_map.entry(uid_val).or_insert(current_pkg.clone());
                    }
                }
            }
        }
        // Fallback: simpler mapping via pm list
        if uid_map.len() < 5 {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", "pm list packages -U 2>/dev/null | head -100"]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    // package:com.example uid:10001
                    if let Some(pkg) = line.split("package:").nth(1).and_then(|s| s.split_whitespace().next()) {
                        if let Some(uid) = line.split("uid:").nth(1).and_then(|s| s.split_whitespace().next()) {
                            uid_map.entry(uid.to_string()).or_insert(pkg.to_string());
                        }
                    }
                }
            }
        }

        // TCP connections
        report.push_str("── TCP Connections ──\n");
        let mut tcp_established = 0u32;
        let mut tcp_listen = 0u32;
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/tcp"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 10 { continue; }
                let state = parts[3];
                if state != "01" && state != "0A" { continue; }
                let (local_ip, local_port) = decode_ip_port(parts[1]);
                let (remote_ip, remote_port) = decode_ip_port(parts[2]);
                let uid = parts.get(7).unwrap_or(&"").to_string();
                let app = uid_map.get(&uid).cloned().unwrap_or_else(|| {
                    if uid == "0" { "system".to_string() } else { format!("uid:{}", uid) }
                });
                let st = state_name(state);
                if state == "01" { tcp_established += 1; } else { tcp_listen += 1; }
                report.push_str(&format!("  [{:12}] {}:{} → {}:{} ({})\n", st, local_ip, local_port, remote_ip, remote_port, app));
            }
        }
        report.push_str(&format!("\n  Summary: {} established, {} listening\n", tcp_established, tcp_listen));

        // TCP6 connections
        report.push_str("\n── TCP6 (IPv6) Connections ──\n");
        let mut tcp6_count = 0u32;
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/tcp6"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 10 { continue; }
                let state = parts[3];
                if state != "01" && state != "0A" { continue; }
                let local_port = parts[1].rsplit(':').next().map(|p| decode_hex_port(p)).unwrap_or(0);
                let remote_port = parts[2].rsplit(':').next().map(|p| decode_hex_port(p)).unwrap_or(0);
                let uid = parts.get(7).unwrap_or(&"").to_string();
                let app = uid_map.get(&uid).cloned().unwrap_or_else(|| {
                    if uid == "0" { "system".to_string() } else { format!("uid:{}", uid) }
                });
                report.push_str(&format!("  [{}] port {} → port {} ({})\n", state_name(state), local_port, remote_port, app));
                tcp6_count += 1;
                if tcp6_count >= 30 { break; }
            }
        }
        if tcp6_count == 0 { report.push_str("  No IPv6 connections\n"); }

        // UDP connections
        report.push_str("\n── UDP Connections ──\n");
        let mut udp_count = 0u32;
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/udp"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() < 10 { continue; }
                let (local_ip, local_port) = decode_ip_port(parts[1]);
                let (remote_ip, remote_port) = decode_ip_port(parts[2]);
                let uid = parts.get(7).unwrap_or(&"").to_string();
                let app = uid_map.get(&uid).cloned().unwrap_or_else(|| {
                    if uid == "0" { "system".to_string() } else { format!("uid:{}", uid) }
                });
                report.push_str(&format!("  {}:{} → {}:{} ({})\n", local_ip, local_port, remote_ip, remote_port, app));
                udp_count += 1;
                if udp_count >= 30 { break; }
            }
        }
        if udp_count == 0 { report.push_str("  No UDP connections\n"); }

        // Routing table
        report.push_str("\n── Routing Table ──\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/route"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 8 {
                    let iface = parts[0];
                    let dest = decode_hex_ip(parts[1]);
                    let gw = decode_hex_ip(parts[2]);
                    let mask = decode_hex_ip(parts[7]);
                    if dest == "0.0.0.0" {
                        report.push_str(&format!("  {} → default via {} (mask: {})\n", iface, gw, mask));
                    } else {
                        report.push_str(&format!("  {} → {}/{} via {}\n", iface, dest, mask, gw));
                    }
                }
            }
        }

        // ARP cache
        report.push_str("\n── ARP Cache ──\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/arp"]) {
            let text = String::from_utf8_lossy(&out);
            let mut arp_count = 0;
            for line in text.lines().skip(1) {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 {
                    report.push_str(&format!("  {} → {} ({}) [{}]\n", parts[0], parts[3], parts[2], parts[5]));
                    arp_count += 1;
                }
            }
            if arp_count == 0 { report.push_str("  Empty ARP cache\n"); }
        }

        // DNS servers
        report.push_str("\n── DNS Configuration ──\n");
        for prop in &["net.dns1", "net.dns2", "net.rmnet0.dns1", "net.rmnet0.dns2"] {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() && v != "null" {
                    report.push_str(&format!("  {}: {}\n", prop, v));
                }
            }
        }

        // Network stats per app
        report.push_str("\n── Per-App Network Usage ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys netstats --proto bin 2>/dev/null || dumpsys netstats | grep -E 'package=|rxBytes=|txBytes=' | head -60"]) {
            let text = String::from_utf8_lossy(&out);
            let mut pkg_rx: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            let mut pkg_tx: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
            let mut current_pkg = String::new();
            for line in text.lines() {
                let l = line.trim();
                if let Some(pkg) = l.strip_prefix("package=") {
                    current_pkg = pkg.split(|c: char| c == ' ' || c == ',').next().unwrap_or("").to_string();
                }
                if let Some(rx_str) = l.strip_prefix("rxBytes=") {
                    if let Ok(rx) = rx_str.split(|c: char| c == ' ' || c == ',').next().unwrap_or("0").parse::<u64>() {
                        *pkg_rx.entry(current_pkg.clone()).or_insert(0) += rx;
                    }
                }
                if let Some(tx_str) = l.strip_prefix("txBytes=") {
                    if let Ok(tx) = tx_str.split(|c: char| c == ' ' || c == ',').next().unwrap_or("0").parse::<u64>() {
                        *pkg_tx.entry(current_pkg.clone()).or_insert(0) += tx;
                    }
                }
            }
            let fmt_bytes = |b: u64| -> String {
                if b > 1_073_741_824 { format!("{:.1} GB", b as f64 / 1_073_741_824.0) }
                else if b > 1_048_576 { format!("{:.1} MB", b as f64 / 1_048_576.0) }
                else if b > 1024 { format!("{:.1} KB", b as f64 / 1024.0) }
                else { format!("{} B", b) }
            };
            let mut apps: Vec<(String, u64, u64)> = pkg_rx.keys()
                .map(|k| (k.clone(), *pkg_rx.get(k).unwrap_or(&0), *pkg_tx.get(k).unwrap_or(&0)))
                .collect();
            apps.sort_by(|a, b| (b.1 + b.2).cmp(&(a.1 + a.2)));
            for (pkg, rx, tx) in apps.iter().take(20) {
                report.push_str(&format!("  {:40} ↓{} ↑{}\n", pkg, fmt_bytes(*rx), fmt_bytes(*tx)));
            }
            if apps.is_empty() { report.push_str("  No per-app stats available\n"); }
        }

        Ok(report)
    })?
}

/// Start clipboard monitoring â€” polls clipboard every interval and emits events.
#[tauri::command]
fn get_clipboard_history(serial: String, count: Option<u32>) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let n = count.unwrap_or(5).min(20);
        let mut clips = Vec::new();

        for i in 0..n {
            let clip_out = adb.shell(&serial, &["am", "broadcast", "-a", "clipper.get"]);
            if let Ok(clip_bytes) = clip_out {
                let clip_text = String::from_utf8_lossy(&clip_bytes);
                if clip_text.contains("data=") {
                    if let Some(start) = clip_text.find("data=\"") {
                        let rest = &clip_text[start + 6..];
                        if let Some(end) = rest.find('"') {
                            let content = &rest[..end];
                            if !content.is_empty() {
                                clips.push(format!("[{}] {}", i + 1, content));
                            }
                        }
                    }
                }
            }
            if i < n - 1 {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }

        if clips.is_empty() {
            Ok("No clipboard content captured (clipboard may be empty or Clipper app not installed)".to_string())
        } else {
            Ok(format!("Clipboard samples ({} polls):\n{}", clips.len(), clips.join("\n")))
        }
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// ADVANCED FORENSIC FEATURES (#21-#38)
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// #21 Recover deleted SMS/MMS messages from mmssms.db (advanced)
#[tauri::command]
fn recover_deleted_sms_advanced(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Deleted SMS/MMS Recovery â•â•â•\n\n");

        let db_paths = [
            "/data/data/com.android.providers.telephony/databases/mmssms.db",
            "/data/user/0/com.android.providers.telephony/databases/mmssms.db",
        ];
        let local_db = format!("{}\\mmssms.db", output_dir);
        let mut found = false;

        for db_path in &db_paths {
            let pull_cmd = format!("run-as com.android.providers.telephony cat {}", db_path);
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &pull_cmd]) {
                if !out.is_empty() {
                    let _ = std::fs::write(&local_db, &out);
                    report.push_str(&format!("Pulled mmssms.db: {} bytes\n\n", out.len()));
                    found = true;
                    break;
                }
            }
            if adb.pull(&serial, db_path, std::path::Path::new(&local_db)).is_ok() {
                if std::path::Path::new(&local_db).metadata().map(|m| m.len()).unwrap_or(0) > 100 {
                    report.push_str(&format!("Pulled mmssms.db via root\n\n"));
                    found = true;
                    break;
                }
            }
        }

        if !found {
            // Fallback: content provider query
            report.push_str("Direct DB access failed â€” trying content provider...\n\n");
            if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri",
                "content://sms", "--projection", "address:body:date:type:read:deleted"]) {
                let text = String::from_utf8_lossy(&out);
                let mut total = 0;
                let mut deleted = 0;
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("Row") {
                        total += 1;
                        let is_deleted = l.contains("deleted=1");
                        let address = l.split("address=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("?");
                        let body = l.split("body=").nth(1).and_then(|s| s.split(", date=").next()).unwrap_or("?");
                        let date = l.split("date=").nth(1).and_then(|s| s.split(", type=").next()).unwrap_or("?");
                        let msg_type = l.split("type=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("?");
                        let type_name = match msg_type { "1" => "INBOX", "2" => "SENT", "3" => "DRAFT", "4" => "OUTBOX", _ => msg_type };
                        let date_str = if let Ok(ms) = date.parse::<u64>() { humantime_format(ms / 1000) } else { date.to_string() };
                        if is_deleted {
                            deleted += 1;
                            report.push_str(&format!("  [DELETED] {} | {} | {} | {}\n", date_str, type_name, address, body));
                        }
                    }
                }
                report.push_str(&format!("\nTotal SMS: {} | Deleted: {}\n", total, deleted));
            }
            return Ok(report);
        }

        // Parse SQLite DB
        match rusqlite::Connection::open(&local_db) {
            Ok(conn) => {
                // Active messages
                let total: i64 = conn.query_row("SELECT COUNT(*) FROM sms", [], |r| r.get(0)).unwrap_or(0);
                let deleted: i64 = conn.query_row("SELECT COUNT(*) FROM sms WHERE deleted=1", [], |r| r.get(0)).unwrap_or(0);
                report.push_str(&format!("Total SMS in DB: {}\nDeleted: {}\n\n", total, deleted));

                // Recover deleted
                if deleted > 0 {
                    report.push_str("â”€â”€ Deleted Messages â”€â”€\n");
                    let mut stmt = conn.prepare(
                        "SELECT address, body, date, type FROM sms WHERE deleted=1 ORDER BY date DESC LIMIT 200"
                    ).ok();
                    if let Some(ref mut s) = stmt {
                        let rows = s.query_map([], |row| {
                            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?,
                                row.get::<_, i64>(2)?, row.get::<_, i64>(3)?))
                        }).ok();
                        if let Some(rows) = rows {
                            for row in rows.flatten() {
                                let (addr, body, date, typ) = row;
                                let a = addr.unwrap_or_else(|| "unknown".into());
                                let b = body.unwrap_or_else(|| "(empty)".into());
                                let t = match typ { 1 => "IN", 2 => "OUT", 3 => "DRAFT", _ => "?" };
                                report.push_str(&format!("  [{}] {} {} | {}\n", humantime_format((date / 1000) as u64), t, a, b));
                            }
                        }
                    }
                }
            }
            Err(e) => { report.push_str(&format!("DB error: {}\n", e)); }
        }
        Ok(report)
    })?
}

/// #22 Call Log Forensics â€” extract and parse calls.db
#[tauri::command]
fn extract_call_logs(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Call Log Forensics â•â•â•\n\n");

        // Query via content provider
        if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri",
            "content://call_log/calls", "--projection", "number:date:duration:type:name:cached_name:deleted"]) {
            let text = String::from_utf8_lossy(&out);
            let mut total = 0;
            let mut deleted = 0;
            let mut incoming = 0;
            let mut outgoing = 0;
            let mut missed = 0;

            for line in text.lines() {
                let l = line.trim();
                if l.contains("Row") {
                    total += 1;
                    let is_deleted = l.contains("deleted=1");
                    let number = l.split("number=").nth(1).and_then(|s| s.split(", date=").next()).unwrap_or("?");
                    let date = l.split("date=").nth(1).and_then(|s| s.split(", duration=").next()).unwrap_or("?");
                    let duration = l.split("duration=").nth(1).and_then(|s| s.split(", type=").next()).unwrap_or("?");
                    let call_type = l.split("type=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("?");
                    let name = l.split("cached_name=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("");

                    let type_name = match call_type {
                        "1" => { incoming += 1; "INCOMING" }
                        "2" => { outgoing += 1; "OUTGOING" }
                        "3" => { missed += 1; "MISSED" }
                        "4" => "VOICEMAIL",
                        "5" => "REJECTED",
                        "6" => "BLOCKED",
                        _ => call_type,
                    };

                    let dur_str = if let Ok(s) = duration.parse::<u64>() {
                        format!("{}:{:02}", s / 60, s % 60)
                    } else { "?".into() };

                    let date_str = if let Ok(ms) = date.parse::<u64>() { humantime_format(ms / 1000) } else { date.to_string() };

                    if is_deleted {
                        deleted += 1;
                        report.push_str(&format!("  [DELETED] {} | {} | {} | {}s | {}\n", date_str, type_name, number, dur_str, name));
                    }
                }
            }
            report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
            report.push_str(&format!("  Total calls: {}\n", total));
            report.push_str(&format!("  Incoming: {} | Outgoing: {} | Missed: {}\n", incoming, outgoing, missed));
            report.push_str(&format!("  Deleted: {}\n", deleted));
        }
        Ok(report)
    })?
}

/// #23 WiFi Connection History â€” every network ever connected
#[tauri::command]
fn extract_wifi_history(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• WiFi Connection History â•â•â•\n\n");

        // Saved networks from wpa_supplicant
        report.push_str("â”€â”€ Saved WiFi Networks â”€â”€\n");
        let wifi_paths = [
            "/data/misc/wifi/WifiConfigStore.xml",
            "/data/misc/apexdata/com.android.wifi/WifiConfigStore.xml",
            "/data/misc/wifi/WifiConfigStoreSoftAp.xml",
        ];
        for path in &wifi_paths {
            if let Ok(out) = adb.shell(&serial, &["cat", path]) {
                let text = String::from_utf8_lossy(&out);
                // Parse XML for SSID entries
                let mut in_ssid = false;
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("<string>") && !l.contains("<string/>") {
                        let ssid = l.replace("<string>", "").replace("</string>", "").trim().to_string();
                        if !ssid.is_empty() && ssid.len() > 1 && !ssid.contains("com.android") {
                            report.push_str(&format!("  {}\n", ssid));
                            in_ssid = true;
                        }
                    }
                }
                if !in_ssid {
                    // Fallback: look for WifiConfigStore entries
                    for line in text.lines() {
                        if line.contains("SSID=") || line.contains("\"") {
                            let l = line.trim();
                            if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
                        }
                    }
                }
                break; // Only read first found file
            }
        }

        // WiFi connection history via dumpsys
        report.push_str("\nâ”€â”€ Recent WiFi Connections â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "wifi", "|", "grep", "-E", "SSID|mWifiInfo|state="]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines().take(20) {
                let l = line.trim();
                if l.contains("SSID:") {
                    let ssid = l.split("SSID:").nth(1).and_then(|s| s.split(',').next()).unwrap_or("?").trim();
                    report.push_str(&format!("  Current: {}\n", ssid));
                }
                if l.contains("state=") {
                    let state = l.split("state=").nth(1).unwrap_or("?").trim();
                    report.push_str(&format!("  State: {}\n", state));
                }
                count += 1;
            }
            if count == 0 { report.push_str("  No WiFi data available\n"); }
        }

        // WiFi credentials (requires root)
        report.push_str("\nâ”€â”€ WiFi Credentials â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/data/misc/wifi/WifiConfigStore.xml"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_creds = false;
            // Parse preKey and key entries for passwords
            for line in text.lines() {
                let l = line.trim();
                if l.contains("<string>") && !l.contains("<string/>") {
                    let val = l.replace("<string>", "").replace("</string>", "").trim().to_string();
                    if !val.is_empty() && val.len() > 2 {
                        report.push_str(&format!("  {}\n", val));
                        found_creds = true;
                    }
                }
            }
            if !found_creds { report.push_str("  No credentials accessible\n"); }
        } else {
            report.push_str("  Requires root access\n");
        }

        Ok(report)
    })?
}

/// #24 Bluetooth Device Forensics â€” paired devices, MAC addresses
#[tauri::command]
fn extract_bluetooth_devices(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Bluetooth Device Forensics â•â•â•\n\n");

        // Bluetooth adapter info
        report.push_str("â”€â”€ Bluetooth Adapter â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "bluetooth_manager"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("mAddress=") || l.contains("mName=") || l.contains("State:") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Paired/bonded devices
        report.push_str("\nâ”€â”€ Paired Devices â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "bluetooth_manager", "|", "grep", "-A", "3", "BondedDevice"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("BondedDevice") || l.contains("address=") || l.contains("name=") {
                    report.push_str(&format!("  {}\n", l));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No bonded devices found\n"); }
        }

        // Bluetooth connection history
        report.push_str("\nâ”€â”€ Recent Connections â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "bluetooth_manager", "|", "grep", "-E", "connected|disconnected|A2dpService|connectionState"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines().take(15) {
                let l = line.trim();
                if !l.is_empty() {
                    report.push_str(&format!("  {}\n", l));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No connection history\n"); }
        }

        Ok(report)
    })?
}

/// #25 Social Media Extraction â€” Instagram, Telegram, Signal, Snapchat
#[tauri::command]
fn extract_social_media(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Social Media Deep Extraction ═══\n\n");
        let extract_dir = std::path::Path::new(&output_dir).join("social_media");
        let _ = std::fs::create_dir_all(&extract_dir);

        // Helper: try to pull a database via run-as or su
        let try_pull_db = |adb: &adb::AdbClient, serial: &str, pkg: &str, db_path: &str, dest: &std::path::Path| -> bool {
            // Method 1: run-as (debuggable apps)
            if let Ok(out) = adb.shell(serial, &["sh", "-c", &format!("run-as {} cat '{}' 2>/dev/null | wc -c", pkg, db_path)]) {
                let size = String::from_utf8_lossy(&out).trim().to_string();
                if let Ok(s) = size.parse::<u64>() {
                    if s > 100 {
                        if let Ok(data) = adb.shell(serial, &["sh", "-c", &format!("run-as {} cat '{}'", pkg, db_path)]) {
                            let _ = std::fs::write(dest, &data);
                            return true;
                        }
                    }
                }
            }
            // Method 2: su -c cat
            if let Ok(out) = adb.shell(serial, &["sh", "-c", &format!("su -c 'cat \"{}\"' 2>/dev/null | wc -c", db_path)]) {
                let size = String::from_utf8_lossy(&out).trim().to_string();
                if let Ok(s) = size.parse::<u64>() {
                    if s > 100 {
                        if let Ok(data) = adb.shell(serial, &["sh", "-c", &format!("su -c 'cat \"{}\"'", db_path)]) {
                            let _ = std::fs::write(dest, &data);
                            return true;
                        }
                    }
                }
            }
            false
        };

        // Helper: get app info
        let get_app_info = |adb: &adb::AdbClient, serial: &str, pkg: &str| -> (String, String, String) {
            let mut version = String::new();
            let mut install_date = String::new();
            let mut data_size = String::new();
            if let Ok(out) = adb.shell(serial, &["sh", "-c", &format!("dumpsys package {} | grep -E 'versionName|firstInstallTime|lastUpdateTime|dataDir'", pkg)]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains("versionName=") { version = l.split("versionName=").nth(1).unwrap_or("").trim().to_string(); }
                    if l.contains("firstInstallTime=") { install_date = l.split('=').nth(1).unwrap_or("").trim().to_string(); }
                    if l.contains("dataDir=") { data_size = l.split('=').nth(1).unwrap_or("").trim().to_string(); }
                }
            }
            (version, install_date, data_size)
        };

        // ═══ Instagram ═══
        report.push_str("── Instagram ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "com.instagram.android");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        // Try to extract Direct messages database
        let ig_direct = extract_dir.join("ig_direct.db");
        if try_pull_db(&adb, &serial, "com.instagram.android", "/data/data/com.instagram.android/databases/direct.db", &ig_direct) {
            report.push_str("  [EXTRACTED] direct.db — Direct messages database\n");
            // Parse the database
            if let Ok(conn) = rusqlite::Connection::open(&ig_direct) {
                // List tables
                if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
                    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).ok()
                        .map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default();
                    report.push_str(&format!("  Tables: {}\n", tables.join(", ")));
                }
                // Try to extract message threads
                for table in &["threads", "direct_threads", "thread"] {
                    if let Ok(mut stmt) = conn.prepare(&format!("SELECT COUNT(*) FROM {} LIMIT 1", table)) {
                        if let Ok(count) = stmt.query_row([], |row| row.get::<_, i64>(0)) {
                            report.push_str(&format!("  {} rows in {}\n", count, table));
                        }
                    }
                }
            }
        } else {
            report.push_str("  direct.db — not accessible (needs root or debuggable app)\n");
        }
        // Check for cached media
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard/Android/media/com.instagram.android -type f 2>/dev/null | head -20"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                let count = text.lines().count();
                report.push_str(&format!("  Cached media files: {} found\n", count));
                for line in text.lines().take(5) {
                    report.push_str(&format!("    {}\n", line.rsplit('/').next().unwrap_or(line)));
                }
            }
        }

        // ═══ Telegram ═══
        report.push_str("\n── Telegram ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "org.telegram.messenger");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        // Try to extract cache4.db (message cache)
        let tg_cache = extract_dir.join("tg_cache4.db");
        if try_pull_db(&adb, &serial, "org.telegram.messenger", "/data/data/org.telegram.messenger/databases/cache4.db", &tg_cache) {
            report.push_str("  [EXTRACTED] cache4.db — message cache\n");
            if let Ok(conn) = rusqlite::Connection::open(&tg_cache) {
                if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
                    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).ok()
                        .map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default();
                    report.push_str(&format!("  Tables: {}\n", tables.join(", ")));
                }
                // Count messages in key tables
                for table in &["messages", "chats", "users", "enc_chats"] {
                    if let Ok(mut stmt) = conn.prepare(&format!("SELECT COUNT(*) FROM {} LIMIT 1", table)) {
                        if let Ok(count) = stmt.query_row([], |row| row.get::<_, i64>(0)) {
                            report.push_str(&format!("  {} rows in {}\n", count, table));
                        }
                    }
                }
            }
        } else {
            report.push_str("  cache4.db — not accessible\n");
        }
        // Telegram backups on sdcard
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -path '*Telegram*' \\( -name '*.tg' -o -name '*.atx' \\) 2>/dev/null | head -10"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                report.push_str("  Cloud backups found:\n");
                for line in text.lines() {
                    let name = line.rsplit('/').next().unwrap_or(line);
                    if let Ok(out2) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", line)]) {
                        let info = String::from_utf8_lossy(&out2).trim().to_string();
                        report.push_str(&format!("    {} ({})\n", name, info.split_whitespace().nth(4).unwrap_or("?")));
                    } else {
                        report.push_str(&format!("    {}\n", name));
                    }
                }
            }
        }

        // ═══ Signal ═══
        report.push_str("\n── Signal ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "org.thoughtcrime.securesms");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        let signal_db = extract_dir.join("signal_messages.db");
        if try_pull_db(&adb, &serial, "org.thoughtcrime.securesms", "/data/data/org.thoughtcrime.securesms/databases/messages.db", &signal_db) {
            report.push_str("  [EXTRACTED] messages.db\n");
            if let Ok(conn) = rusqlite::Connection::open(&signal_db) {
                if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
                    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).ok()
                        .map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default();
                    report.push_str(&format!("  Tables: {}\n", tables.join(", ")));
                }
                for table in &["message", "thread", "recipient", "identity_key"] {
                    if let Ok(mut stmt) = conn.prepare(&format!("SELECT COUNT(*) FROM {} LIMIT 1", table)) {
                        if let Ok(count) = stmt.query_row([], |row| row.get::<_, i64>(0)) {
                            report.push_str(&format!("  {} rows in {}\n", count, table));
                        }
                    }
                }
            }
        } else {
            report.push_str("  messages.db — not accessible\n");
        }
        // Signal backups
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -path '*Signal*' -name '*.backup' 2>/dev/null | head -5"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                report.push_str("  Backups:\n");
                for line in text.lines() { report.push_str(&format!("    {}\n", line.rsplit('/').next().unwrap_or(line))); }
            }
        }

        // ═══ Snapchat ═══
        report.push_str("\n── Snapchat ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "com.snapchat.android");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        let snap_db = extract_dir.join("snap_arroyo.db");
        if try_pull_db(&adb, &serial, "com.snapchat.android", "/data/data/com.snapchat.android/databases/arroyo.db", &snap_db) {
            report.push_str("  [EXTRACTED] arroyo.db — chat database\n");
            if let Ok(conn) = rusqlite::Connection::open(&snap_db) {
                if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
                    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).ok()
                        .map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default();
                    report.push_str(&format!("  Tables: {}\n", tables.join(", ")));
                }
            }
        } else {
            report.push_str("  arroyo.db — not accessible\n");
        }
        // Snapchat memories on sdcard
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -path '*Snapchat*' -type f 2>/dev/null | head -10"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                let count = text.lines().count();
                report.push_str(&format!("  Snapchat files on sdcard: {}\n", count));
            }
        }

        // ═══ Facebook Messenger ═══
        report.push_str("\n── Facebook Messenger ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "com.facebook.orca");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        let orca_db = extract_dir.join("orca_threads.db");
        if try_pull_db(&adb, &serial, "com.facebook.orca", "/data/data/com.facebook.orca/databases/threads_db2", &orca_db) {
            report.push_str("  [EXTRACTED] threads_db2 — Messenger database\n");
            if let Ok(conn) = rusqlite::Connection::open(&orca_db) {
                if let Ok(mut stmt) = conn.prepare("SELECT name FROM sqlite_master WHERE type='table'") {
                    let tables: Vec<String> = stmt.query_map([], |row| row.get(0)).ok()
                        .map(|iter| iter.filter_map(|r| r.ok()).collect()).unwrap_or_default();
                    report.push_str(&format!("  Tables: {}\n", tables.join(", ")));
                }
                for table in &["messages", "threads", "users"] {
                    if let Ok(mut stmt) = conn.prepare(&format!("SELECT COUNT(*) FROM {} LIMIT 1", table)) {
                        if let Ok(count) = stmt.query_row([], |row| row.get::<_, i64>(0)) {
                            report.push_str(&format!("  {} rows in {}\n", count, table));
                        }
                    }
                }
            }
        } else {
            report.push_str("  threads_db2 — not accessible\n");
        }

        // ═══ WeChat ═══
        report.push_str("\n── WeChat ──\n");
        let (ver, installed, _) = get_app_info(&adb, &serial, "com.tencent.mm");
        if !ver.is_empty() {
            report.push_str(&format!("  Version: {}\n", ver));
            report.push_str(&format!("  Installed: {}\n", installed));
        } else {
            report.push_str("  Not installed\n");
        }
        // WeChat stores databases per-user in MicroMsg/<hash>/
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard/tencent/MicroMsg -name '*.db' 2>/dev/null | head -15"]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() {
                report.push_str("  WeChat databases found:\n");
                for line in text.lines().take(10) {
                    let name = line.rsplit('/').next().unwrap_or(line);
                    report.push_str(&format!("    {}\n", name));
                }
            }
        }

        // ═══ Summary ═══
        report.push_str("\n── Extraction Summary ──\n");
        if let Ok(entries) = std::fs::read_dir(&extract_dir) {
            let db_count = entries.filter_map(|e| e.ok()).filter(|e| e.path().extension().map(|ext| ext == "db").unwrap_or(false)).count();
            report.push_str(&format!("  Databases extracted: {}\n", db_count));
            report.push_str(&format!("  Output directory: {}\n", extract_dir.display()));
        }

        Ok(report)
    })?
}

/// #26 Deleted Photo Recovery â€” thumbnail carving
#[tauri::command]
fn recover_deleted_photos(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Deleted Photo Recovery â•â•â•\n\n");

        // Scan .thumbnails folder
        report.push_str("â”€â”€ Thumbnail Cache (.thumbnails) â”€â”€\n");
        let thumb_paths = [
            "/sdcard/DCIM/.thumbnails",
            "/sdcard/DCIM/Camera/.thumbnails",
        ];
        let mut total_thumbs = 0;
        for path in &thumb_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null | head -50", path)]) {
                let text = String::from_utf8_lossy(&out);
                for line in text.lines() {
                    let l = line.trim();
                    if l.contains(".thumbdata") || l.contains(".jpg") || l.contains(".dat") {
                        let parts: Vec<&str> = l.split_whitespace().collect();
                        if parts.len() >= 5 {
                            let name = parts.last().unwrap_or(&"?");
                            let size = parts[4];
                            report.push_str(&format!("  {} ({} bytes)\n", name, size));
                            total_thumbs += 1;
                        }
                    }
                }
            }
        }
        report.push_str(&format!("  Total thumbnails: {}\n", total_thumbs));

        // Check for orphaned photos (in DCIM but not in MediaStore)
        report.push_str("\nâ”€â”€ DCIM Photo Count â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard/DCIM -name '*.jpg' -o -name '*.jpeg' -o -name '*.png' 2>/dev/null | wc -l"]) {
            report.push_str(&format!("  DCIM photos: {}\n", String::from_utf8_lossy(&out).trim()));
        }

        // Check for recently deleted folder
        report.push_str("\nâ”€â”€ Recently Deleted Folders â”€â”€\n");
        let deleted_paths = [
            "/sdcard/DCIM/.trash",
            "/sdcard/DCIM/.recently_deleted",
            "/sdcard/.trash",
        ];
        for path in &deleted_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null | head -20", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() {
                    report.push_str(&format!("  Found: {}\n", path));
                    for line in text.lines() {
                        if !line.starts_with("total") && !line.trim().is_empty() {
                            let parts: Vec<&str> = line.trim().split_whitespace().collect();
                            if parts.len() >= 5 {
                                report.push_str(&format!("    {} ({} bytes)\n", parts.last().unwrap_or(&"?"), parts[4]));
                            }
                        }
                    }
                }
            }
        }

        report.push_str(&format!("\nOutput directory: {}\n", output_dir));
        Ok(report)
    })?
}

/// #27 Keyboard Dictionary Recovery
#[tauri::command]
fn extract_keyboard_dictionary(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Keyboard Dictionary Recovery â•â•â•\n\n");

        // Gboard user dictionary
        report.push_str("â”€â”€ Gboard User Dictionary â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri",
            "content://com.google.android.providers.gsf.partnersettings",
            "--projection", "name:value"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                if line.contains("Row") { count += 1; }
            }
            report.push_str(&format!("  Settings entries: {}\n", count));
        }

        // User dictionary provider
        if let Ok(out) = adb.shell(&serial, &["content", "query", "--uri",
            "content://user_dictionary/words", "--projection", "word:locale:frequency"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Row") {
                    let word = l.split("word=").nth(1).and_then(|s| s.split(", locale=").next()).unwrap_or("?");
                    let freq = l.split("frequency=").nth(1).and_then(|s| s.split(',').next()).unwrap_or("?");
                    report.push_str(&format!("  {} (freq: {})\n", word, freq));
                    count += 1;
                    if count >= 50 { break; }
                }
            }
            if count == 0 { report.push_str("  No user dictionary entries\n"); }
        }

        // Gboard personal dictionary DB
        report.push_str("\nâ”€â”€ Gboard Personal Dictionary â”€â”€\n");
        let gboard_paths = [
            "/data/data/com.google.android.inputmethod.latin/databases/user_dictionary.db",
            "/data/data/com.google.android.inputmethod.latin/databases/userdict.db",
        ];
        for path in &gboard_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() { report.push_str(&format!("  Found: {}\n", path.rsplit('/').next().unwrap_or(path))); }
            }
        }

        // Keyboard type
        report.push_str("\nâ”€â”€ Active Keyboard â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "default_input_method"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Input method: {}\n", v));
        }

        Ok(report)
    })?
}

/// #28 Clipboard Forensics â€” deep clipboard analysis
#[tauri::command]
fn extract_clipboard_forensics(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Clipboard Forensics â•â•â•\n\n");

        // Current clipboard via service call
        report.push_str("â”€â”€ Current Clipboard â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["service", "call", "clipboard", "2"]) {
            let raw = String::from_utf8_lossy(&out);
            // Decode hex
            let mut decoded = String::new();
            for word in raw.split_whitespace() {
                if word.len() == 8 && word.chars().all(|c| c.is_ascii_hexdigit()) {
                    for i in (0..8).step_by(2) {
                        if let Ok(byte) = u8::from_str_radix(&word[i..i+2], 16) {
                            if byte >= 0x20 && byte < 0x7f { decoded.push(byte as char); }
                        }
                    }
                }
            }
            let text: String = decoded.chars().filter(|c| !c.is_control() || *c == '\n').collect();
            if !text.trim().is_empty() {
                report.push_str(&format!("  Content: {}\n", text.trim()));
            } else {
                report.push_str("  Clipboard empty\n");
            }
        }

        // Clipper app clipboard history
        report.push_str("\nâ”€â”€ Clipper History â”€â”€\n");
        let clipper_db = "/sdcard/clipper/clipboard_history.db";
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", clipper_db)]) {
            let text = String::from_utf8_lossy(&out).trim().to_string();
            if !text.is_empty() { report.push_str("  Clipper app history DB found\n"); }
        } else {
            report.push_str("  No Clipper app installed\n");
        }

        // Clipboard service state
        report.push_str("\nâ”€â”€ Clipboard Service â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "clipboard"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("primary") || l.contains("clip") || l.contains("text") || l.contains("label") {
                    if !l.is_empty() && l.len() < 200 {
                        report.push_str(&format!("  {}\n", l));
                    }
                }
            }
        }

        Ok(report)
    })?
}

/// #29 USB Connection Forensics
#[tauri::command]
fn extract_usb_connections(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• USB Connection Forensics â•â•â•\n\n");

        // USB device manager state
        report.push_str("â”€â”€ USB Device Manager â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "usb"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("mCurrentUsbFunction") || l.contains("UsbDevice") || l.contains("mConnected") || l.contains("vendor") || l.contains("product") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // USB configuration
        report.push_str("\nâ”€â”€ USB Configuration â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "sys.usb.config"]) {
            report.push_str(&format!("  Current config: {}\n", String::from_utf8_lossy(&out).trim()));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "sys.usb.state"]) {
            report.push_str(&format!("  USB state: {}\n", String::from_utf8_lossy(&out).trim()));
        }

        // USB host connections (OTG)
        report.push_str("\nâ”€â”€ USB Host Devices â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "usb", "|", "grep", "-E", "device|connected|vendor|product"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines().take(15) {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); count += 1; }
            }
            if count == 0 { report.push_str("  No USB host devices detected\n"); }
        }

        // ADB connection info
        report.push_str("\nâ”€â”€ ADB Connection â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "service.adb.tcp.port"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  ADB TCP port: {}\n", if v.is_empty() { "USB only" } else { &v }));
        }

        Ok(report)
    })?
}

/// #30 Power/Battery Forensics
#[tauri::command]
fn extract_power_forensics(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Power/Battery Forensics ═══\n\n");

        // Battery hardware info — structured parsing
        report.push_str("── Battery Hardware ──\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "battery"]) {
            let text = String::from_utf8_lossy(&out);
            let mut level = String::new();
            let mut status = String::new();
            let mut health = String::new();
            let mut temp = String::new();
            let mut voltage = String::new();
            let mut technology = String::new();
            let mut ac = false; let mut usb = false; let mut wireless = false;

            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("level:") { level = l.split(':').nth(1).unwrap_or("").trim().to_string(); }
                if l.starts_with("status:") {
                    status = match l.split(':').nth(1).unwrap_or("").trim() {
                        "2" => "Charging", "3" => "Discharging", "4" => "Not charging", "5" => "Full", _ => "Unknown"
                    }.to_string();
                }
                if l.starts_with("health:") {
                    health = match l.split(':').nth(1).unwrap_or("").trim() {
                        "2" => "Good", "3" => "Overheat", "4" => "Dead", "5" => "Over voltage", "6" => "Failure", "7" => "Cold", _ => "Unknown"
                    }.to_string();
                }
                if l.starts_with("temperature:") {
                    if let Ok(t) = l.split(':').nth(1).unwrap_or("").trim().parse::<f64>() {
                        temp = format!("{:.1}°C", t / 10.0);
                    }
                }
                if l.starts_with("voltage:") {
                    if let Ok(v) = l.split(':').nth(1).unwrap_or("").trim().parse::<f64>() {
                        voltage = format!("{:.2}V", v / 1000.0);
                    }
                }
                if l.starts_with("technology:") { technology = l.split(':').nth(1).unwrap_or("").trim().to_string(); }
                if l.contains("AC powered:") && l.contains("true") { ac = true; }
                if l.contains("USB powered:") && l.contains("true") { usb = true; }
                if l.contains("Wireless powered:") && l.contains("true") { wireless = true; }
            }
            let source = if ac { "AC" } else if usb { "USB" } else if wireless { "Wireless" } else { "Battery" };
            report.push_str(&format!("  Level: {}% | Status: {} | Source: {}\n", level, status, source));
            report.push_str(&format!("  Health: {} | Temp: {} | Voltage: {}\n", health, temp, voltage));
            if !technology.is_empty() { report.push_str(&format!("  Technology: {}\n", technology)); }
        }

        // Discharge statistics
        report.push_str("\n── Discharge Statistics ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --daily 2>/dev/null | head -40"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("Discharge") || l.contains("Screen on") || l.contains("Screen off") || l.contains("Time on battery") || l.contains("Drain") || l.contains("Capacity") || l.contains("Start clock") || l.contains("End clock")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Estimated power use per app
        report.push_str("\n── Per-App Power Consumption ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --proto bin 2>/dev/null | head -5 || dumpsys batterystats | grep -B1 -A3 'Estimated power' | head -40"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(30) {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
            }
        }

        // Wake lock analysis — detailed
        report.push_str("\n── Wake Lock Analysis ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --wake-locks 2>/dev/null | head -50"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains(':') || l.contains("Wake lock") || l.contains("uid=") || l.contains("Running")) {
                    report.push_str(&format!("  {}\n", l));
                    count += 1;
                    if count >= 25 { break; }
                }
            }
            if count == 0 { report.push_str("  No wake lock data available\n"); }
        }

        // Doze mode analysis
        report.push_str("\n── Doze Mode Analysis ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys deviceidle 2>/dev/null"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("mState") || l.contains("Screen") || l.contains("Charging") || l.contains("Motion") || l.contains("Idle") || l.contains("Deep") || l.contains("Light") || l.contains("mActive") || l.contains("mInactive")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Job scheduler analysis
        report.push_str("\n── Job Scheduler (Deferred Work) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys jobscheduler | grep -E 'Job #' | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Job #") {
                    report.push_str(&format!("  {}\n", l));
                    count += 1;
                    if count >= 15 { break; }
                }
            }
            if count == 0 { report.push_str("  No scheduled jobs\n"); }
        }

        // Screen on/off event timeline
        report.push_str("\n── Screen Event Timeline ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats --history 2>/dev/null | grep -E 'screen_on|screen_off|interactive' | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut screen_on_count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("screen_on") || l.contains("+screen_on") { screen_on_count += 1; }
                if !l.is_empty() && (l.contains("screen") || l.contains("interactive")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
            if screen_on_count > 0 {
                report.push_str(&format!("  Screen unlock events: {}\n", screen_on_count));
            }
        }

        // App standby buckets
        report.push_str("\n── App Standby Buckets ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats | grep -E 'standby|bucket' | head -15"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(10) {
                let l = line.trim();
                if !l.is_empty() { report.push_str(&format!("  {}\n", l)); }
            }
        }

        // Battery saver
        report.push_str("\n── Battery Saver ──\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "low_power"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Status: {}\n", if v == "1" { "ON" } else { "OFF" }));
        }

        Ok(report)
    })?
}

/// #31 Notification History
#[tauri::command]
fn extract_notification_history(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Notification Forensics ═══\n\n");

        // Full notification dump with --noredact to get actual content
        report.push_str("── Active Notifications (Full Content) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys notification --noredact 2>/dev/null || dumpsys notification"]) {
            let text = String::from_utf8_lossy(&out);
            let mut in_record = false;
            let mut current_pkg = String::new();
            let mut current_tag = String::new();
            let mut current_id = String::new();
            let mut current_channel = String::new();
            let mut current_priority = String::new();
            let mut current_time = String::new();
            let mut current_title = String::new();
            let mut current_text = String::new();
            let mut count = 0u32;

            let flush_record = |report: &mut String, pkg: &str, tag: &str, id: &str, channel: &str, priority: &str, time: &str, title: &str, text: &str| {
                if pkg.is_empty() || pkg.contains("com.android.systemui") { return; }
                report.push_str(&format!("  [{}] {} (id:{})\n", pkg, if !title.is_empty() { title } else { tag }, id));
                if !text.is_empty() {
                    report.push_str(&format!("    Content: {}\n", text));
                }
                if !channel.is_empty() {
                    report.push_str(&format!("    Channel: {}\n", channel));
                }
                let mut meta = Vec::new();
                if !priority.is_empty() { meta.push(format!("priority:{}", priority)); }
                if !time.is_empty() { meta.push(format!("time:{}", time)); }
                if !meta.is_empty() {
                    report.push_str(&format!("    {}\n", meta.join(", ")));
                }
            };

            for line in text.lines() {
                let l = line.trim();

                // NotificationRecord header
                if l.contains("NotificationRecord(") {
                    // Flush previous record
                    if in_record {
                        flush_record(&mut report, &current_pkg, &current_tag, &current_id, &current_channel, &current_priority, &current_time, &current_title, &current_text);
                        count += 1;
                    }
                    in_record = true;
                    current_pkg.clear(); current_tag.clear(); current_id.clear();
                    current_channel.clear(); current_priority.clear(); current_time.clear();
                    current_title.clear(); current_text.clear();
                    // Parse pkg from record header
                    if let Some(p) = l.split("pkg=").nth(1) {
                        current_pkg = p.split(|c: char| c == ' ' || c == ',').next().unwrap_or("").to_string();
                    }
                }

                if !in_record { continue; }

                // Extract fields from notification data
                if l.starts_with("pkg=") || (l.contains("pkg=") && current_pkg.is_empty()) {
                    current_pkg = l.split("pkg=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or(&current_pkg).to_string();
                }
                if l.contains("tag=") && current_tag.is_empty() {
                    current_tag = l.split("tag=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                if l.contains("id=") && current_id.is_empty() {
                    current_id = l.split("id=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                if l.contains("channelId=") {
                    current_channel = l.split("channelId=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                if l.contains("priority=") {
                    current_priority = l.split("priority=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                if l.contains("time=") {
                    current_time = l.split("time=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                // Extract notification title/text from extras
                if l.contains("android.title=") {
                    current_title = l.split("android.title=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                if l.contains("android.text=") {
                    current_text = l.split("android.text=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }
                // Also try BigText
                if l.contains("android.bigText=") && current_text.is_empty() {
                    current_text = l.split("android.bigText=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("").to_string();
                }

                if count >= 50 { break; }
            }
            // Flush last record
            if in_record {
                flush_record(&mut report, &current_pkg, &current_tag, &current_id, &current_channel, &current_priority, &current_time, &current_title, &current_text);
                count += 1;
            }
            if count == 0 { report.push_str("  No active notifications\n"); }
            else { report.push_str(&format!("\n  Total active: {}\n", count)); }
        }

        // Notification channels per app
        report.push_str("\n── Notification Channels ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys notification | grep -E 'Channel|channelId|importance' | head -40"]) {
            let text = String::from_utf8_lossy(&out);
            let mut channel_count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Channel{") || l.contains("channelId=") {
                    report.push_str(&format!("  {}\n", l));
                    channel_count += 1;
                    if channel_count >= 20 { break; }
                }
            }
            if channel_count == 0 { report.push_str("  No channel data available\n"); }
        }

        // Notification policy (DND)
        report.push_str("\n── Notification Policy ──\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "zen_mode"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            let zen = match v.as_str() {
                "0" => "Off".to_string(),
                "1" => "Priority only".to_string(),
                "2" => "Total silence".to_string(),
                "3" => "Alarms only".to_string(),
                _ => v
            };
            report.push_str(&format!("  Do Not Disturb: {}\n", zen));
        }
        // Notification policy details
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys notification | grep -A 5 'Zen mode\\|ZenConfig\\|policy' | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(10) {
                let l = line.trim();
                if !l.is_empty() && (l.contains("Zen") || l.contains("policy") || l.contains("allow") || l.contains("suppress")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Notification access apps
        report.push_str("\n── Apps with Notification Access ──\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "secure", "enabled_notification_listeners"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            if !v.is_empty() && v != "null" {
                for listener in v.split(':') {
                    let pkg = listener.split('/').next().unwrap_or(listener);
                    report.push_str(&format!("  {}\n", pkg));
                }
            } else {
                report.push_str("  No apps with notification access\n");
            }
        }

        // Notification log database (historical)
        report.push_str("\n── Historical Notification Log ──\n");
        let log_paths = [
            "/data/system/notification_log.db",
            "/data/system/notification_history.db",
        ];
        for path in &log_paths {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() {
                    let fname = path.rsplit('/').next().unwrap_or(path);
                    report.push_str(&format!("  {} exists: {}\n", fname, text.lines().next().unwrap_or("")));
                    report.push_str("  (requires root to extract historical notification database)\n");
                }
            }
        }

        // Per-app notification counts from usage stats
        report.push_str("\n── Per-App Notification Counts ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys notification | grep 'pkg=' | sed 's/.*pkg=//;s/ .*//' | sort | uniq -c | sort -rn | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && l.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    let parts: Vec<&str> = l.splitn(2, ' ').collect();
                    if parts.len() == 2 {
                        report.push_str(&format!("  {:>4} notifications  {}\n", parts[0], parts[1]));
                        found = true;
                    }
                }
            }
            if !found { report.push_str("  No per-app data available\n"); }
        }

        Ok(report)
    })?
}

/// #32 Screen Time / Digital Wellbeing
#[tauri::command]
fn extract_screen_time(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Screen Time / Digital Wellbeing â•â•â•\n\n");

        // Usage stats â€” top apps by foreground time
        report.push_str("â”€â”€ App Usage (Foreground Time) â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "usagestats"]) {
            let text = String::from_utf8_lossy(&out);
            let mut apps: Vec<(String, u64)> = Vec::new();
            for line in text.lines() {
                if line.contains("package=") && line.contains("totalTimeInForeground=") {
                    let pkg = line.split("package=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("?").to_string();
                    let fg = line.split("totalTimeInForeground=").nth(1).and_then(|s| s.split(|c: char| c == ' ' || c == ',').next()).unwrap_or("0");
                    if let Ok(ms) = fg.parse::<u64>() {
                        apps.push((pkg, ms));
                    }
                }
            }
            apps.sort_by(|a, b| b.1.cmp(&a.1));
            let mut total_fg: u64 = 0;
            for (pkg, ms) in apps.iter().take(20) {
                total_fg += ms;
                let hrs = ms / 3600000;
                let mins = (ms % 3600000) / 60000;
                report.push_str(&format!("  {} | {}h {}m\n", pkg, hrs, mins));
            }
            let total_hrs = total_fg / 3600000;
            let total_mins = (total_fg % 3600000) / 60000;
            report.push_str(&format!("\n  Total screen time: {}h {}m\n", total_hrs, total_mins));
        }

        // Screen unlock count
        report.push_str("\nâ”€â”€ Unlock Events â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "batterystats", "|", "grep", "screen_on"]) {
            let text = String::from_utf8_lossy(&out);
            let count = text.lines().filter(|l| l.contains("screen_on")).count();
            report.push_str(&format!("  Screen-on events: {}\n", count));
        }

        // App standby buckets
        report.push_str("\nâ”€â”€ App Standby Buckets â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "usagestats", "|", "grep", "standbyBucket"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines().take(15) {
                let l = line.trim();
                if l.contains("standbyBucket") { report.push_str(&format!("  {}\n", l)); count += 1; }
            }
            if count == 0 { report.push_str("  No standby data\n"); }
        }

        Ok(report)
    })?
}

/// #33 SQLite WAL/Journal Carving Engine
#[tauri::command]
fn carve_sqlite_wal(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• SQLite WAL/Journal Carving â•â•â•\n\n");

        // Scan for WAL and journal files
        report.push_str("â”€â”€ WAL Files Found â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -name '*.db-wal' -o -name '*.db-journal' 2>/dev/null | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    let size = if let Ok(s) = adb.shell(&serial, &["stat", "-c", "%s", path]) {
                        String::from_utf8_lossy(&s).trim().to_string()
                    } else { "?".into() };
                    report.push_str(&format!("  {} ({} bytes)\n", name, size));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No WAL/journal files on /sdcard\n"); }
            else { report.push_str(&format!("  Total: {}\n", count)); }
        }

        // Check app-private databases
        report.push_str("\nâ”€â”€ App Database WAL Files â”€â”€\n");
        let app_dbs = [
            ("com.whatsapp", "/data/data/com.whatsapp/databases/msgstore.db"),
            ("com.android.providers.telephony", "/data/data/com.android.providers.telephony/databases/mmssms.db"),
            ("com.android.chrome", "/data/data/com.android.chrome/app_chrome/Default/History"),
        ];
        for (app, db) in &app_dbs {
            let wal = format!("{}-wal", db);
            let journal = format!("{}-journal", db);
            for path in &[&wal, &journal] {
                if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("ls -la '{}' 2>/dev/null", path)]) {
                    let text = String::from_utf8_lossy(&out).trim().to_string();
                    if !text.is_empty() {
                        let parts: Vec<&str> = text.split_whitespace().collect();
                        let size = parts.get(4).unwrap_or(&"?");
                        report.push_str(&format!("  {} â†’ {} ({} bytes)\n", app, path.rsplit('/').next().unwrap_or(path), size));
                    }
                }
            }
        }

        report.push_str(&format!("\nOutput directory: {}\n", output_dir));
        report.push_str("Note: WAL files contain uncommitted transactions â€” may include recently deleted data.\n");
        Ok(report)
    })?
}

/// #34 App Sandbox Extraction via run-as
#[tauri::command]
fn extract_app_sandbox(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• App Sandbox Extraction â•â•â•\n\n");

        // Find debuggable apps
        report.push_str("â”€â”€ Debuggable Apps â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "pm list packages -d 2>/dev/null | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let pkg = line.trim().replace("package:", "");
                if !pkg.is_empty() {
                    // Try run-as
                    if let Ok(test) = adb.shell(&serial, &["run-as", &pkg, "ls", "/data/data/", &pkg]) {
                        if !String::from_utf8_lossy(&test).contains("unknown package") {
                            report.push_str(&format!("  {} (debuggable)\n", pkg));
                            count += 1;
                        }
                    }
                }
            }
            if count == 0 { report.push_str("  No debuggable apps found\n"); }
        }

        // Try to extract SharedPreferences from known apps
        report.push_str("\nâ”€â”€ SharedPreferences Extraction â”€â”€\n");
        let targets = ["com.whatsapp", "com.instagram.android", "com.snapchat.android"];
        for pkg in &targets {
            let prefs_path = format!("/data/data/{}/shared_prefs", pkg);
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("run-as {} ls {}", pkg, prefs_path)]) {
                let text = String::from_utf8_lossy(&out).trim().to_string();
                if !text.is_empty() && !text.contains("Permission denied") {
                    report.push_str(&format!("  {} shared_prefs:\n", pkg));
                    for line in text.lines().take(10) {
                        report.push_str(&format!("    {}\n", line.trim()));
                    }
                }
            }
        }

        Ok(report)
    })?
}

/// #35 Hidden Partition Scanner
#[tauri::command]
fn scan_hidden_partitions(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Hidden Partition Scanner â•â•â•\n\n");

        // All block devices
        report.push_str("â”€â”€ Block Devices â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["ls", "-la", "/dev/block/"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("mmc") || l.contains("sda") || l.contains("sdb") || l.contains("dm-") || l.contains("loop") {
                    let parts: Vec<&str> = l.split_whitespace().collect();
                    if parts.len() >= 5 {
                        let name = parts.last().unwrap_or(&"?");
                        let size = parts[4];
                        report.push_str(&format!("  {} ({} bytes)\n", name, size));
                        count += 1;
                    }
                }
            }
            report.push_str(&format!("  Total: {} devices\n", count));
        }

        // Mount points
        report.push_str("\nâ”€â”€ Mount Points â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/mounts"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 4 {
                    let device = parts[0];
                    let mount = parts[1];
                    let fs_type = parts[2];
                    // Skip system/internal mounts, show interesting ones
                    if mount.starts_with("/sdcard") || mount.starts_with("/storage") || mount.starts_with("/mnt/expand") || mount.contains("fuse") {
                        report.push_str(&format!("  {} â†’ {} ({})\n", device, mount, fs_type));
                    }
                }
            }
        }

        // Multiple users / work profiles
        report.push_str("\nâ”€â”€ Multiple Users â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["pm", "list", "users"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("UserInfo") || l.contains("{") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // Adoptable storage
        report.push_str("\nâ”€â”€ Adoptable Storage â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sm", "list-disks"]) {
            let text = String::from_utf8_lossy(&out);
            if !text.trim().is_empty() {
                for line in text.lines() { report.push_str(&format!("  Disk: {}\n", line.trim())); }
            } else { report.push_str("  No adoptable storage detected\n"); }
        }

        Ok(report)
    })?
}

/// #36 Network Traffic Snapshot
#[tauri::command]
fn extract_network_traffic(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Network Traffic Snapshot â•â•â•\n\n");

        // Per-app data usage from netstats
        report.push_str("â”€â”€ Per-App Data Usage â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "netstats", "|", "grep", "-E", "package|rxBytes|txBytes"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines().take(40) {
                let l = line.trim();
                if l.contains("package=") || l.contains("rxBytes=") {
                    report.push_str(&format!("  {}\n", l));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No per-app network data\n"); }
        }

        // Active network interfaces
        report.push_str("\nâ”€â”€ Network Interfaces â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/tcp"]) {
            let text = String::from_utf8_lossy(&out);
            let connections = text.lines().count().saturating_sub(1); // skip header
            report.push_str(&format!("  Active TCP connections: {}\n", connections));
        }
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/net/udp"]) {
            let text = String::from_utf8_lossy(&out);
            let connections = text.lines().count().saturating_sub(1);
            report.push_str(&format!("  Active UDP connections: {}\n", connections));
        }

        // Data saver mode
        report.push_str("\nâ”€â”€ Data Saver â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["settings", "get", "global", "data_saver_mode"]) {
            let v = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Data saver: {}\n", match v.as_str() { "1" => "Enabled", "0" => "Disabled", _ => &v }));
        }

        Ok(report)
    })?
}

/// #37 Sensor Data Analysis
#[tauri::command]
fn extract_sensor_data(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Sensor Data Analysis ═══\n\n");

        // Full sensor list with details
        report.push_str("── Available Sensors ──\n");
        let mut sensor_count = 0u32;
        let mut active_sensors: Vec<String> = Vec::new();
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys sensorservice 2>/dev/null"]) {
            let text = String::from_utf8_lossy(&out);
            let mut in_sensor_list = false;

            for line in text.lines() {
                let l = line.trim();

                // Detect sensor entries — they contain "Sensor" with a handle number
                if l.contains("Sensor") && (l.contains("handle=") || l.contains("0x")) {
                    in_sensor_list = true;
                }

                if in_sensor_list && l.contains("Sensor") {
                    // Parse sensor name, type, handle
                    let name = if let Some(n) = l.split("Sensor{").nth(1) {
                        n.split('}').next().unwrap_or(l)
                    } else {
                        l
                    };
                    report.push_str(&format!("  {}\n", name));
                    sensor_count += 1;
                    if sensor_count >= 40 { break; }
                }

                // Also match named sensors
                if l.contains("accelerometer") || l.contains("gyroscope") || l.contains("magnetometer") ||
                   l.contains("proximity") || l.contains("light") || l.contains("gravity") ||
                   l.contains("step") || l.contains("heart") || l.contains("pressure") ||
                   l.contains("orientation") || l.contains("rotation") || l.contains("linear acceleration") ||
                   l.contains("significant motion") || l.contains("geomagnetic") {
                    if !l.contains("Sensor") {
                        report.push_str(&format!("  {}\n", l));
                        sensor_count += 1;
                    }
                }
            }
            report.push_str(&format!("\n  Total sensors found: {}\n", sensor_count));

            // Active/registered sensors
            report.push_str("\n── Active Sensor Listeners ──\n");
            let mut active_count = 0u32;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("active") || l.contains("Active") || l.contains("registered") || l.contains("listener") {
                    if l.contains("pid") || l.contains("uid") || l.contains("pkg") || l.contains("handle") || l.contains("Hz") {
                        report.push_str(&format!("  {}\n", l));
                        active_count += 1;
                        if let Some(s) = l.split("Sensor{").nth(1).and_then(|s| s.split('}').next()) {
                            active_sensors.push(s.to_string());
                        }
                        if active_count >= 20 { break; }
                    }
                }
            }
            if active_count == 0 { report.push_str("  No active sensor listeners\n"); }
            else { report.push_str(&format!("  Active listeners: {}\n", active_count)); }
        }

        // Step counter — detailed
        report.push_str("\n── Step Counter ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys sensorservice | grep -i -A2 'step counter\\|step detector'"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines().take(10) {
                let l = line.trim();
                if !l.is_empty() {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No step counter data\n"); }
        }

        // Sensor power consumption
        report.push_str("\n── Sensor Power Consumption ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys batterystats | grep -i -E 'sensor|accelerometer|gyroscope|gps|location' | head -15"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("sensor") || l.contains("Sensor") || l.contains("gps") || l.contains("location")) {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No sensor power data available\n"); }
        }

        // Activity recognition data
        report.push_str("\n── Activity Recognition ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys activity_recognition 2>/dev/null | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("activity") || l.contains("transition") || l.contains("confidence") || l.contains("WALKING") || l.contains("RUNNING") || l.contains("STILL") || l.contains("IN_VEHICLE") || l.contains("ON_BICYCLE")) {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No activity recognition data\n"); }
        }

        // Location sensor usage (which apps use GPS)
        report.push_str("\n── Location Sensor Usage (GPS) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys location | grep -E 'last location|passive|gps|Request' | head -20"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() && (l.contains("Request") || l.contains("passive") || l.contains("gps") || l.contains("location")) {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No location request data\n"); }
        }

        // Significant motion events
        report.push_str("\n── Significant Motion Events ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys sensorservice | grep -i 'significant' | head -5"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No significant motion data\n"); }
        }

        // Sensor calibration data
        report.push_str("\n── Sensor Calibration ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "dumpsys sensorservice | grep -i -E 'calibrat|offset|bias' | head -10"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found = false;
            for line in text.lines() {
                let l = line.trim();
                if !l.is_empty() {
                    report.push_str(&format!("  {}\n", l));
                    found = true;
                }
            }
            if !found { report.push_str("  No calibration data available\n"); }
        }

        Ok(report)
    })?
}

/// #38 Full File System Manifest
#[tauri::command]
fn generate_file_manifest(serial: String, output_dir: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("═══ Full File System Manifest ═══\n\n");

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
        let manifest_path = format!("{}\\manifest_{}.txt", output_dir, timestamp);

        // Expanded directory list with forensic relevance
        let dirs = [
            ("/sdcard/DCIM", "Camera Photos & Videos"),
            ("/sdcard/Download", "Downloads"),
            ("/sdcard/Documents", "Documents"),
            ("/sdcard/Pictures", "Pictures & Screenshots"),
            ("/sdcard/WhatsApp", "WhatsApp Data"),
            ("/sdcard/Recordings", "Voice Recordings"),
            ("/sdcard/Android/data", "App Data (Public)"),
            ("/sdcard/Android/media", "App Media"),
            ("/sdcard/Music", "Music Files"),
            ("/sdcard/Movies", "Movies/Videos"),
            ("/sdcard/Podcasts", "Podcasts"),
            ("/sdcard/Ringtones", "Ringtones"),
            ("/sdcard/Notifications", "Notification Sounds"),
        ];

        let mut total_files = 0u64;
        let mut total_size = 0u64;
        let mut hidden_files = 0u64;
        let fmt_size = |bytes: u64| -> String {
            if bytes > 1_073_741_824 { format!("{:.1} GB", bytes as f64 / 1_073_741_824.0) }
            else if bytes > 1_048_576 { format!("{:.1} MB", bytes as f64 / 1_048_576.0) }
            else if bytes > 1024 { format!("{:.1} KB", bytes as f64 / 1024.0) }
            else { format!("{} B", bytes) }
        };

        for (dir, label) in &dirs {
            report.push_str(&format!("── {} ({}) ──\n", label, dir));
            // Use ls -laR for detailed listing with sizes and timestamps
            // Format: permissions, owner, size, date, time, name
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &format!("find '{}' -type f -exec ls -la {{}} \\; 2>/dev/null | head -200", dir)]) {
                let text = String::from_utf8_lossy(&out);
                let mut dir_files = 0u64;
                let mut dir_size = 0u64;
                let mut largest_file: (String, u64) = (String::new(), 0);
                let mut newest_file: (String, String) = (String::new(), String::new());
                let mut extensions: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

                for line in text.lines() {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() < 9 { continue; }
                    // Parse: -rw-rw---- 1 root sdcard_rw 1234567 2024-01-15 14:30 filename.jpg
                    let size: u64 = parts.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
                    let date = parts.get(5).unwrap_or(&"").to_string();
                    let time_str = parts.get(6).unwrap_or(&"").to_string();
                    let name = parts[8..].join(" ");
                    let filename = name.rsplit('/').next().unwrap_or(&name).to_string();

                    dir_files += 1;
                    dir_size += size;
                    if size > largest_file.1 { largest_file = (filename.clone(), size); }
                    if date > newest_file.1 || newest_file.1.is_empty() { newest_file = (filename.clone(), format!("{} {}", date, time_str)); }

                    // Track file extensions
                    if let Some(ext) = filename.rsplit('.').next() {
                        let ext_lower = ext.to_lowercase();
                        *extensions.entry(ext_lower).or_insert(0) += 1;
                    }

                    // Track hidden files
                    if filename.starts_with('.') { hidden_files += 1; }

                    // Output file entry with details
                    report.push_str(&format!("  {:>10}  {}  {}\n", fmt_size(size), date, filename));
                }

                total_files += dir_files;
                total_size += dir_size;

                // Directory summary
                report.push_str(&format!("\n  Summary: {} files, total {}\n", dir_files, fmt_size(dir_size)));
                if !largest_file.0.is_empty() {
                    report.push_str(&format!("  Largest: {} ({})\n", largest_file.0, fmt_size(largest_file.1)));
                }
                if !newest_file.0.is_empty() {
                    report.push_str(&format!("  Most recent: {} ({})\n", newest_file.0, newest_file.1));
                }
                // Top file types
                let mut ext_sorted: Vec<(&String, &u32)> = extensions.iter().collect();
                ext_sorted.sort_by(|a, b| b.1.cmp(a.1));
                if !ext_sorted.is_empty() {
                    let ext_str: Vec<String> = ext_sorted.iter().take(5).map(|(e, c)| format!(".{} ({})", e, c)).collect();
                    report.push_str(&format!("  File types: {}\n", ext_str.join(", ")));
                }
                report.push('\n');
            }
        }

        // Hidden files / dotfiles scan
        report.push_str("── Hidden Files (.dotfiles) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -maxdepth 3 -name '.*' -type f 2>/dev/null | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    report.push_str(&format!("  {}\n", path));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No hidden files found\n"); }
        }

        // Recently modified files (last 24h)
        report.push_str("\n── Recently Modified Files (last 24h) ──\n");
        if let Ok(out) = adb.shell(&serial, &["sh", "-c", "find /sdcard -type f -mmin -1440 2>/dev/null | head -30"]) {
            let text = String::from_utf8_lossy(&out);
            let mut count = 0;
            for line in text.lines() {
                let path = line.trim();
                if !path.is_empty() {
                    let name = path.rsplit('/').next().unwrap_or(path);
                    report.push_str(&format!("  {}\n", name));
                    count += 1;
                }
            }
            if count == 0 { report.push_str("  No recently modified files\n"); }
        }

        // Overall summary
        report.push_str(&format!("\n═══ Total: {} files, {} across {} directories ═══\n", total_files, fmt_size(total_size), dirs.len()));
        report.push_str(&format!("Hidden files: {}\n", hidden_files));
        report.push_str(&format!("Manifest saved to: {}\n", manifest_path));

        // Write manifest to file
        let _ = std::fs::write(&manifest_path, &report);

        Ok(report)
    })?
}

// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•
// BUG BOUNTY HUNTER FEATURES (#39-#46)
// â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•â•

/// #39 Permission Auditor Pro â€” find apps with dangerous/overprivileged permissions
#[tauri::command]
fn audit_permissions_pro(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Permission Auditor Pro â•â•â•\n\n");

        // Dangerous permissions that warrant scrutiny
        let dangerous_perms = [
            "android.permission.READ_SMS", "android.permission.RECEIVE_SMS",
            "android.permission.SEND_SMS", "android.permission.READ_CALL_LOG",
            "android.permission.WRITE_CALL_LOG", "android.permission.READ_CONTACTS",
            "android.permission.WRITE_CONTACTS", "android.permission.CAMERA",
            "android.permission.RECORD_AUDIO", "android.permission.ACCESS_FINE_LOCATION",
            "android.permission.ACCESS_COARSE_LOCATION", "android.permission.READ_EXTERNAL_STORAGE",
            "android.permission.WRITE_EXTERNAL_STORAGE", "android.permission.READ_PHONE_STATE",
            "android.permission.CALL_PHONE", "android.permission.SYSTEM_ALERT_WINDOW",
            "android.permission.BIND_ACCESSIBILITY_SERVICE", "android.permission.BIND_NOTIFICATION_LISTENER_SERVICE",
            "android.permission.PACKAGE_USAGE_STATS", "android.permission.WRITE_SECURE_SETTINGS",
            "android.permission.READ_CLIPBOARD", "android.permission.INSTALL_PACKAGES",
            "android.permission.DELETE_PACKAGES", "android.permission.CHANGE_COMPONENT_ENABLED_STATE",
            "android.permission.GRANT_RUNTIME_PERMISSIONS", "android.permission.DUMP",
        ];

        // Signature-level permissions that should NEVER be granted to third-party apps
        let signature_only = [
            "android.permission.BIND_DEVICE_ADMIN", "android.permission.MANAGE_DEVICE_ADMINS",
            "android.permission.WRITE_APN_SETTINGS", "android.permission.READ_LOGS",
            "android.permission.SET_ORIENTATION", "android.permission.REBOOT",
            "android.permission.MOUNT_UNMOUNT_FILESYSTEMS", "android.permission.ACCESS_SURFACE_FLINGER",
        ];

        report.push_str("â”€â”€ Scanning Installed Packages â”€â”€\n");
        let pkg_out = adb.shell(&serial, &["pm", "list", "packages", "-3"]);
        let third_party: Vec<String> = match pkg_out {
            Ok(out) => {
                let text = String::from_utf8_lossy(&out);
                text.lines().filter(|l| l.starts_with("package:")).map(|l| l.trim_start_matches("package:").to_string()).collect()
            }
            Err(_) => Vec::new(),
        };
        report.push_str(&format!("Third-party apps found: {}\n\n", third_party.len()));

        let mut high_risk_count = 0;
        let mut critical_findings: Vec<String> = Vec::new();

        for pkg in &third_party {
            let perms_out = adb.shell(&serial, &["dumpsys", "package", pkg]);
            if let Ok(out) = perms_out {
                let text = String::from_utf8_lossy(&out);
                let mut granted: Vec<&str> = Vec::new();
                let mut sig_violations: Vec<&str> = Vec::new();

                for line in text.lines() {
                    let l = line.trim();
                    // Only count permissions that are actually GRANTED
                    if l.starts_with("android.permission.") && l.contains("granted=true") {
                        let perm = l.split_whitespace().next().unwrap_or(l);
                        if dangerous_perms.contains(&perm) {
                            granted.push(perm);
                        }
                        if signature_only.contains(&perm) {
                            sig_violations.push(perm);
                        }
                    }
                }

                if !sig_violations.is_empty() {
                    high_risk_count += 1;
                    let finding = format!("  [CRITICAL] {} has signature-level permissions: {:?}",
                        pkg, sig_violations);
                    critical_findings.push(finding.clone());
                    report.push_str(&finding);
                    report.push('\n');
                } else if granted.len() >= 5 {
                    high_risk_count += 1;
                    report.push_str(&format!("  [HIGH] {} â€” {} dangerous perms: {:?}\n",
                        pkg, granted.len(), granted.iter().take(8).collect::<Vec<_>>()));
                } else if !granted.is_empty() {
                    report.push_str(&format!("  [MEDIUM] {} â€” {:?}\n", pkg, granted));
                }
            }
        }

        report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
        report.push_str(&format!("  Total third-party apps: {}\n", third_party.len()));
        report.push_str(&format!("  High-risk apps: {}\n", high_risk_count));
        report.push_str(&format!("  Critical findings: {}\n", critical_findings.len()));

        if critical_findings.is_empty() {
            report.push_str("\n  No signature-level permission violations detected.\n");
        } else {
            report.push_str("\n  âš  CRITICAL: These apps have permissions that should require system signature:\n");
            for f in &critical_findings {
                report.push_str(&format!("  {}\n", f));
            }
            report.push_str("\n  These could be bug bounty candidates if OEM granted them improperly.\n");
        }

        Ok(report)
    })?
}

/// #40 Intent Fuzzer â€” probe exported components with malformed intents
#[tauri::command]
fn fuzz_intents(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Intent Fuzzer / Component Prober â•â•â•\n\n");

        report.push_str("â”€â”€ Discovering Exported Components â”€â”€\n");
        let dump_out = adb.shell(&serial, &["dumpsys", "package"]);
        let text = match dump_out {
            Ok(out) => String::from_utf8_lossy(&out).to_string(),
            Err(e) => return Err(format!("Failed to dump packages: {}", e)),
        };

        let mut exported_activities: Vec<(String, String)> = Vec::new(); // (package, activity)
        let mut exported_receivers: Vec<(String, String)> = Vec::new();
        let mut exported_services: Vec<(String, String)> = Vec::new();
        let mut exported_providers: Vec<String> = Vec::new();

        let mut current_pkg = String::new();
        let mut in_activity = false;
        let mut in_receiver = false;
        let mut in_service = false;

        for line in text.lines() {
            let l = line.trim();
            if l.starts_with("Package [") {
                current_pkg = l.trim_start_matches("Package [").trim_end_matches("]:").to_string();
            }
            if l.contains("Activity filter") || l.contains("Receiver filter") || l.contains("Service filter") {
                in_activity = l.contains("Activity");
                in_receiver = l.contains("Receiver");
                in_service = l.contains("Service");
            }
            if l.contains("exported=true") || l.contains("exported=TRUE") {
                // Look for component name in preceding lines
                if in_activity {
                    // Try to find the activity name
                    if let Some(pos) = text[..text.find(l).unwrap_or(0)].rfind(&current_pkg) {
                        let after = &text[pos..];
                        if let Some(name_line) = after.lines().find(|ll| ll.contains(&current_pkg) && ll.contains("/")) {
                            let name = name_line.trim().split_whitespace().last().unwrap_or("").to_string();
                            if name.contains('/') && !exported_activities.iter().any(|(p, _)| p == &name) {
                                exported_activities.push((current_pkg.clone(), name));
                            }
                        }
                    }
                }
            }
            if l.starts_with("Provider{") && l.contains("exported=true") {
                let provider_name = l.split('{').nth(1).and_then(|s| s.split('}').next()).unwrap_or("").to_string();
                if !provider_name.is_empty() {
                    exported_providers.push(provider_name);
                }
            }
        }

        report.push_str(&format!("  Exported activities found: {}\n", exported_activities.len().min(50)));
        report.push_str(&format!("  Exported providers found: {}\n", exported_providers.len()));

        // Probe exported activities with unusual extras
        report.push_str("\nâ”€â”€ Probing Activities (safe, non-destructive) â”€â”€\n");
        let mut crash_count = 0;
        let mut probed = 0;
        let probe_extras = [
            "--es _extra_test \"AAAA\"",
            "--ei _test_int 99999",
            "--ez _test_bool true",
        ];

        for (pkg, activity) in exported_activities.iter().take(20) {
            let component = if activity.contains('/') {
                activity.clone()
            } else {
                format!("{}/{}", pkg, activity)
            };

            for extra in &probe_extras {
                let cmd = format!("am start -n {} {}", component, extra);
                if let Ok(out) = adb.shell(&serial, &["sh", "-c", &cmd]) {
                    let result = String::from_utf8_lossy(&out);
                    if result.contains("Crash") || result.contains("crash") || result.contains("FATAL") {
                        crash_count += 1;
                        report.push_str(&format!("  [CRASH] {} with {}\n", component, extra));
                        report.push_str(&format!("    Output: {}\n", result.trim()));
                    }
                }
                probed += 1;
                if probed >= 60 { break; } // limit probing
            }
            if probed >= 60 { break; }
        }

        report.push_str(&format!("\nâ”€â”€ Fuzzing Summary â”€â”€\n"));
        report.push_str(&format!("  Components probed: {}\n", probed));
        report.push_str(&format!("  Crashes detected: {}\n", crash_count));
        if crash_count > 0 {
            report.push_str("\n  âš  CRASHES FOUND: These may indicate exploitable bugs.\n");
            report.push_str("  Check logcat for stack traces: adb logcat -b crash\n");
            report.push_str("  Reportable to Google Bug Bounty if in AOSP component.\n");
        } else {
            report.push_str("\n  No crashes detected with safe extras.\n");
            report.push_str("  For deeper fuzzing, consider AFL-based intent fuzzing with root.\n");
        }

        Ok(report)
    })?
}

/// #41 SELinux Policy Analyzer â€” check enforcement status and permissive domains
#[tauri::command]
fn analyze_selinux(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• SELinux Policy Analyzer â•â•â•\n\n");

        // 1. Current enforcement mode
        report.push_str("â”€â”€ Enforcement Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getenforce"]) {
            let mode = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Current Mode: {}\n", mode));
            if mode == "Permissive" {
                report.push_str("  âš  CRITICAL: SELinux is in PERMISSIVE mode!\n");
                report.push_str("  All access controls are logged but NOT enforced.\n");
                report.push_str("  This is a major security weakness â€” bug bounty candidate.\n");
            } else if mode == "Disabled" {
                report.push_str("  âš  CRITICAL: SELinux is DISABLED!\n");
                report.push_str("  No mandatory access control is active.\n");
                report.push_str("  This is a critical security vulnerability.\n");
            } else if mode == "Enforcing" {
                report.push_str("  OK: SELinux is enforcing.\n");
            }
        }

        // 2. Check for permissive domains
        report.push_str("\nâ”€â”€ Permissive Domain Check â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dmesg"]) {
            let dmesg = String::from_utf8_lossy(&out);
            let mut avc_denials = 0;
            let mut permissive_domains: Vec<String> = Vec::new();

            for line in dmesg.lines() {
                if line.contains("avc:") && line.contains("denied") {
                    avc_denials += 1;
                }
                if line.contains("permissive") && line.contains("domain") {
                    let domain = line.split("domain=").nth(1)
                        .and_then(|s| s.split_whitespace().next())
                        .unwrap_or("unknown")
                        .to_string();
                    if !permissive_domains.contains(&domain) {
                        permissive_domains.push(domain);
                    }
                }
            }

            report.push_str(&format!("  AVC denials in dmesg: {}\n", avc_denials));
            if !permissive_domains.is_empty() {
                report.push_str(&format!("  âš  Permissive domains found: {:?}\n", permissive_domains));
                report.push_str("  These domains bypass SELinux enforcement.\n");
            } else {
                report.push_str("  No permissive domains detected.\n");
            }
        }

        // 3. Check sepolicy version
        report.push_str("\nâ”€â”€ Policy Version â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.build.version.security_patch"]) {
            let patch = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Security Patch: {}\n", patch));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.build.selinux"]) {
            let selinux_build = String::from_utf8_lossy(&out).trim().to_string();
            if !selinux_build.is_empty() {
                report.push_str(&format!("  SELinux Build: {}\n", selinux_build));
            }
        }

        // 4. Check for SELinux booleans that may be misconfigured
        report.push_str("\nâ”€â”€ SELinux Booleans â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["sestatus", "-b"]) {
            let sestatus = String::from_utf8_lossy(&out);
            let mut active_count = 0;
            let mut concerning: Vec<String> = Vec::new();
            for line in sestatus.lines() {
                let l = line.trim();
                if l.contains("on") && !l.starts_with("#") {
                    active_count += 1;
                    let bool_name = l.split_whitespace().next().unwrap_or("").to_string();
                    // Flag booleans that could weaken security
                    if bool_name.contains("allow") || bool_name.contains("unconfined") || bool_name.contains("permissive") {
                        concerning.push(bool_name);
                    }
                }
            }
            report.push_str(&format!("  Active booleans: {}\n", active_count));
            if !concerning.is_empty() {
                report.push_str(&format!("  âš  Security-relevant booleans: {:?}\n", concerning.iter().take(10).collect::<Vec<_>>()));
            }
        } else {
            report.push_str("  sestatus not available (may need root)\n");
        }

        // 5. Check /proc/keys and other SELinux-protected resources
        report.push_str("\nâ”€â”€ Protected Resource Access Tests â”€â”€\n");
        let tests = [
            ("cat /proc/1/environ", "Init environ read (should be denied)"),
            ("cat /data/system/selinux_policy", "SELinux policy read"),
            ("ls /sys/fs/selinux/", "SELinux fs access"),
        ];
        for (cmd, desc) in &tests {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", cmd]) {
                let result = String::from_utf8_lossy(&out);
                if !result.contains("Permission denied") && !result.contains("denied") && !result.is_empty() {
                    report.push_str(&format!("  âš  {}: ALLOWED (potential weakness)\n", desc));
                    report.push_str(&format!("    Output: {}...\n", result.trim().chars().take(80).collect::<String>()));
                } else {
                    report.push_str(&format!("  OK: {} â€” denied as expected\n", desc));
                }
            }
        }

        Ok(report)
    })?
}

/// #42 Content Provider Enumerator â€” probe content:// URIs for data leaks
#[tauri::command]
fn enumerate_content_providers(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Content Provider Enumerator â•â•â•\n\n");

        // Known sensitive content URIs to probe
        let sensitive_uris = [
            ("content://sms", "SMS messages"),
            ("content://call_log/calls", "Call log"),
            ("content://contacts/phones", "Contacts"),
            ("content://media/external/images/media", "External images"),
            ("content://media/external/video/media", "External videos"),
            ("content://media/external/audio/media", "External audio"),
            ("content://downloads/my_downloads", "Downloads"),
            ("content://settings/global", "Global settings"),
            ("content://settings/secure", "Secure settings"),
            ("content://settings/system", "System settings"),
            ("content://com.android.calendar/events", "Calendar events"),
            ("content://com.android.calendar/calendars", "Calendars"),
            ("content://browser/bookmarks", "Browser bookmarks"),
            ("content://browser/searches", "Browser searches"),
            ("content://telephony/carriers", "Carrier APNs"),
            ("content://mms", "MMS messages"),
            ("content://mms-sms/conversations", "MMS-SMS conversations"),
            ("content://com.google.android.gsf.gservices", "Google services"),
            ("content://com.google.settings", "Google settings"),
            ("content://com.android.providers.downloads/downloads", "Download manager"),
        ];

        report.push_str("â”€â”€ Probing Sensitive Content URIs â”€â”€\n\n");
        let mut leaks_found = 0;

        for (uri, desc) in &sensitive_uris {
            let query = format!("content query --uri {} 2>&1", uri);
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &query]) {
                let result = String::from_utf8_lossy(&out);
                let result_trimmed = result.trim();

                if result_trimmed.contains("Permission denied") || result_trimmed.contains("SecurityException") {
                    report.push_str(&format!("  [DENIED] {} â€” properly protected\n", desc));
                } else if result_trimmed.contains("Row") || result_trimmed.contains("_id=") {
                    let row_count = result_trimmed.matches("Row").count();
                    leaks_found += 1;
                    report.push_str(&format!("  [LEAK] {} â€” {} rows accessible!\n", desc, row_count));
                    // Show first row as evidence
                    if let Some(first_row) = result_trimmed.lines().find(|l| l.contains("Row")) {
                        report.push_str(&format!("    Sample: {}...\n", first_row.chars().take(120).collect::<String>()));
                    }
                } else if result_trimmed.is_empty() || result_trimmed.contains("No items") {
                    report.push_str(&format!("  [EMPTY] {} â€” accessible but empty\n", desc));
                } else {
                    report.push_str(&format!("  [UNKNOWN] {} â€” unexpected response\n", desc));
                }
            }
        }

        // Also try to discover unknown providers from package info
        report.push_str("\nâ”€â”€ Discovering Third-Party Providers â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "package", "providers"]) {
            let text = String::from_utf8_lossy(&out);
            let mut provider_count = 0;
            for line in text.lines() {
                let l = line.trim();
                if l.starts_with("ContentProvider{") && l.contains("}") {
                    provider_count += 1;
                    if provider_count <= 30 {
                        let name = l.split('{').nth(1).and_then(|s| s.split(' ').next()).unwrap_or("?");
                        let pkg = l.split(' ').nth(1).unwrap_or("?");
                        report.push_str(&format!("  {} ({})\n", name, pkg));
                    }
                }
            }
            report.push_str(&format!("\n  Total providers discovered: {}\n", provider_count));
        }

        report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
        report.push_str(&format!("  Sensitive URIs probed: {}\n", sensitive_uris.len()));
        report.push_str(&format!("  Data leaks found: {}\n", leaks_found));
        if leaks_found > 0 {
            report.push_str("\n  âš  DATA LEAKS: These content providers are accessible without permission!\n");
            report.push_str("  This may indicate a permission bypass vulnerability â€” bug bounty candidate.\n");
        } else {
            report.push_str("\n  All sensitive providers properly denied. No leaks detected.\n");
        }

        Ok(report)
    })?
}

/// #43 Privilege Escalation Tester â€” check if shell UID can access restricted APIs
#[tauri::command]
fn test_privilege_escalation(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Privilege Escalation Tester â•â•â•\n\n");

        report.push_str("â”€â”€ Testing Shell UID Privileges â”€â”€\n\n");
        let mut escalations: Vec<(String, String)> = Vec::new();

        // Test vector: (command, description, should_be_denied)
        let tests: Vec<(Vec<&str>, &str, bool)> = vec![
            (vec!["cat", "/proc/1/environ"], "Read init process environment", true),
            (vec!["cat", "/proc/kallsyms"], "Read kernel symbol table", true),
            (vec!["cat", "/data/system/users/0/settings_secure.xml"], "Read secure settings DB", true),
            (vec!["ls", "/data/data/"], "List app data directories", true),
            (vec!["cat", "/dev/__properties__/u:object_r:system_prop:s0"], "Read system properties raw", true),
            (vec!["service", "call", "iphonesubinfo", "1"], "Read IMEI via service call", false),
            (vec!["settings", "put", "global", "airplane_mode_on", "1"], "Toggle airplane mode via settings", false),
            (vec!["am", "broadcast", "-a", "android.intent.action.BOOT_COMPLETED"], "Send boot completed broadcast", true),
            (vec!["pm", "grant", "com.android.shell", "android.permission.READ_SMS"], "Self-grant SMS permission", true),
            (vec!["content", "query", "--uri", "content://settings/secure"], "Read secure settings via content", false),
            (vec!["dumpsys", "battery", "set", "status", "2"], "Fake battery charging status", false),
            (vec!["cmd", "statusbar", "expand-notifications"], "Expand statusbar via cmd", false),
            (vec!["wm", "size", "720x1280"], "Change display resolution via wm", true),
            (vec!["input", "keyevent", "26"], "Send power key event", false),
            (vec!["setprop", "persist.sys.language", "en"], "Set persistent system property", true),
        ];

        for (cmd, desc, should_fail) in &tests {
            if let Ok(out) = adb.shell(&serial, cmd) {
                let result = String::from_utf8_lossy(&out);
                let result_trimmed = result.trim();
                let denied = result_trimmed.contains("Permission denied")
                    || result_trimmed.contains("SecurityException")
                    || result_trimmed.contains("does not have permission")
                    || result_trimmed.contains("Operation not allowed");

                if *should_fail && !denied {
                    escalations.push((desc.to_string(), format!("ALLOWED: {}", result_trimmed.chars().take(100).collect::<String>())));
                    report.push_str(&format!("  [ESCALATION] {} â€” SUCCEEDED (should be denied)\n", desc));
                    report.push_str(&format!("    Result: {}\n", result_trimmed.chars().take(120).collect::<String>()));
                } else if *should_fail && denied {
                    report.push_str(&format!("  [OK] {} â€” properly denied\n", desc));
                } else if !*should_fail && !denied {
                    report.push_str(&format!("  [INFO] {} â€” allowed (expected)\n", desc));
                } else {
                    report.push_str(&format!("  [INFO] {} â€” denied (may be expected)\n", desc));
                }
            }
        }

        // Check UID and capabilities
        report.push_str("\nâ”€â”€ Shell Identity â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["id"]) {
            let id_info = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Shell UID: {}\n", id_info));
        }
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/self/status"]) {
            let status = String::from_utf8_lossy(&out);
            for line in status.lines() {
                if line.starts_with("Cap") {
                    report.push_str(&format!("  {}\n", line.trim()));
                }
            }
        }

        report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
        report.push_str(&format!("  Tests run: {}\n", tests.len()));
        report.push_str(&format!("  Privilege escalations found: {}\n", escalations.len()));
        if !escalations.is_empty() {
            report.push_str("\n  âš  ESCALATION VECTORS FOUND:\n");
            for (desc, result) in &escalations {
                report.push_str(&format!("  â€¢ {} â€” {}\n", desc, result));
            }
            report.push_str("\n  These operations should be restricted for the shell UID.\n");
            report.push_str("  Reportable as privilege escalation bugs to Google Bug Bounty.\n");
        } else {
            report.push_str("\n  No privilege escalation vectors detected. Shell UID properly restricted.\n");
        }

        Ok(report)
    })?
}

/// #44 Secure Element Probe â€” test NFC/HCE isolation and secure storage
#[tauri::command]
fn probe_secure_element(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Secure Element / NFC Probe â•â•â•\n\n");

        // 1. NFC hardware status
        report.push_str("â”€â”€ NFC Hardware Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "nfc"]) {
            let text = String::from_utf8_lossy(&out);
            let mut found_state = false;
            for line in text.lines() {
                let l = line.trim();
                if l.contains("mState") || l.contains("State:") {
                    report.push_str(&format!("  {}\n", l));
                    found_state = true;
                }
                if l.contains("mIsHceCapable") || l.contains("mIsNfcCapable") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
            if !found_state { report.push_str("  NFC state not reported\n"); }
        } else {
            report.push_str("  NFC service not available\n");
        }

        // 2. Check NFC access permissions
        report.push_str("\nâ”€â”€ NFC Access Control â”€â”€\n");
        let nfc_tests = [
            ("ls /dev/pn547", "NFC device node"),
            ("ls /dev/nq-nci", "NFC NCI device"),
            ("cat /sys/class/nfc/", "NFC sysfs class"),
        ];
        for (cmd, desc) in &nfc_tests {
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", cmd]) {
                let result = String::from_utf8_lossy(&out);
                if result.contains("Permission denied") {
                    report.push_str(&format!("  [OK] {} â€” properly denied\n", desc));
                } else if !result.trim().is_empty() {
                    report.push_str(&format!("  [ACCESS] {} â€” shell can access: {}\n", desc, result.trim().chars().take(80).collect::<String>()));
                }
            }
        }

        // 3. Secure lockscreen / keyguard analysis
        report.push_str("\nâ”€â”€ Keyguard / Secure Lockscreen â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "trust"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines() {
                let l = line.trim();
                if l.contains("Agent") || l.contains("enabled") || l.contains("trusted") {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        }

        // 4. Keystore status
        report.push_str("\nâ”€â”€ Keystore / Keymint Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dumpsys", "android.security.keystore2"]) {
            let text = String::from_utf8_lossy(&out);
            for line in text.lines().take(20) {
                let l = line.trim();
                if !l.is_empty() && (l.contains("version") || l.contains("security") || l.contains("TEE") || l.contains("StrongBox")) {
                    report.push_str(&format!("  {}\n", l));
                }
            }
        } else {
            // Try alternative
            if let Ok(out) = adb.shell(&serial, &["getprop", "ro.hardware.keystore"]) {
                let hw = String::from_utf8_lossy(&out).trim().to_string();
                if !hw.is_empty() {
                    report.push_str(&format!("  Keystore Hardware: {}\n", hw));
                }
            }
        }

        // 5. Check for StrongBox / TEE
        report.push_str("\nâ”€â”€ Hardware Security Module â”€â”€\n");
        let sec_props = [
            ("ro.hardware.keystore", "Keystore HW"),
            ("ro.hardware.keystore_desede", "DESede support"),
            ("ro.boot.verifiedbootstate", "Verified boot state"),
            ("ro.boot.veritymode", "dm-verity mode"),
            ("ro.boot.flash.locked", "Flash locked"),
        ];
        for (prop, label) in &sec_props {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() {
                    report.push_str(&format!("  {}: {}\n", label, v));
                }
            }
        }

        Ok(report)
    })?
}

/// #45 Bootloader / Verified Boot Checker â€” dm-verity, AVB, lock status
#[tauri::command]
fn check_verified_boot(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Bootloader / Verified Boot Checker â•â•â•\n\n");

        // 1. Bootloader lock status
        report.push_str("â”€â”€ Bootloader Lock Status â”€â”€\n");
        let lock_props = [
            ("ro.boot.flash.locked", "Flash Locked"),
            ("ro.boot.verifiedbootstate", "Verified Boot State"),
            ("ro.boot.veritymode", "dm-verity Mode"),
            ("ro.debuggable", "Debuggable Build"),
            ("ro.secure", "Secure Build"),
            ("ro.build.type", "Build Type"),
        ];
        let mut critical_issues: Vec<String> = Vec::new();

        for (prop, label) in &lock_props {
            if let Ok(out) = adb.shell(&serial, &["getprop", prop]) {
                let v = String::from_utf8_lossy(&out).trim().to_string();
                if !v.is_empty() {
                    let warning = match *prop {
                        "ro.boot.flash.locked" if v == "0" => Some("âš  Bootloader is UNLOCKED"),
                        "ro.boot.verifiedbootstate" if v == "orange" || v == "yellow" => Some("âš  Verified boot state is non-green"),
                        "ro.debuggable" if v == "1" => Some("âš  Build is debuggable"),
                        "ro.build.type" if v == "eng" || v == "userdebug" => Some("âš  Non-production build type"),
                        "ro.boot.veritymode" if v == "disabled" => Some("âš  dm-verity is DISABLED"),
                        _ => None,
                    };
                    if let Some(w) = warning {
                        report.push_str(&format!("  {} {}: {} â€” {}\n", w, label, v, "SECURITY RISK"));
                        critical_issues.push(format!("{}: {}", label, v));
                    } else {
                        report.push_str(&format!("  {}: {} âœ“\n", label, v));
                    }
                }
            }
        }

        // 2. dm-verity status
        report.push_str("\nâ”€â”€ dm-verity / AVB Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.boot.veritymode"]) {
            let mode = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  dm-verity: {}\n", if mode.is_empty() { "unknown" } else { &mode }));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.boot.avb_version"]) {
            let avb = String::from_utf8_lossy(&out).trim().to_string();
            if !avb.is_empty() { report.push_str(&format!("  AVB Version: {}\n", avb)); }
        }

        // 3. Check mount points for verity
        report.push_str("\nâ”€â”€ Partition Integrity â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/mounts"]) {
            let mounts = String::from_utf8_lossy(&out);
            let mut verity_count = 0;
            let mut dm_count = 0;
            for line in mounts.lines() {
                if line.contains("verity") { verity_count += 1; }
                if line.contains("/dev/block/dm-") { dm_count += 1; }
            }
            report.push_str(&format!("  Verity mount entries: {}\n", verity_count));
            report.push_str(&format!("  Device-mapper partitions: {}\n", dm_count));
            if dm_count == 0 {
                report.push_str("  âš  No device-mapper partitions â€” system may not use verified boot\n");
            }
        }

        // 4. Kernel command line (boot parameters)
        report.push_str("\nâ”€â”€ Kernel Boot Parameters â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["cat", "/proc/cmdline"]) {
            let cmdline = String::from_utf8_lossy(&out);
            for param in cmdline.split_whitespace() {
                if param.contains("verity") || param.contains("avb") || param.contains("dm=")
                    || param.contains("boot") || param.contains("secure") || param.contains("androidboot") {
                    report.push_str(&format!("  {}\n", param));
                }
            }
        }

        // 5. Encryption status
        report.push_str("\nâ”€â”€ Encryption Status â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.state"]) {
            let state = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Encryption: {}\n", if state.is_empty() { "unknown" } else { &state }));
        }
        if let Ok(out) = adb.shell(&serial, &["getprop", "ro.crypto.type"]) {
            let ctype = String::from_utf8_lossy(&out).trim().to_string();
            if !ctype.is_empty() { report.push_str(&format!("  Type: {}\n", ctype)); }
        }

        report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
        if critical_issues.is_empty() {
            report.push_str("  âœ“ Bootloader locked, verified boot intact, no issues detected.\n");
        } else {
            report.push_str(&format!("  âš  {} security issues found:\n", critical_issues.len()));
            for issue in &critical_issues {
                report.push_str(&format!("  â€¢ {}\n", issue));
            }
            report.push_str("\n  Unlocked bootloader or disabled verity = bug bounty candidate.\n");
        }

        Ok(report)
    })?
}

/// #46 Kernel Exploit Detector â€” check kernel config, /proc exposure, known vulns
#[tauri::command]
fn detect_kernel_exploits(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut report = String::from("â•â•â• Kernel Exploit Detector â•â•â•\n\n");

        // 1. Kernel version and info
        report.push_str("â”€â”€ Kernel Information â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["uname", "-a"]) {
            let uname = String::from_utf8_lossy(&out).trim().to_string();
            report.push_str(&format!("  Kernel: {}\n", uname));
            // Parse version
            if let Some(ver) = uname.split_whitespace().nth(2) {
                report.push_str(&format!("  Version: {}\n", ver));
                let parts: Vec<&str> = ver.split('.').collect();
                if parts.len() >= 3 {
                    let major: u32 = parts[0].parse().unwrap_or(0);
                    let minor: u32 = parts[1].parse().unwrap_or(0);
                    if major < 4 || (major == 4 && minor < 9) {
                        report.push_str("  âš  Kernel version < 4.9 â€” may be vulnerable to known exploits\n");
                    }
                }
            }
        }

        // 2. Check /proc exposure (information leaks)
        report.push_str("\nâ”€â”€ /proc Information Leak Check â”€â”€\n");
        let proc_checks: Vec<(&str, &str, bool)> = vec![
            ("/proc/kallsyms", "Kernel symbols", true),
            ("/proc/1/environ", "Init environment", true),
            ("/proc/1/cmdline", "Init cmdline", false),
            ("/proc/vmallocinfo", "Vmalloc info", true),
            ("/proc/slabinfo", "Slab allocator info", true),
            ("/proc/iomem", "IO memory map", true),
            ("/proc/ioports", "IO ports", true),
            ("/proc/config.gz", "Kernel config", true),
            ("/proc/net/tcp", "TCP connections", false),
            ("/proc/net/unix", "Unix sockets", false),
            ("/proc/modules", "Loaded modules", true),
        ];

        let mut leak_count = 0;
        for (path, desc, sensitive) in &proc_checks {
            if let Ok(out) = adb.shell(&serial, &["cat", path]) {
                let result = String::from_utf8_lossy(&out);
                if result.contains("Permission denied") {
                    report.push_str(&format!("  [OK] {} â€” denied\n", desc));
                } else if !result.trim().is_empty() {
                    if *sensitive {
                        leak_count += 1;
                        report.push_str(&format!("  [LEAK] {} â€” ACCESSIBLE ({} bytes)\n", desc, result.len()));
                    } else {
                        report.push_str(&format!("  [INFO] {} â€” accessible (non-sensitive)\n", desc));
                    }
                }
            }
        }

        // 3. Check dmesg access
        report.push_str("\nâ”€â”€ dmesg Access â”€â”€\n");
        if let Ok(out) = adb.shell(&serial, &["dmesg"]) {
            let dmesg = String::from_utf8_lossy(&out);
            if dmesg.contains("Permission denied") {
                report.push_str("  [OK] dmesg â€” properly restricted\n");
            } else {
                let line_count = dmesg.lines().count();
                leak_count += 1;
                report.push_str(&format!("  [LEAK] dmesg â€” {} lines accessible\n", line_count));
                // Check for interesting kernel messages
                for line in dmesg.lines() {
                    if line.contains("BUG:") || line.contains("Oops") || line.contains("panic") {
                        report.push_str(&format!("    âš  {}\n", line.trim().chars().take(100).collect::<String>()));
                    }
                }
            }
        }

        // 4. Check for open debug interfaces
        report.push_str("\nâ”€â”€ Debug Interface Check â”€â”€\n");
        let debug_paths = [
            "/sys/kernel/debug/",
            "/sys/debug/",
            "/proc/sys/kernel/core_pattern",
            "/proc/sys/kernel/kptr_restrict",
            "/proc/sys/kernel/dmesg_restrict",
            "/proc/sys/kernel/modules_disabled",
        ];
        for path in &debug_paths {
            if let Ok(out) = adb.shell(&serial, &["cat", path]) {
                let result = String::from_utf8_lossy(&out);
                if !result.contains("Permission denied") {
                    let val = result.trim();
                    report.push_str(&format!("  {}: {}\n", path, val.chars().take(100).collect::<String>()));
                    // Check security-critical values
                    if path.contains("kptr_restrict") && val == "0" {
                        report.push_str("    âš  Kernel pointers exposed â€” helps kernel exploits\n");
                        leak_count += 1;
                    }
                    if path.contains("dmesg_restrict") && val == "0" {
                        report.push_str("    âš  dmesg unrestricted\n");
                    }
                }
            }
        }

        // 5. Check for known vulnerable kernel configs
        report.push_str("\nâ”€â”€ Security Config Check â”€â”€\n");
        let sec_checks = [
            ("CONFIG_STRICT_KERNEL_RWX", "Strict kernel text protection"),
            ("CONFIG_RANDOMIZE_BASE", "KASLR enabled"),
            ("CONFIG_STACKPROTECTOR", "Stack protector"),
            ("CONFIG_SECURITY_SELINUX", "SELinux support"),
        ];
        for (config, desc) in &sec_checks {
            let check_cmd = format!("zcat /proc/config.gz 2>/dev/null | grep {}", config);
            if let Ok(out) = adb.shell(&serial, &["sh", "-c", &check_cmd]) {
                let result = String::from_utf8_lossy(&out).trim().to_string();
                if result.contains("=y") || result.contains("=m") {
                    report.push_str(&format!("  âœ“ {} â€” enabled\n", desc));
                } else if result.contains("=n") {
                    report.push_str(&format!("  âš  {} â€” DISABLED\n", desc));
                    leak_count += 1;
                } else {
                    report.push_str(&format!("  ? {} â€” unknown ({})\n", desc, if result.is_empty() { "config not available" } else { &result }));
                }
            }
        }

        report.push_str(&format!("\nâ”€â”€ Summary â”€â”€\n"));
        report.push_str(&format!("  Information leaks found: {}\n", leak_count));
        if leak_count >= 5 {
            report.push_str("  âš  HIGH: Multiple kernel information leaks detected.\n");
            report.push_str("  These can aid kernel exploit development.\n");
        } else if leak_count >= 2 {
            report.push_str("  MODERATE: Some information leaks detected.\n");
        } else {
            report.push_str("  LOW: Kernel is reasonably hardened against information leaks.\n");
        }

        Ok(report)
    })?
}

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
            detect_kernel_exploits
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
