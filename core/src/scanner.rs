//! Artifact discovery: probe WhatsApp locations on a connected device
//! (read-only) or inside a locally imported backup folder, then optionally
//! pull device artifacts into a local workspace.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactKind {
    Database,
    Backup,
    Media,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// Device path (`/sdcard/...`) or local path depending on `ScanReport::source`.
    pub path: String,
    pub size: u64,
    pub kind: ArtifactKind,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ScanReport {
    /// Device serial or the imported folder path.
    pub source: String,
    pub whatsapp_found: bool,
    pub artifacts: Vec<Artifact>,
    pub notes: Vec<String>,
}

/// WhatsApp locations probed on-device (read-only listings).
pub const DEVICE_PROBE_PATHS: &[&str] = &[
    // Android 11+ scoped storage layout
    "/sdcard/Android/media/com.whatsapp/WhatsApp/Databases",
    "/sdcard/Android/media/com.whatsapp/WhatsApp/Backups",
    "/sdcard/Android/media/com.whatsapp/WhatsApp/Media",
    // Legacy layout
    "/sdcard/WhatsApp/Databases",
    "/sdcard/WhatsApp/Backups",
    "/sdcard/WhatsApp/Media",
];

/// Max individual file size pulled by default (skip huge media).
pub const DEFAULT_PULL_SIZE_CAP: u64 = 512 * 1024 * 1024;

/// Image/video extensions considered recoverable media.
const IMAGE_EXTS: &[&str] = &[".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];
const VIDEO_EXTS: &[&str] = &[".mp4", ".3gp", ".webm", ".mkv", ".avi", ".mov"];

/// Returns `"image"` or `"video"` when `name` is a recoverable media file.
pub fn media_kind_of(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    if IMAGE_EXTS.iter().any(|e| n.ends_with(e)) {
        Some("image")
    } else if VIDEO_EXTS.iter().any(|e| n.ends_with(e)) {
        Some("video")
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFile {
    /// Device path (`/sdcard/...`) or local path depending on the source.
    pub path: String,
    pub size: u64,
    /// `image` or `video`.
    pub kind: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MediaRecovery {
    /// Media files available locally after the pull.
    pub files: Vec<MediaFile>,
    pub pulled_dirs: usize,
    pub failed_dirs: usize,
}

/// Device WhatsApp media roots probed for image/video recovery.
pub const DEVICE_MEDIA_ROOTS: &[&str] = &[
    "/sdcard/Android/media/com.whatsapp/WhatsApp/Media",
    "/sdcard/WhatsApp/Media",
];

/// Universal acquisition roots: camera/gallery content from ANY app.
pub const DEVICE_GALLERY_ROOTS: &[&str] = &["/sdcard/DCIM", "/sdcard/Pictures", "/sdcard/Movies"];

/// Downloads & documents saved by any app/browser.
pub const DEVICE_DOWNLOAD_ROOTS: &[&str] = &["/sdcard/Download", "/sdcard/Documents"];

/// Recursively list images/videos inside the device WhatsApp media trees
/// (read-only: only `ls -laR` is used).
pub fn scan_media_device(adb: &AdbClient, serial: &str) -> Result<Vec<MediaFile>> {
    scan_media_paths(adb, serial, DEVICE_MEDIA_ROOTS)
}

/// Recursively list images/videos under arbitrary device roots (read-only).
pub fn scan_media_paths(adb: &AdbClient, serial: &str, roots: &[&str]) -> Result<Vec<MediaFile>> {
    const MAX_FILES: usize = 20_000;
    let mut out: Vec<MediaFile> = Vec::new();
    for base in roots {
        let Ok(bytes) = adb.exec_out(serial, &["ls", "-laR", base]) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("No such file") {
            continue;
        }
        // `ls -R` prints a "<dir>:" header before each directory's entries.
        let mut dir = base.to_string();
        for line in text.lines() {
            let t = line.trim_end();
            if t.is_empty() {
                continue;
            }
            if t.ends_with(':') && t.starts_with('/') {
                dir = t[..t.len() - 1].to_string();
                continue;
            }
            let tokens: Vec<&str> = t.split_whitespace().collect();
            if tokens.len() < 8 || tokens[0].starts_with('d') {
                continue;
            }
            let Ok(size) = tokens[4].parse::<u64>() else {
                continue;
            };
            let name = tokens[7..].join(" ");
            if name == "." || name == ".." {
                continue;
            }
            if let Some(kind) = media_kind_of(&name) {
                out.push(MediaFile {
                    path: format!("{dir}/{name}"),
                    size,
                    kind: kind.into(),
                });
                if out.len() >= MAX_FILES {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

/// List images/videos inside a local folder tree (e.g. pulled device data).
pub fn scan_media_folder(root: &Path) -> Vec<MediaFile> {
    const MAX_FILES: usize = 20_000;
    let mut out = Vec::new();
    walk_media(root, 0, &mut out);
    out.truncate(MAX_FILES);
    out
}

fn walk_media(dir: &Path, depth: usize, out: &mut Vec<MediaFile>) {
    if depth > 10 || out.len() >= 20_000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_media(&path, depth + 1, out);
        } else if let Ok(meta) = entry.metadata() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if let Some(kind) = media_kind_of(&name.unwrap_or_default()) {
                out.push(MediaFile {
                    path: path.display().to_string(),
                    size: meta.len(),
                    kind: kind.into(),
                });
            }
        }
    }
}

pub fn classify(name: &str) -> ArtifactKind {
    let n = name.to_lowercase();
    // Check encrypted backups first: "msgstore.db.crypt14" also matches the db prefix.
    if n.contains(".crypt") || n.ends_with(".enc") {
        ArtifactKind::Backup
    } else if n.ends_with(".db") || n.starts_with("msgstore.db") {
        ArtifactKind::Database
    } else if [
        ".jpg", ".jpeg", ".png", ".webp", ".mp4", ".3gp", ".gif", ".mp3", ".opus", ".ogg",
        ".pdf", ".doc", ".docx", ".vcf", ".webm",
    ]
    .iter()
    .any(|e| n.ends_with(e))
    {
        ArtifactKind::Media
    } else {
        ArtifactKind::Other
    }
}

/// Parse one level of `ls -la` output (toybox/bionic format:
/// `perms links owner group size date time name`).
fn parse_ls_listing(text: &str) -> Vec<(String, u64)> {
    let mut entries = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        // perms links owner group size date time name...
        if tokens.len() < 8 {
            continue;
        }
        let Ok(size) = tokens[4].parse::<u64>() else {
            continue;
        };
        let name = tokens[7..].join(" ");
        if name == "." || name == ".." || tokens[0].starts_with('d') {
            continue;
        }
        entries.push((name, size));
    }
    entries
}

/// List WhatsApp artifact locations on a connected device (read-only).
pub fn scan_device(adb: &AdbClient, serial: &str) -> Result<ScanReport> {
    let mut report = ScanReport {
        source: serial.to_string(),
        ..Default::default()
    };
    for base in DEVICE_PROBE_PATHS {
        let listing = adb.exec_out(serial, &["ls", "-la", base]);
        let Ok(bytes) = listing else { continue };
        let text = String::from_utf8_lossy(&bytes);
        if text.contains("No such file") {
            continue;
        }
        let entries = parse_ls_listing(&text);
        if entries.is_empty() {
            continue;
        }
        report.whatsapp_found = true;
        let is_media_dir = base.ends_with("/Media");
        for (name, size) in entries {
            let path = format!("{base}/{name}");
            let kind = classify(&name);
            report.artifacts.push(Artifact { path, size, kind });
            if is_media_dir {
                // Media subtrees are huge; we only list the top level and
                // record that deeper files exist.
                report.notes.push(format!(
                    "{base}/{name} listed shallowly (media tree not recursed)"
                ));
            }
        }
    }
    if !report.whatsapp_found {
        report.notes.push(
            "no WhatsApp storage locations found; check USB debugging authorization".into(),
        );
    }
    Ok(report)
}

/// Scan a locally imported backup folder (recursive, bounded).
pub fn scan_folder(root: &Path) -> Result<ScanReport> {
    let mut report = ScanReport {
        source: root.display().to_string(),
        ..Default::default()
    };
    walk(root, root, 0, &mut report);
    report.whatsapp_found = report
        .artifacts
        .iter()
        .any(|a| a.kind == ArtifactKind::Database || a.kind == ArtifactKind::Backup);
    if !report.whatsapp_found {
        report.notes.push("no WhatsApp databases/backups found in folder".into());
    }
    Ok(report)
}

fn walk(root: &Path, dir: &Path, depth: usize, report: &mut ScanReport) {
    const MAX_DEPTH: usize = 6;
    const MAX_FILES: usize = 20_000;
    if depth > MAX_DEPTH || report.artifacts.len() >= MAX_FILES {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, depth + 1, report);
        } else if let Ok(meta) = entry.metadata() {
            let rel = path.strip_prefix(root).unwrap_or(&path);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let has_wa_hint = rel
                .components()
                .any(|c| c.as_os_str().to_string_lossy().to_lowercase().contains("whatsapp"));
            let kind = classify(&name.clone().unwrap_or_default());
            if has_wa_hint {
                report.whatsapp_found = true;
            }
            report.artifacts.push(Artifact {
                path: path.display().to_string(),
                size: meta.len(),
                kind,
            });
        }
    }
}

/// Pull discovered device artifacts into `dest`, mirroring device paths.
/// Read-only on the device: only `adb pull` is used.
pub fn pull_artifacts<F>(
    adb: &AdbClient,
    serial: &str,
    artifacts: &[Artifact],
    dest: &Path,
    include_media: bool,
    mut progress: F,
) -> Result<Vec<PathBuf>>
where
    F: FnMut(usize, usize, &str),
{
    let todo: Vec<&Artifact> = artifacts
        .iter()
        .filter(|a| (include_media || a.kind != ArtifactKind::Media) && a.size <= DEFAULT_PULL_SIZE_CAP)
        .collect();
    let total = todo.len();
    let mut pulled = Vec::new();
    for (i, art) in todo.iter().enumerate() {
        progress(i + 1, total, &art.path);
        // Mirror `/sdcard/Android/...` under dest with the leading slash stripped.
        let rel = art.path.trim_start_matches('/');
        let local = dest.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if adb.pull(serial, &art.path, &local).is_ok() {
            pulled.push(local);
        }
    }
    Ok(pulled)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_kinds() {
        assert_eq!(classify("msgstore.db"), ArtifactKind::Database);
        assert_eq!(classify("msgstore.db-2024-01-01.1"), ArtifactKind::Database);
        assert_eq!(classify("msgstore.db.crypt14"), ArtifactKind::Backup);
        assert_eq!(classify("IMG-2024.jpg"), ArtifactKind::Media);
        assert_eq!(classify("notes.txt"), ArtifactKind::Other);
    }

    #[test]
    fn parses_ls_listing() {
        let text = "\
total 1234
drwxrwx--x 2 u0_a1 media_rw 4096 2024-05-14 10:11 .
drwxrwx--x 4 u0_a1 media_rw 4096 2024-05-14 10:11 ..
-rw-rw---- 1 u0_a1 media_rw 156783 2024-05-14 02:30 msgstore.db
-rw-rw---- 1 u0_a1 media_rw 12 2024-05-14 02:30 msgstore.db-wal
";
        let entries = parse_ls_listing(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0, "msgstore.db");
        assert_eq!(entries[0].1, 156783);
    }
}
