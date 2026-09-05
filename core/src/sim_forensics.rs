//! SIM / network forensics — read-only collection of SIM & operator identity,
//! persistent history, baseline comparison and SIM-swap detection.
//!
//! Everything is gathered through the read-only ADB layer (`exec-out` running
//! `getprop`, `dumpsys isub` and `content query`), so it works without root and
//! can never modify the phone. A carrier-side SIM swap cannot be performed over
//! USB, but its *footprint* on the device (a changed ICCID against an unchanged
//! number, a new operator/country, an ABSENT→READY transition) is detectable by
//! comparing the live snapshot against a stored baseline and against the
//! device's own historical subscription records.

use crate::adb::AdbClient;
use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// A point-in-time snapshot of the device's SIM / network identity.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimSnapshot {
    pub id: i64,
    pub timestamp: i64, // Unix ms
    pub device_serial: String,

    // ── SIM identity ────────────────────────────────────────────────
    pub iccid: Option<String>,          // SIM serial number (the physical card)
    pub msisdn: Option<String>,         // phone number
    pub imsi: Option<String>,           // best-effort; usually restricted on Android 10+
    pub carrier_name: Option<String>,   // gsm.sim.operator.alpha
    pub operator_numeric: Option<String>, // gsm.sim.operator.numeric (MCC+MNC)
    pub mcc: Option<String>,
    pub mnc: Option<String>,
    pub country_iso: Option<String>,    // gsm.sim.operator.iso-country
    pub sim_state: Option<String>,      // READY / ABSENT / PIN …
    pub slot_index: Option<i32>,

    // ── Registered network ──────────────────────────────────────────
    pub network_operator: Option<String>, // gsm.operator.alpha
    pub network_type: Option<String>,     // gsm.network.type (LTE/NR/…)
    pub roaming: Option<String>,          // gsm.operator.isroaming

    // ── Modem / device ──────────────────────────────────────────────
    pub baseband: Option<String>, // gsm.version.baseband
    pub imei: Option<String>,     // best-effort

    // ── Integrity / bookkeeping ─────────────────────────────────────
    pub is_baseline: bool,
    pub fingerprint: String, // SHA-256 over the identity fields
}

/// A SIM record seen by the device — from `dumpsys isub` (active + inactive)
/// or the `telephony/siminfo` table. Inactive records are the device's own
/// history of previously-inserted SIMs (direct evidence of past SIM changes).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HistoricalSim {
    pub iccid: Option<String>,
    pub carrier: Option<String>,
    pub msisdn: Option<String>,
    pub slot: Option<i32>,
    pub mcc: Option<String>,
    pub mnc: Option<String>,
    pub active: bool,
}

/// The swap-detection outcome of comparing a snapshot against its baseline.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimSwapVerdict {
    pub risk: String, // "NONE" | "LOW" | "HIGH"
    pub changed: bool,
    pub reasons: Vec<String>,
}

/// Everything the UI needs for the SIM / Network panel in one call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimSwapReport {
    pub snapshot: SimSnapshot,
    pub baseline: Option<SimSnapshot>,
    pub historical: Vec<HistoricalSim>,
    pub verdict: SimSwapVerdict,
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Normalise an ICCID: keep alphanumerics, upper-case.
fn norm_iccid(s: &str) -> Option<String> {
    let v: String = s.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_uppercase();
    if v.len() >= 8 {
        Some(v)
    } else {
        None
    }
}

/// Split a dumpsys/content-query blob into `key=value` pairs. Splits on
/// whitespace and commas so both `dumpsys isub` and `content query` rows parse.
fn kv_pairs(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for tok in text.split(|c: char| c.is_whitespace() || c == ',') {
        if let Some((k, v)) = tok.split_once('=') {
            let k = k.trim_matches(|c| c == '[' || c == ']' || c == '{' || c == '}').trim();
            let v = v.trim_matches(|c| c == '"' || c == '\'' || c == ']' || c == '}' || c == ',').trim();
            if !k.is_empty() {
                out.push((k.to_string(), v.to_string()));
            }
        }
    }
    out
}

/// First non-empty value among the candidate keys (case-insensitive).
fn pick(text: &str, keys: &[&str]) -> Option<String> {
    for (k, v) in kv_pairs(text) {
        for key in keys {
            if k.eq_ignore_ascii_case(key) && !v.is_empty() && !v.eq_ignore_ascii_case("null") {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Derive (MCC, MNC) from an operator-numeric string like `40486` or `404860`.
fn split_mcc_mnc(op: Option<&str>) -> (Option<String>, Option<String>) {
    match op {
        Some(s) if s.len() >= 5 => {
            let digits: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() >= 5 {
                (Some(digits[..3].to_string()), Some(digits[3..].to_string()))
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    }
}

/// Parse `dumpsys isub` into active + inactive subscription records.
/// Uses a locality window (prev/current/next line) so wrapped entries still
/// resolve their fields, and tracks the Active/Inactive section header.
fn parse_isub(text: &str) -> Vec<HistoricalSim> {
    let lines: Vec<&str> = text.lines().collect();
    // Per-line section state: which header last preceded it.
    let mut active_by_line = vec![true; lines.len()];
    let mut cur_active = true;
    for (i, line) in lines.iter().enumerate() {
        let l = line.to_lowercase();
        if l.contains("inactive subscription") || l.contains("inactiv") && l.contains("subscription") {
            cur_active = false;
        } else if l.contains("active subscription") {
            cur_active = true;
        }
        active_by_line[i] = cur_active;
    }

    let mut out: Vec<HistoricalSim> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (i, line) in lines.iter().enumerate() {
        let Some(raw_iccid) = pick(line, &["iccid", "iccId", "icc_id"]) else {
            continue;
        };
        let Some(iccid) = norm_iccid(&raw_iccid) else { continue };
        if !seen.insert(iccid.clone()) {
            continue;
        }
        // Locality window: previous, current and next line joined.
        let lo = i.saturating_sub(1);
        let hi = (i + 1).min(lines.len().saturating_sub(1));
        let window = lines[lo..=hi].join(" ");
        let slot = pick(&window, &["simSlotIndex", "slotIndex", "sim_id", "slot"]).and_then(|v| v.parse::<i32>().ok());
        let carrier = pick(&window, &["displayName", "carrierName", "display_name", "carrier_name"]);
        let msisdn = pick(&window, &["number", "phoneNumber"]);
        let (mcc, mnc) = (
            pick(&window, &["mcc"]),
            pick(&window, &["mnc"]),
        );
        out.push(HistoricalSim {
            iccid: Some(iccid),
            carrier,
            msisdn,
            slot,
            mcc,
            mnc,
            active: active_by_line[i],
        });
    }
    out
}

/// Parse `content query --uri content://telephony/siminfo` rows.
fn parse_siminfo(text: &str) -> Vec<HistoricalSim> {
    let mut out: Vec<HistoricalSim> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for line in text.lines() {
        if !line.contains("Row") && !line.contains("icc_id") && !line.contains("iccid") {
            continue;
        }
        let Some(raw_iccid) = pick(line, &["icc_id", "iccid", "iccId"]) else {
            continue;
        };
        let Some(iccid) = norm_iccid(&raw_iccid) else { continue };
        if !seen.insert(iccid.clone()) {
            continue;
        }
        let slot = pick(line, &["sim_id", "slot_index", "simSlotIndex"]).and_then(|v| v.parse::<i32>().ok());
        out.push(HistoricalSim {
            iccid: Some(iccid),
            carrier: pick(line, &["display_name", "carrier_name", "displayName", "carrierName"]),
            msisdn: pick(line, &["number"]),
            slot,
            mcc: pick(line, &["mcc"]),
            mnc: pick(line, &["mnc"]),
            active: true, // siminfo rows are provisioned subscriptions
        });
    }
    out
}

/// Merge + de-duplicate historical records by ICCID (isub first, then siminfo).
fn merge_historical(isub: Vec<HistoricalSim>, siminfo: Vec<HistoricalSim>) -> Vec<HistoricalSim> {
    let mut out: Vec<HistoricalSim> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for rec in isub.into_iter().chain(siminfo.into_iter()) {
        let Some(iccid) = rec.iccid.clone() else { continue };
        if seen.insert(iccid) {
            out.push(rec);
        }
    }
    // Active records first, then by slot.
    out.sort_by(|a, b| b.active.cmp(&a.active).then(a.slot.cmp(&b.slot)));
    out
}

/// SHA-256 fingerprint over the identity fields (integrity + quick equality).
fn fingerprint(s: &SimSnapshot) -> String {
    let material = format!(
        "{}|{}|{}|{}|{}",
        s.iccid.clone().unwrap_or_default(),
        s.msisdn.clone().unwrap_or_default(),
        s.operator_numeric.clone().unwrap_or_default(),
        s.country_iso.clone().unwrap_or_default().to_uppercase(),
        s.slot_index.map(|v| v.to_string()).unwrap_or_default(),
    );
    let mut h = Sha256::new();
    h.update(material.as_bytes());
    hex::encode(h.finalize())
}

/// Collect a live SIM / network snapshot from the device (read-only).
pub fn collect_sim_snapshot(adb: &AdbClient, serial: &str) -> Result<SimSnapshot> {
    // ── getprop: authoritative for carrier / operator / state / network ──
    let carrier_name = adb.prop(serial, "gsm.sim.operator.alpha").ok().flatten();
    let operator_numeric = adb.prop(serial, "gsm.sim.operator.numeric").ok().flatten();
    let country_iso = adb.prop(serial, "gsm.sim.operator.iso-country").ok().flatten();
    let sim_state = adb.prop(serial, "gsm.sim.state").ok().flatten();
    let network_operator = adb.prop(serial, "gsm.operator.alpha").ok().flatten();
    let network_type = adb.prop(serial, "gsm.network.type").ok().flatten();
    let roaming = adb.prop(serial, "gsm.operator.isroaming").ok().flatten();
    let baseband = adb.prop(serial, "gsm.version.baseband").ok().flatten();

    let (mcc, mnc) = split_mcc_mnc(operator_numeric.as_deref());

    // ── dumpsys isub: ICCID + number + slot, plus historical records ──
    let isub_text = adb
        .exec_out(serial, &["dumpsys", "isub"])
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let isub = parse_isub(&isub_text);

    // ── telephony/siminfo content provider: best-effort structured read ──
    let siminfo_text = adb
        .exec_out(
            serial,
            &[
                "content", "query", "--uri", "content://telephony/siminfo",
                "--projection", "icc_id:number:display_name:carrier_name:mcc:mnc:sim_id",
            ],
        )
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let siminfo = parse_siminfo(&siminfo_text);

    let historical = merge_historical(isub, siminfo);

    // Pick the active record for the current SIM's ICCID / number / slot.
    let active = historical.iter().find(|h| h.active).or_else(|| historical.first());
    let iccid = active.and_then(|h| h.iccid.clone());
    let slot_index = active.and_then(|h| h.slot);
    // Prefer the SIM's own number from the record; fall back to isub text.
    let msisdn = active
        .and_then(|h| h.msisdn.clone())
        .or_else(|| pick(&isub_text, &["number"]))
        .filter(|v| !v.is_empty());

    let mut snap = SimSnapshot {
        id: 0,
        timestamp: now_ms(),
        device_serial: serial.to_string(),
        iccid,
        msisdn,
        imsi: pick(&isub_text, &["imsi"]).filter(|v| v.len() >= 6),
        carrier_name,
        operator_numeric,
        mcc,
        mnc,
        country_iso,
        sim_state,
        slot_index,
        network_operator,
        network_type,
        roaming,
        baseband,
        imei: None,
        is_baseline: false,
        fingerprint: String::new(),
    };
    snap.fingerprint = fingerprint(&snap);
    Ok(snap)
}

/// Compare a snapshot against its baseline (and historical records) → verdict.
pub fn analyze(baseline: Option<&SimSnapshot>, current: &SimSnapshot, historical: &[HistoricalSim]) -> SimSwapVerdict {
    let short = |s: Option<&String>| -> String {
        match s {
            Some(v) if v.len() > 12 => format!("{}…{}", &v[..6], &v[v.len() - 4..]),
            Some(v) => v.clone(),
            None => "—".into(),
        }
    };

    let Some(base) = baseline else {
        // No baseline yet — still surface the device's own multi-SIM history.
        let distinct: HashSet<&str> = historical
            .iter()
            .filter_map(|h| h.iccid.as_deref())
            .collect();
        if distinct.len() > 1 {
            return SimSwapVerdict {
                risk: "LOW".into(),
                changed: true,
                reasons: vec![format!(
                    "No baseline set, but the device records {} different SIM ICCIDs over time — review the history for unexpected changes.",
                    distinct.len()
                )],
            };
        }
        return SimSwapVerdict {
            risk: "NONE".into(),
            changed: false,
            reasons: vec![
                "No baseline set — this snapshot is recorded as the reference. Tap “Set as baseline” to enable swap detection.".into(),
            ],
        };
    };

    let mut reasons: Vec<String> = Vec::new();

    let iccid_changed = matches!((&base.iccid, &current.iccid), (Some(a), Some(b)) if a != b);
    let number_changed = matches!((&base.msisdn, &current.msisdn), (Some(a), Some(b)) if a != b);
    let op_changed = matches!((&base.operator_numeric, &current.operator_numeric), (Some(a), Some(b)) if a != b);
    let country_changed = matches!((&base.country_iso, &current.country_iso), (Some(a), Some(b)) if !a.eq_ignore_ascii_case(b));

    if iccid_changed {
        reasons.push(format!(
            "ICCID changed — the physical SIM was replaced: {} → {}",
            short(base.iccid.as_ref()),
            short(current.iccid.as_ref())
        ));
    }
    if op_changed {
        reasons.push(format!(
            "Carrier/operator changed: {} → {}",
            base.carrier_name.clone().or(base.operator_numeric.clone()).unwrap_or_else(|| "—".into()),
            current.carrier_name.clone().or(current.operator_numeric.clone()).unwrap_or_else(|| "—".into())
        ));
    }
    if country_changed {
        reasons.push(format!(
            "SIM country changed: {} → {}",
            base.country_iso.clone().unwrap_or_else(|| "—".into()).to_uppercase(),
            current.country_iso.clone().unwrap_or_else(|| "—".into()).to_uppercase()
        ));
    }
    if let (Some(bs), Some(cs)) = (&base.sim_state, &current.sim_state) {
        if bs.eq_ignore_ascii_case("READY") && !cs.eq_ignore_ascii_case("READY") {
            reasons.push(format!("SIM state regressed from READY to {}.", cs));
        }
    }

    let risk = if iccid_changed {
        if number_changed {
            reasons.push("Both the SIM (ICCID) and the phone number changed — a different line is now present.".into());
        } else {
            reasons.push("Same phone number now sits on a different physical SIM — classic SIM-swap / SIM-replacement indicator.".into());
        }
        "HIGH"
    } else if op_changed || country_changed {
        "LOW"
    } else {
        "NONE"
    };

    if reasons.is_empty() {
        reasons.push("SIM identity unchanged from the baseline.".into());
    }

    SimSwapVerdict {
        risk: risk.into(),
        changed: iccid_changed || number_changed || op_changed || country_changed,
        reasons,
    }
}

/// Persistent SIM history + baseline store (mirrors `screen_captures.db`).
pub struct SimHistoryDB {
    db_path: PathBuf,
}

impl SimHistoryDB {
    pub fn new(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;
        let db_path = base_dir.join("sim_history.db");
        let db = Connection::open(&db_path)?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS sim_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                device_serial TEXT NOT NULL,
                iccid TEXT, msisdn TEXT, imsi TEXT,
                carrier_name TEXT, operator_numeric TEXT, mcc TEXT, mnc TEXT,
                country_iso TEXT, sim_state TEXT, slot_index INTEGER,
                network_operator TEXT, network_type TEXT, roaming TEXT,
                baseband TEXT, imei TEXT,
                is_baseline INTEGER DEFAULT 0,
                fingerprint TEXT NOT NULL
            )",
            [],
        )?;
        Ok(Self { db_path })
    }

    /// Persist a snapshot; returns it with its assigned `id`.
    pub fn save_snapshot(&self, snap: &SimSnapshot) -> Result<SimSnapshot> {
        let db = Connection::open(&self.db_path)?;
        db.execute(
            "INSERT INTO sim_snapshots (
                timestamp, device_serial, iccid, msisdn, imsi, carrier_name,
                operator_numeric, mcc, mnc, country_iso, sim_state, slot_index,
                network_operator, network_type, roaming, baseband, imei,
                is_baseline, fingerprint
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
            params![
                snap.timestamp,
                snap.device_serial,
                snap.iccid,
                snap.msisdn,
                snap.imsi,
                snap.carrier_name,
                snap.operator_numeric,
                snap.mcc,
                snap.mnc,
                snap.country_iso,
                snap.sim_state,
                snap.slot_index,
                snap.network_operator,
                snap.network_type,
                snap.roaming,
                snap.baseband,
                snap.imei,
                snap.is_baseline as i32,
                snap.fingerprint,
            ],
        )?;
        let mut saved = snap.clone();
        saved.id = db.last_insert_rowid();
        Ok(saved)
    }

    fn row_to_snapshot(row: &rusqlite::Row) -> rusqlite::Result<SimSnapshot> {
        Ok(SimSnapshot {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            device_serial: row.get(2)?,
            iccid: row.get(3)?,
            msisdn: row.get(4)?,
            imsi: row.get(5)?,
            carrier_name: row.get(6)?,
            operator_numeric: row.get(7)?,
            mcc: row.get(8)?,
            mnc: row.get(9)?,
            country_iso: row.get(10)?,
            sim_state: row.get(11)?,
            slot_index: row.get(12)?,
            network_operator: row.get(13)?,
            network_type: row.get(14)?,
            roaming: row.get(15)?,
            baseband: row.get(16)?,
            imei: row.get(17)?,
            is_baseline: row.get::<_, i32>(18).unwrap_or(0) != 0,
            fingerprint: row.get(19)?,
        })
    }

    const COLS: &'static str = "id, timestamp, device_serial, iccid, msisdn, imsi, carrier_name, \
        operator_numeric, mcc, mnc, country_iso, sim_state, slot_index, network_operator, \
        network_type, roaming, baseband, imei, is_baseline, fingerprint";

    /// Snapshot history, newest first.
    pub fn get_history(&self, serial: Option<&str>, limit: usize) -> Result<Vec<SimSnapshot>> {
        let db = Connection::open(&self.db_path)?;
        let snaps = if let Some(s) = serial {
            let sql = format!("SELECT {} FROM sim_snapshots WHERE device_serial = ?1 ORDER BY timestamp DESC LIMIT ?2", Self::COLS);
            let mut stmt = db.prepare(&sql)?;
            let rows = stmt.query_map(params![s, limit as i64], Self::row_to_snapshot)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let sql = format!("SELECT {} FROM sim_snapshots ORDER BY timestamp DESC LIMIT ?1", Self::COLS);
            let mut stmt = db.prepare(&sql)?;
            let rows = stmt.query_map(params![limit as i64], Self::row_to_snapshot)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        Ok(snaps)
    }

    /// The baseline snapshot for a device (most recent flagged one).
    pub fn get_baseline(&self, serial: &str) -> Result<Option<SimSnapshot>> {
        let db = Connection::open(&self.db_path)?;
        let sql = format!(
            "SELECT {} FROM sim_snapshots WHERE device_serial = ?1 AND is_baseline = 1 ORDER BY timestamp DESC LIMIT 1",
            Self::COLS
        );
        let mut stmt = db.prepare(&sql)?;
        let mut rows = stmt.query_map(params![serial], Self::row_to_snapshot)?;
        Ok(rows.next().transpose()?)
    }

    /// Flag `id` as the baseline for its device (clearing any previous flag).
    pub fn set_baseline(&self, id: i64) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        let serial: Option<String> = db
            .query_row("SELECT device_serial FROM sim_snapshots WHERE id = ?1", params![id], |r| r.get(0))
            .ok();
        if let Some(s) = serial {
            db.execute(
                "UPDATE sim_snapshots SET is_baseline = 0 WHERE device_serial = ?1",
                params![s],
            )?;
        }
        db.execute("UPDATE sim_snapshots SET is_baseline = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_snapshot(&self, id: i64) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        db.execute("DELETE FROM sim_snapshots WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Collect a live snapshot, persist it, and produce a full swap report
/// (current snapshot + baseline + device historical records + verdict).
pub fn check_sim_swap(adb: &AdbClient, serial: &str, db: &SimHistoryDB) -> Result<SimSwapReport> {
    let snapshot = db.save_snapshot(&collect_sim_snapshot(adb, serial)?)?;

    // Re-read historical records for the verdict (cheap, read-only).
    let isub_text = adb
        .exec_out(serial, &["dumpsys", "isub"])
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let siminfo_text = adb
        .exec_out(
            serial,
            &[
                "content", "query", "--uri", "content://telephony/siminfo",
                "--projection", "icc_id:number:display_name:carrier_name:mcc:mnc:sim_id",
            ],
        )
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let historical = merge_historical(parse_isub(&isub_text), parse_siminfo(&siminfo_text));

    let baseline = db.get_baseline(serial)?;
    let verdict = analyze(baseline.as_ref(), &snapshot, &historical);

    Ok(SimSwapReport {
        snapshot,
        baseline,
        historical,
        verdict,
    })
}
