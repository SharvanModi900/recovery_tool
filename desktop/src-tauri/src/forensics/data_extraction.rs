//! Data extraction forensic commands.

use recovery_core::adb;
use crate::run_blocking;
use crate::humantime_format;
use tauri::Emitter;

/// Recover deleted contacts by parsing contacts2.db WAL and journal files.
/// Pulls the database from the device and scans for deleted rows.
#[tauri::command]
pub fn recover_deleted_contacts(serial: String, output_dir: String) -> Result<String, String> {
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

/// Analyze device storage: parsed breakdown of what's using space.
#[tauri::command]
pub fn analyze_storage(serial: String) -> Result<String, String> {
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
pub fn recover_voice_memos(serial: String, output_dir: String) -> Result<String, String> {
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
pub fn extract_saved_passwords(serial: String) -> Result<String, String> {
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
pub fn extract_whatsapp_chats(serial: String, output_dir: String) -> Result<String, String> {
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
pub fn extract_email_accounts(serial: String) -> Result<String, String> {
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
pub fn extract_browser_history(serial: String) -> Result<String, String> {
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
pub fn extract_video_metadata(serial: String) -> Result<String, String> {
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
pub fn extract_app_timeline(serial: String) -> Result<String, String> {
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
pub fn extract_cloud_accounts(serial: String) -> Result<String, String> {
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
pub fn analyze_encrypted_backups(serial: String) -> Result<String, String> {
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
pub fn generate_pdf_report(serial: String, output_dir: String, case_id: String) -> Result<String, String> {
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
pub async fn start_logcat(serial: String, app_handle: tauri::AppHandle) -> Result<String, String> {
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
pub fn get_logcat_snapshot(serial: String, lines: Option<u32>) -> Result<String, String> {
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
pub fn global_keyword_search(data_dir: String, keyword: String) -> Result<String, String> {
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
pub fn get_usage_timeline(serial: String) -> Result<String, String> {
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
pub fn generate_forensic_report(case_id: String, device_serial: String, data_dir: String, output_dir: String) -> Result<String, String> {
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
pub fn audit_app_permissions(serial: String) -> Result<String, String> {
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
pub fn detect_spyware(serial: String) -> Result<String, String> {
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
pub fn detect_linked_devices(serial: String) -> Result<String, String> {
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

#[tauri::command]
pub fn recover_deleted_sms_advanced(serial: String, output_dir: String) -> Result<String, String> {
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

// #22 Call Log Forensics â€” extract and parse calls.db

#[tauri::command]
pub fn extract_call_logs(serial: String) -> Result<String, String> {
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

// #23 WiFi Connection History â€” every network ever connected

#[tauri::command]
pub fn recover_deleted_photos(serial: String, output_dir: String) -> Result<String, String> {
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

// #27 Keyboard Dictionary Recovery
