//! Read-only launcher-icon extraction straight from on-device APKs.
//!
//! An APK is a ZIP file. Instead of pulling the whole APK (100 MB+ for big
//! apps) this module reads only three tiny byte ranges over USB:
//!   1. the file tail — to find the ZIP central directory,
//!   2. the central directory — to locate the best `ic_launcher` bitmap,
//!   3. that one compressed entry — then inflates it locally.
//!
//! Everything runs through read-only `pm` / `tail` / `head` / `stat` queries,
//! so the phone is never modified.

use flate2::read::DeflateDecoder;
use recovery_core::adb::AdbClient;
use std::io::Read;

pub struct Icon {
    pub bytes: Vec<u8>,
    pub ext: &'static str,
}

const EOCD_SIG: u32 = 0x0605_4b50;
const CD_SIG: u32 = 0x0201_4b50;

/// Range-read `len` bytes starting at absolute `offset` from a device file.
/// Uses `tail -c +<offset+1> | head -c <len>` (both seek on regular files,
/// so this never scans the whole APK).
fn read_range(adb: &AdbClient, serial: &str, path: &str, offset: u64, len: u64) -> Result<Vec<u8>, String> {
    let cmd = format!("tail -c +{} < '{path}' | head -c {len}", offset + 1);
    adb.exec_out(serial, &["sh", "-c", &cmd])
        .map_err(|e| format!("cannot read the APK ({e})"))
}

fn u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}
fn u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/// Higher score = better launcher icon candidate; `None` = not an icon.
fn icon_score(name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    if !n.starts_with("res/") {
        return None;
    }
    let ext_ok = n.ends_with(".png") || n.ends_with(".webp") || n.ends_with(".jpg");
    if !ext_ok {
        return None;
    }
    let file = n.rsplit('/').next().unwrap_or("");
    if !file.starts_with("ic_launcher") {
        return None;
    }
    // Prefer the standard square icon at the highest density.
    let mut score = if n.contains("xxxhdpi") {
        90
    } else if n.contains("xxhdpi") {
        80
    } else if n.contains("xhdpi") {
        70
    } else if n.contains("hdpi") {
        60
    } else if n.contains("mdpi") {
        50
    } else {
        40
    };
    if file.contains("round") {
        score -= 15;
    }
    if file.contains("foreground") || file.contains("background") || file.contains("monochrome") {
        return None; // adaptive-icon layers, not a full icon
    }
    Some(score)
}

fn ext_of(name: &str) -> &'static str {
    let n = name.to_ascii_lowercase();
    if n.ends_with(".webp") {
        "webp"
    } else if n.ends_with(".jpg") {
        "jpg"
    } else {
        "png"
    }
}

/// Extract the launcher icon of `package` from its on-device APK.
pub fn fetch_icon(adb: &AdbClient, serial: &str, package: &str) -> Result<Icon, String> {
    // 1. Locate the base APK (`pm path` also lists config/language splits).
    let out = adb
        .exec_out(serial, &["pm", "path", package])
        .map_err(|e| e.to_string())?;
    let mut paths: Vec<String> = String::from_utf8_lossy(&out)
        .lines()
        .filter_map(|l| l.trim().strip_prefix("package:").map(|p| p.trim().to_string()))
        .filter(|p| p.ends_with(".apk"))
        .collect();
    if paths.is_empty() {
        return Err("no APK found for this package".into());
    }
    // The icon usually lives in base.apk, but density splits can carry it;
    // language/arch splits never do.
    let mut paths: Vec<String> = paths
        .into_iter()
        .filter(|p| !p.contains("i18n") && !p.contains("arm") && !p.contains("x86"))
        .collect();
    if let Some(i) = paths.iter().position(|p| p.ends_with("base.apk")) {
        paths.swap(0, i);
    }
    // Density splits are the most likely fallback carriers.
    paths.sort_by_key(|p| if p.contains("dpi") { 0 } else { 1 });
    let mut last_err = "no bitmap launcher icon in any APK split".to_string();
    for apk in &paths {
        match fetch_icon_from_apk(adb, serial, apk) {
            Ok(icon) => return Ok(icon),
            Err(e) => last_err = e,
        }
    }
    Err(last_err)
}

/// Extract the launcher icon from a single on-device APK file.
fn fetch_icon_from_apk(adb: &AdbClient, serial: &str, apk: &str) -> Result<Icon, String> {
    // File tail → EOCD → central directory offset.
    let tail = adb
        .exec_out(serial, &["tail", "-c", "66000", apk])
        .map_err(|e| format!("cannot read the APK ({e})"))?;
    if tail.len() < 22 {
        return Err("APK is too small to be a valid package".into());
    }
    let sig = EOCD_SIG.to_le_bytes();
    let eocd_pos = tail
        .windows(4)
        .rposition(|w| w == sig)
        .ok_or("ZIP end-of-directory not found in the APK")?;
    if tail.len() < eocd_pos + 22 {
        return Err("corrupt ZIP trailer in the APK".into());
    }
    let cd_off = u32(&tail, eocd_pos + 16) as u64;
    let cd_len = u32(&tail, eocd_pos + 12) as u64;
    if cd_off == 0 && cd_len == 0 {
        return Err("ZIP64 APKs are not supported".into());
    }

    // 3. Central directory → pick the best launcher bitmap.
    let cd = read_range(adb, serial, apk, cd_off, cd_len)?;
    let mut best: Option<(u32, String, u16, u64)> = None; // (score, name, method, local offset)
    let mut i = 0usize;
    while i + 46 <= cd.len() {
        if u32(&cd, i) != CD_SIG {
            break;
        }
        let method = u16(&cd, i + 10);
        let name_len = u16(&cd, i + 28) as usize;
        let extra_len = u16(&cd, i + 30) as usize;
        let comment_len = u16(&cd, i + 32) as usize;
        let local_off = u32(&cd, i + 42) as u64;
        if i + 46 + name_len > cd.len() {
            break;
        }
        let name = String::from_utf8_lossy(&cd[i + 46..i + 46 + name_len]).into_owned();
        if let Some(score) = icon_score(&name) {
            if best.as_ref().map(|b| score > b.0).unwrap_or(true) {
                best = Some((score, name, method, local_off));
            }
        }
        i += 46 + name_len + extra_len + comment_len;
    }
    let (_, icon_name, method, local_off) =
        best.ok_or("app ships only an adaptive/vector icon — no bitmap to show")?;

    // 4. Local header → compressed bytes → inflate locally.
    let lh = read_range(adb, serial, apk, local_off, 4096)?;
    if lh.len() < 30 {
        return Err("cannot read the icon entry header".into());
    }
    let lh_name_len = u16(&lh, 26) as usize;
    let lh_extra_len = u16(&lh, 28) as usize;
    let data_off = local_off + 30 + (lh_name_len as u64) + (lh_extra_len as u64);
    // entry sizes live in the matching central directory record; re-read them
    let comp = {
        // find comp_size again from the CD entry of the chosen icon
        let mut comp = 0u64;
        let mut j = 0usize;
        while j + 46 <= cd.len() {
            if u32(&cd, j) != CD_SIG {
                break;
            }
            let nl = u16(&cd, j + 28) as usize;
            let el = u16(&cd, j + 30) as usize;
            let cl = u16(&cd, j + 32) as usize;
            if j + 46 + nl <= cd.len() {
                let nm = String::from_utf8_lossy(&cd[j + 46..j + 46 + nl]);
                if nm == icon_name {
                    comp = u32(&cd, j + 20) as u64;
                    break;
                }
            }
            j += 46 + nl + el + cl;
        }
        comp
    };
    if comp == 0 {
        return Err("icon entry is empty".into());
    }
    let raw = read_range(adb, serial, apk, data_off, comp)?;
    let bytes = if method == 0 {
        raw
    } else if method == 8 {
        let mut out = Vec::new();
        DeflateDecoder::new(&raw[..])
            .read_to_end(&mut out)
            .map_err(|e| format!("icon inflate failed: {e}"))?;
        out
    } else {
        return Err(format!("unsupported ZIP compression method {method}"));
    };
    if bytes.len() < 100 {
        return Err("decoded icon is empty".into());
    }
    Ok(Icon {
        bytes,
        ext: ext_of(&icon_name),
    })
}
