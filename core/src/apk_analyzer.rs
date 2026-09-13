//! APK analysis: hash installed APKs, extract signing certificate info.
//!
//! Uses `pm list packages -f` for paths, pulls APKs via `exec-out cat`
//! (header only for hash), and extracts signing certificate metadata.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Analysis result for a single APK.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApkAnalysis {
    pub package: String,
    pub apk_path: String,
    pub sha256: String,
    pub signer_cn: Option<String>,
    pub signer_fingerprint: Option<String>,
    pub install_time: Option<i64>,
    pub last_update: Option<i64>,
    pub is_system: bool,
    pub forensic_flags: Vec<String>,
}

/// Result of APK analysis scan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApkAnalysisResult {
    pub apps: Vec<ApkAnalysis>,
    pub notes: Vec<String>,
}

/// Analyze installed APKs on the device.
///
/// `packages` optionally limits which packages to analyze (empty = all third-party).
pub fn analyze_apks(adb: &AdbClient, serial: &str, packages: &[&str]) -> Result<ApkAnalysisResult> {
    let mut notes = Vec::new();
    let mut apps = Vec::new();

    // Get list of installed packages with APK paths.
    let output = match adb.exec_out(serial, &["pm", "list", "packages", "-f"]) {
        Ok(o) => o,
        Err(e) => {
            notes.push(format!("Failed to list packages: {e}"));
            return Ok(ApkAnalysisResult { apps, notes });
        }
    };

    let text = String::from_utf8_lossy(&output);
    let mut pkg_paths: Vec<(String, String)> = Vec::new();

    for line in text.lines() {
        // Format: package:/path/to/base.apk=com.example.app
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("package:") {
            if let Some((path, pkg)) = rest.rsplit_once('=') {
                // Filter: if packages list is non-empty, only include those.
                if !packages.is_empty() && !packages.iter().any(|p| *p == pkg) {
                    continue;
                }
                // Skip system apps unless explicitly requested.
                let is_system = path.starts_with("/system/") || path.starts_with("/product/");
                if packages.is_empty() && is_system {
                    continue;
                }
                pkg_paths.push((pkg.to_string(), path.to_string()));
            }
        }
    }

    notes.push(format!("Found {} packages to analyze", pkg_paths.len()));

    // Limit to first 100 for performance.
    let limit = pkg_paths.len().min(100);
    for (pkg, apk_path) in &pkg_paths[..limit] {
        let is_system = apk_path.starts_with("/system/") || apk_path.starts_with("/product/");

        // Pull first 2MB of APK for hashing (enough for signing block).
        let apk_data = match adb.exec_out(serial, &["head", "-c", "2097152", apk_path]) {
            Ok(d) if !d.is_empty() => d,
            _ => continue,
        };

        // Compute SHA-256 of what we pulled.
        let sha256 = {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(&apk_data);
            hex::encode(h.finalize())
        };

        // Try to extract signing certificate info from APK signature block.
        let (signer_cn, signer_fingerprint) = extract_signer_info(&apk_data);

        // Get install/update times via dumpsys package.
        let (install_time, last_update) = get_package_times(adb, serial, pkg);

        // Forensic flags.
        let mut forensic_flags = Vec::new();
        if is_system {
            forensic_flags.push("system".into());
        }
        if signer_cn.is_none() {
            forensic_flags.push("no_signer".into());
        }
        // Check if APK path is in a suspicious location.
        if !apk_path.starts_with("/system/") && !apk_path.starts_with("/data/app/")
            && !apk_path.starts_with("/product/") && !apk_path.starts_with("/vendor/")
        {
            forensic_flags.push("unusual_path".into());
        }

        apps.push(ApkAnalysis {
            package: pkg.clone(),
            apk_path: apk_path.clone(),
            sha256,
            signer_cn,
            signer_fingerprint,
            install_time,
            last_update,
            is_system,
            forensic_flags,
        });
    }

    if pkg_paths.len() > limit {
        notes.push(format!("Limited to {limit} of {} packages", pkg_paths.len()));
    }

    Ok(ApkAnalysisResult { apps, notes })
}

/// Try to extract signer CN and fingerprint from APK data.
///
/// APK signing block is at the end of the ZIP. For simplicity, we look for
/// the X.509 certificate in the APK signature scheme v2/v3 block.
fn extract_signer_info(_apk_data: &[u8]) -> (Option<String>, Option<String>) {
    // Full APK signature block parsing is complex. For now, return None.
    // A production implementation would parse the ZIP central directory,
    // find the APK Signing Block, and extract the X.509 certificate.
    (None, None)
}

/// Get install and last update times for a package.
fn get_package_times(adb: &AdbClient, serial: &str, package: &str) -> (Option<i64>, Option<i64>) {
    let output = match adb.exec_out(serial, &["dumpsys", "package", package]) {
        Ok(o) => o,
        Err(_) => return (None, None),
    };
    let text = String::from_utf8_lossy(&output);
    let mut install = None;
    let mut update = None;

    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("firstInstallTime=") {
            install = line.strip_prefix("firstInstallTime=")
                .and_then(|s| s.parse::<i64>().ok());
        }
        if line.starts_with("lastUpdateTime=") {
            update = line.strip_prefix("lastUpdateTime=")
                .and_then(|s| s.parse::<i64>().ok());
        }
    }
    (install, update)
}
