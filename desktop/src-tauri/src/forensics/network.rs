//! Network & connectivity forensic commands.

use recovery_core::adb;
use crate::run_blocking;

/// Parses raw dumpsys output into a clean, structured forensic report.
#[tauri::command]
pub fn collect_cell_tower_history(serial: String) -> Result<String, String> {
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


#[tauri::command]
pub fn collect_gps_timeline(serial: String) -> Result<String, String> {
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

/// Get Bluetooth paired devices and connection history.
#[tauri::command]
pub fn get_bluetooth_forensics(serial: String) -> Result<String, String> {
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
pub fn get_network_connections(serial: String) -> Result<String, String> {
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
pub fn get_clipboard_history(serial: String, count: Option<u32>) -> Result<String, String> {
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

// #21 Recover deleted SMS/MMS messages from mmssms.db (advanced)

#[tauri::command]
pub fn extract_wifi_history(serial: String) -> Result<String, String> {
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

// #24 Bluetooth Device Forensics â€” paired devices, MAC addresses

#[tauri::command]
pub fn extract_bluetooth_devices(serial: String) -> Result<String, String> {
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

// #25 Social Media Extraction â€” Instagram, Telegram, Signal, Snapchat

#[tauri::command]
pub fn extract_usb_connections(serial: String) -> Result<String, String> {
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

// #30 Power/Battery Forensics

#[tauri::command]
pub fn extract_network_traffic(serial: String) -> Result<String, String> {
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

// #37 Sensor Data Analysis
