//! Deep analysis forensic commands.

use recovery_core::adb;
use crate::run_blocking;

/// Detect anti-forensics: app wipers, cache cleaners, evidence of tampering.
#[tauri::command]
pub fn detect_anti_forensics(serial: String) -> Result<String, String> {
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
#[tauri::command]
pub fn extract_social_media(serial: String, output_dir: String) -> Result<String, String> {
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

// #26 Deleted Photo Recovery â€” thumbnail carving

#[tauri::command]
pub fn extract_keyboard_dictionary(serial: String) -> Result<String, String> {
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

// #28 Clipboard Forensics â€” deep clipboard analysis

#[tauri::command]
pub fn extract_clipboard_forensics(serial: String) -> Result<String, String> {
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

// #29 USB Connection Forensics

#[tauri::command]
pub fn extract_power_forensics(serial: String) -> Result<String, String> {
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

// #31 Notification History

#[tauri::command]
pub fn extract_notification_history(serial: String) -> Result<String, String> {
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

// #32 Screen Time / Digital Wellbeing

#[tauri::command]
pub fn extract_screen_time(serial: String) -> Result<String, String> {
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

// #33 SQLite WAL/Journal Carving Engine

#[tauri::command]
pub fn carve_sqlite_wal(serial: String, output_dir: String) -> Result<String, String> {
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

// #34 App Sandbox Extraction via run-as

#[tauri::command]
pub fn extract_app_sandbox(serial: String) -> Result<String, String> {
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

// #35 Hidden Partition Scanner

#[tauri::command]
pub fn scan_hidden_partitions(serial: String) -> Result<String, String> {
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

// #36 Network Traffic Snapshot

#[tauri::command]
pub fn extract_sensor_data(serial: String) -> Result<String, String> {
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

// #38 Full File System Manifest

#[tauri::command]
pub fn generate_file_manifest(serial: String, output_dir: String) -> Result<String, String> {
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

// #39 Permission Auditor Pro â€” find apps with dangerous/overprivileged permissions
