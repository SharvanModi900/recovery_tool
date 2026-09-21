//! Security audit forensic commands.

use recovery_core::adb;
use crate::run_blocking;

#[tauri::command]
pub fn audit_permissions_pro(serial: String) -> Result<String, String> {
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

// #40 Intent Fuzzer â€” probe exported components with malformed intents

#[tauri::command]
pub fn fuzz_intents(serial: String) -> Result<String, String> {
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

// #41 SELinux Policy Analyzer â€” check enforcement status and permissive domains

#[tauri::command]
pub fn analyze_selinux(serial: String) -> Result<String, String> {
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

// #42 Content Provider Enumerator â€” probe content:// URIs for data leaks

#[tauri::command]
pub fn enumerate_content_providers(serial: String) -> Result<String, String> {
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

// #43 Privilege Escalation Tester â€” check if shell UID can access restricted APIs

#[tauri::command]
pub fn test_privilege_escalation(serial: String) -> Result<String, String> {
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

// #44 Secure Element Probe â€” test NFC/HCE isolation and secure storage

#[tauri::command]
pub fn probe_secure_element(serial: String) -> Result<String, String> {
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

// #45 Bootloader / Verified Boot Checker â€” dm-verity, AVB, lock status

#[tauri::command]
pub fn check_verified_boot(serial: String) -> Result<String, String> {
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

// #46 Kernel Exploit Detector â€” check kernel config, /proc exposure, known vulns

#[tauri::command]
pub fn detect_kernel_exploits(serial: String) -> Result<String, String> {
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
