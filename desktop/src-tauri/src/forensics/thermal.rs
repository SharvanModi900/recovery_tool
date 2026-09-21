//! Thermal analysis commands â€” temperature reading and CPU stress testing.

use recovery_core::adb;
use crate::run_blocking;

/// Get device temperature readings from all thermal zones.
#[tauri::command]
pub fn get_device_temperatures(serial: String) -> Result<String, String> {
    run_blocking(move || {
        let adb = adb::AdbClient::default();
        let mut result = String::new();
        let batt_out = adb.shell(&serial, &["dumpsys", "battery"]).map_err(|e| e.to_string())?;
        let batt_text = String::from_utf8_lossy(&batt_out);
        for line in batt_text.lines() {
            if line.contains("temperature") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if let Some(val) = parts.last() {
                    if let Ok(t) = val.parse::<f64>() { result.push_str(&format!("Battery: {:.1}Â°C\n", t / 10.0)); }
                }
            }
        }
        let svc_out = adb.shell(&serial, &["dumpsys", "thermalservice"]).map_err(|e| e.to_string())?;
        let svc_text = String::from_utf8_lossy(&svc_out);
        for line in svc_text.lines() {
            let l = line.trim();
            if l.contains("mStatus") || l.contains("Status:") { result.push_str(&format!("{}\n", l)); }
            if l.contains("{t:") || l.contains("type:") {
                if let Some(ts) = l.find("t:") {
                    let tr = &l[ts + 2..];
                    if let Some(te) = tr.find(|c: char| c == ',' || c == '}') {
                        let st = tr[..te].trim();
                        if let Some(vs) = l.find("v:") {
                            let vr = &l[vs + 2..];
                            if let Some(ve) = vr.find(|c: char| c == ',' || c == '}') {
                                if let Ok(v) = vr[..ve].trim().parse::<f64>() { result.push_str(&format!("{}: {:.1}Â°C\n", st, v)); }
                            }
                        }
                    }
                }
            }
        }
        let thermal_out = adb.shell(&serial, &["ls", "/sys/class/thermal/"]);
        if let Ok(thermal_bytes) = thermal_out {
            let thermal_text = String::from_utf8_lossy(&thermal_bytes);
            if !thermal_text.contains("Permission denied") {
                for entry in thermal_text.lines() {
                    if entry.starts_with("thermal_zone") {
                        let zone_path = format!("/sys/class/thermal/{}", entry);
                        if let Ok(temp_bytes) = adb.shell(&serial, &["cat", &format!("{}/temp", zone_path)]) {
                            let temp_str = String::from_utf8_lossy(&temp_bytes).trim().to_string();
                            if let Ok(milli_c) = temp_str.parse::<f64>() {
                                let zone_type = adb.shell(&serial, &["cat", &format!("{}/type", zone_path)]).map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
                                result.push_str(&format!("{}: {:.1}Â°C\n", zone_type, milli_c / 1000.0));
                            }
                        }
                    }
                }
            }
        }
        if result.is_empty() { result = "No temperature data available".to_string(); }
        Ok(result.trim().to_string())
    })?
}

/// Run a thermal stress test: load CPU on-device, monitor temperature rise and throttling.
#[tauri::command]
pub async fn run_thermal_stress_test(serial: String, duration_secs: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let adb = adb::AdbClient::default();
        let dur = duration_secs.min(120).max(5);
        let mut report = String::from("â•â•â• Thermal Stress Test â•â•â•\n\n");
        let read_batt = |adb: &adb::AdbClient, s: &str| -> Option<f64> {
            if let Ok(out) = adb.shell(s, &["dumpsys", "battery"]) {
                let t = String::from_utf8_lossy(&out);
                for line in t.lines() { if line.contains("temperature") { if let Some(v) = line.split_whitespace().last() { if let Ok(f) = v.parse::<f64>() { return Some(f / 10.0); } } } }
            }
            None
        };
        let read_sensors = |adb: &adb::AdbClient, s: &str| -> Vec<(String, f64)> {
            let mut v = Vec::new();
            if let Ok(out) = adb.shell(s, &["dumpsys", "thermalservice"]) {
                for line in String::from_utf8_lossy(&out).lines() {
                    let l = line.trim();
                    if l.contains("{t:") { if let Some(ts) = l.find("t:") { let tr = &l[ts+2..]; if let Some(te) = tr.find(|c: char| c==','||c=='}') { let st=tr[..te].trim().to_string(); if let Some(vs)=l.find("v:") { let vr=&l[vs+2..]; if let Some(ve)=vr.find(|c: char| c==','||c=='}') { if let Ok(f)=vr[..ve].trim().parse::<f64>() { v.push((st,f)); } } } } } }
                }
            }
            v
        };
        let read_freq = |adb: &adb::AdbClient, s: &str| -> Option<String> {
            if let Ok(out) = adb.shell(s, &["cat", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"]) {
                let t = String::from_utf8_lossy(&out).trim().to_string();
                if let Ok(khz) = t.parse::<u64>() { return Some(format!("{:.0} MHz", khz as f64/1000.0)); }
                if !t.is_empty() && t != "Permission denied" { return Some(t); }
            }
            None
        };
        let read_status = |adb: &adb::AdbClient, s: &str| -> String {
            if let Ok(out) = adb.shell(s, &["dumpsys", "thermalservice"]) {
                for line in String::from_utf8_lossy(&out).lines() { let l=line.trim(); if l.contains("mStatus")||l.contains("Status:") { return l.to_string(); } }
            }
            "Unknown".into()
        };
        let read_cores = |adb: &adb::AdbClient, s: &str| -> u32 {
            if let Ok(out) = adb.shell(s, &["nproc"]) { if let Ok(n) = String::from_utf8_lossy(&out).trim().parse::<u32>() { return n; } }
            4
        };

        report.push_str("â”€â”€ Phase 1: Baseline (Before Stress) â”€â”€\n");
        let bl_batt = read_batt(&adb, &serial);
        let bl_sensors = read_sensors(&adb, &serial);
        let bl_freq = read_freq(&adb, &serial);
        let bl_status = read_status(&adb, &serial);
        let cores = read_cores(&adb, &serial);
        if let Some(t) = bl_batt { report.push_str(&format!("  Battery: {:.1}Â°C\n", t)); } else { report.push_str("  Battery: Not available\n"); }
        if !bl_sensors.is_empty() { report.push_str("  Thermal Sensors:\n"); for (n,v) in &bl_sensors { report.push_str(&format!("    {}: {:.1}Â°C\n",n,v)); } }
        report.push_str(&format!("  CPU Freq: {}\n  Cores: {}\n  Status: {}\n", bl_freq.as_deref().unwrap_or("N/A"), cores, bl_status));

        report.push_str(&format!("\nâ”€â”€ Phase 2: CPU Stress ({}s) â”€â”€\n", dur));
        let script = format!("for i in $(seq 1 {}); do (while true; do echo 'scale=10; 4*a(1)' | bc -l 2>/dev/null || awk 'BEGIN{{for(i=0;i<100000;i++)s+=sin(i)}}END{{print s}}' 2>/dev/null; done) & done; dd if=/dev/urandom bs=1M count=200 of=/dev/null 2>/dev/null; sleep {}; kill $(jobs -p) 2>/dev/null; echo STRESS_DONE", cores, dur);
        let _ = adb.shell(&serial, &["sh", "-c", &script]);
        let mut temps: Vec<(u32,Option<f64>)> = Vec::new();
        let mut freqs: Vec<(u32,Option<String>)> = Vec::new();
        let poll = std::cmp::max(dur/10, 3);
        let mut max_t: f64 = bl_batt.unwrap_or(0.0);
        report.push_str(&format!("  Polling every {}s...\n  Time   | Battery Â°C | CPU Freq     | Status\n  -------|------------|--------------|--------\n", poll));
        for el in (0..=dur).step_by(poll as usize) {
            std::thread::sleep(std::time::Duration::from_secs(poll as u64));
            let bt = read_batt(&adb,&serial); let cf = read_freq(&adb,&serial); let st = read_status(&adb,&serial);
            if let Some(t) = bt { if t > max_t { max_t = t; } }
            temps.push((el,bt)); freqs.push((el,cf.clone()));
            let ts = bt.map(|t| format!("{:.1}",t)).unwrap_or_else(|| "N/A".into());
            let fs = cf.as_deref().unwrap_or("N/A");
            let ss = if st.len()>16 { &st[..16] } else { &st };
            report.push_str(&format!("  {:>4}s   | {:>7}Â°C   | {:>12} | {}\n", el, ts, fs, ss));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));

        report.push_str("\nâ”€â”€ Phase 3: Post-Stress â”€â”€\n");
        let p_batt = read_batt(&adb,&serial); let p_sensors = read_sensors(&adb,&serial);
        let p_freq = read_freq(&adb,&serial); let p_status = read_status(&adb,&serial);
        if let Some(t) = p_batt { report.push_str(&format!("  Battery: {:.1}Â°C\n", t)); }
        if !p_sensors.is_empty() { report.push_str("  Sensors:\n"); for (n,v) in &p_sensors { report.push_str(&format!("    {}: {:.1}Â°C\n",n,v)); } }
        report.push_str(&format!("  CPU Freq: {}\n  Status: {}\n", p_freq.as_deref().unwrap_or("N/A"), p_status));

        report.push_str("\nâ”€â”€ Summary â”€â”€\n");
        if let (Some(b),Some(p)) = (bl_batt, p_batt) { report.push_str(&format!("  Temp Rise: {:.1}Â°C ({:.1}â†’{:.1}Â°C)\n", p-b, b, p)); }
        let pmhz = |s: &str| -> Option<f64> { s.split_whitespace().next().and_then(|n| n.parse::<f64>().ok()) };
        let bm = bl_freq.as_deref().and_then(|s| pmhz(s)); let pm = p_freq.as_deref().and_then(|s| pmhz(s));
        if let (Some(b),Some(a)) = (bm,pm) { if a < b { report.push_str(&format!("  âš  CPU THROTTLED: {:.0}â†’{:.0} MHz\n", b, a)); } }
        if bl_status != p_status && !bl_status.is_empty() && !p_status.is_empty() { report.push_str(&format!("  Status Change: {} â†’ {}\n", bl_status, p_status)); }
        report.push_str("\nâ”€â”€ Risk â”€â”€\n");
        if max_t >= 50.0 { report.push_str("  âš  CRITICAL: >50Â°C! Risk of thermal shutdown.\n"); }
        else if max_t >= 45.0 { report.push_str("  âš  WARNING: 45-50Â°C. Approaching throttle threshold.\n"); }
        else if max_t >= 40.0 { report.push_str("  MODERATE: 40-45Â°C. Normal for sustained load.\n"); }
        else if max_t > 0.0 { report.push_str(&format!("  SAFE: Peak {:.1}Â°C.\n", max_t)); }
        let model = adb.shell(&serial, &["getprop","ro.product.model"]).map(|b| String::from_utf8_lossy(&b).trim().to_string()).unwrap_or_default();
        if !model.is_empty() { report.push_str(&format!("\n  Device: {}\n", model)); }
        report.push_str(&format!("  Duration: {}s on {} cores\n", dur, cores));
        Ok(report)
    }).await.map_err(|e| e.to_string())?
}
