//! Device identity, lock info, dual SIM, and timezone forensic commands.

use recovery_core::adb;
use crate::run_blocking;

/// Collect device identity: IMEI, ESN, MEID, serial number, model, brand, etc.
/// Uses multiple ADB methods for maximum compatibility.
#[tauri::command]
pub fn collect_device_identity(serial: String) -> Result<String, String> {
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
pub fn extract_lock_info(serial: String) -> Result<String, String> {
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

/// Parses raw Android properties into a clean forensic report.
#[tauri::command]
pub fn collect_dual_sim_info(serial: String) -> Result<String, String> {
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
pub fn collect_timezone_fingerprint(serial: String) -> Result<String, String> {
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

