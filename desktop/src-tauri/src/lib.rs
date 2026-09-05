//! RecoverKit desktop — Tauri commands wrapping the `recovery_core` engine.

mod icons;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use recovery_core::{
    acquisition::{self, DirectAcquisition},
    adb,
    adb::Device,
    case_file,
    crypto,
    scanner::{self, ScanReport},
    screen_capture,
    sim_forensics,
    sms_xml,
    sqlite::{self, Analysis, Message, ParseResult},
    txt_export, vcf,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

/// Cap on messages sent to the UI in one shot (full data stays in state for export).
const UI_MESSAGE_CAP: usize = 3000;

struct AppState {
    last_result: Mutex<Option<ParseResult>>,
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
    /// jid → display name (identity resolution for the UI).
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
        .ok_or("This file is still on the phone and no device is selected — go to ① Connect, then use ⬇ Scan & pull on step ②")?;
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
        // Different su flavors take the command differently — try the common styles.
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
            "root access not working — su said: '{}'. If a superuser prompt appeared on the phone, tap GRANT and retry. \
             On custom ROMs also check Developer options → Root access → 'Apps and ADB'.",
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
/// `whatsapp` (default), `gallery` (DCIM/Pictures/Movies — any app) or
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
            MediaProgress { phase: "scan", folder: "Scanning the phone…".into(), index: 0, total: 0, files: Vec::new() },
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
            return Err("No recovered media to save yet — click \u{2b07} Recover media first.".into());
        }
        let dest = PathBuf::from(&dest);
        std::fs::create_dir_all(&dest).map_err(|e| e.to_string())?;
        // Copy in parallel across all CPU cores — thousands of files finish
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

        // Battery: `dumpsys battery` → "level: 87", "USB powered: true"
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

        // Storage: `df /sdcard` second line → total / available (KB)
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
            return Err("Could not capture the screen — unlock the phone and re-allow USB debugging.".into());
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

/// Screen streaming state (one per device).
struct StreamState {
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

/// Start a high-frequency screen stream for live viewing.
/// `mode` = "fast" uses screenrecord (30-60 fps), "normal" uses screencap (5-30 fps).
/// Emits frames via Tauri events. Returns immediately; stream runs in background.
#[tauri::command]
fn start_screen_stream(app: AppHandle, serial: String, fps: u32, mode: String) -> Result<(), String> {
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let running_clone = running.clone();
    let app_clone = app.clone();

    std::thread::spawn(move || {
        let adb = adb::AdbClient::default();
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
        let _ = adb.exec_out(&serial, &["input", "keyevent", "224"]);
        std::thread::sleep(std::time::Duration::from_millis(300));

        if mode == "fast" {
            // HIGH-FPS MODE: Use screenrecord for 30-60 fps
            // Record short clips and stream them to the frontend
            eprintln!("Starting high-FPS screenrecord mode at {} fps", fps);
            
            let clip_duration = 2; // Record 2-second clips
            let remote_path = "/sdcard/recoverkit_stream.mp4";
            let mut clip_count = 0u64;
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                // Record a short clip
                let record_cmd = format!(
                    "screenrecord --time-limit {} --bit-rate 4000000 --size 720x1280 {}",
                    clip_duration, remote_path
                );
                
                match adb.exec_out(&serial, &["sh", "-c", &record_cmd]) {
                    Ok(_) => {
                        // Pull the recorded clip
                        let local_clip = mirror_dir.join(format!("clip_{}.mp4", clip_count % 5));
                        if adb.pull(&serial, remote_path, &local_clip).is_ok() {
                            // Emit the video clip to frontend
                            let _ = app_clone.emit(
                                "screen-frame",
                                serde_json::json!({
                                    "serial": serial,
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
                        
                        // Clean up remote file
                        let _ = adb.exec_out(&serial, &["rm", "-f", remote_path]);
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

                // Capture frame
                match adb.exec_out(&serial, &["screencap", "-p"]) {
                    Ok(bytes) if bytes.len() > 100 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) => {
                        let frame_path = mirror_dir.join(format!("{serial}_stream_{}.png", frame_count % 10));
                        if std::fs::write(&frame_path, &bytes).is_ok() {
                            let _ = app_clone.emit(
                                "screen-frame",
                                serde_json::json!({
                                    "serial": serial,
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

                // Maintain target FPS
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
async fn stop_screen_stream(_serial: String) -> Result<(), String> {
    // In a real implementation, we'd track StreamState per device
    // For now, the stream stops when the app is closed
    Ok(())
}

/// One installed app + its configuration, as far as non-root ADB allows.
#[derive(Serialize)]
struct AppInfo {
    package: String,
    apk_path: String,
    /// Installed under /system, /product, /vendor, … (preloaded firmware).
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
            // line format: package:/data/app/…/base.apk=com.example
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

/// Import an SMS Backup & Restore `.xml` file — real SMS/MMS, no root.
#[tauri::command]
fn import_sms_xml(state: tauri::State<AppState>, path: String) -> Result<UiRecovery, String> {
    let result = run_blocking(move || -> Result<ParseResult, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read the XML file: {e}"))?;
        let result = sms_xml::import_from_xml(&text, &path);
        if result.messages.is_empty() {
            return Err(
                "No SMS entries found — is this an SMS Backup & Restore .xml file? \
                 (install \"SMS Backup & Restore\" on the phone → Back up → copy the .xml here)"
                    .into(),
            );
        }
        Ok(result)
    })??;
    let ui = build_ui(&result);
    *state.last_result.lock().unwrap() = Some(result);
    Ok(ui)
}

/// Import a `.vcf` contacts export — full address book, no root.
#[tauri::command]
fn import_vcf(state: tauri::State<AppState>, path: String) -> Result<UiRecovery, String> {
    let result = run_blocking(move || -> Result<ParseResult, String> {
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("cannot read the .vcf file: {e}"))?;
        let result = vcf::import_from_vcf(&text, &path);
        if result.contacts.is_empty() {
            return Err(
                "No contacts found — is this a .vcf exported from the phone's Contacts app? \
                 (Contacts → ⋯ → Export → .vcf)"
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

/// Import a WhatsApp “Export chat” `.txt` transcript — real messages,
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
                "No messages found in this file — is it a WhatsApp chat export .txt? \
                 (WhatsApp → chat → ⋯ → More → Export chat → Without media)"
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
    let result = guard.as_ref().ok_or("Run a recovery first — nothing to export")?;
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
            .unwrap_or_else(|| "—".into());
        rows.push_str(&format!(
            "<tr class=\"{class}\"><td>{ts}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            html_escape(&display_name(m.chat_jid.as_deref().unwrap_or("—"))),
            if m.from_me { "out" } else { "in" },
            html_escape(m.text.as_deref().unwrap_or("")),
        ));
        let _ = label;
    }

    let range = match (analysis.first_ts, analysis.last_ts) {
        (Some(a), Some(b)) => format!("{} → {}", chrono_fmt(a), chrono_fmt(b)),
        _ => "unknown".into(),
    };
    let top = analysis
        .top_chats
        .iter()
        .map(|(jid, name, count)| {
            let who = if name.is_empty() { jid.clone() } else { format!("{name} ({jid})") };
            format!("<li>{} — {} messages</li>", html_escape(&who), count)
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
<h1>RecoverKit — recovery report</h1>
<p class="meta">db: {db} · schema: {schema} · messages: {total} · recovered deleted: {deleted} · chats: {chats} · contacts: {contacts} · media refs: {media}</p>
<p class="meta">🛡 SHA-256 of source database: <code>{sha}</code> · activity span: {range} · outgoing share: {outgoing}%</p>
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

// ── Screen capture history ───────────────────────────────────────────────

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
        
        // Smart gate state — tracks the last saved frame hash + foreground app
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
                    // Meaningful, distinct screen — emit to frontend
                    let _ = app_clone.emit("screen-capture-event", &capture);
                }
                Ok(None) => {
                    // Skipped: dark/locked frame, or identical static screen — no event
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

// ── SIM / network forensics ──────────────────────────────────────────────

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

// ── Consolidated case file + chain of custody ────────────────────────────

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
            .ok_or("No recovered WhatsApp data — run the Recover step first")?;
        result
            .messages
            .iter()
            .map(|m| case_file::TimelineEvent {
                id: 0,
                case_id,
                timestamp: m.timestamp.unwrap_or(0),
                kind: "whatsapp_message".to_string(),
                title: format!(
                    "WhatsApp · {}",
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

/// Milliseconds-since-epoch → simple local-ish timestamp string without extra crates.
fn chrono_fmt(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    // Civil-from-days algorithm (Howard Hinnant) — UTC.
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            last_result: Mutex::new(None),
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
            device_screenshot,
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
            export_result
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
