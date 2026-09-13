//! Deleted SMS/MMS recovery from `mmssms.db`.
//!
//! Pulls the raw SQLite database from the device via ADB `exec-out cat`,
//! parses live rows through rusqlite, then carves deleted records from
//! freeblocks, freelist pages, and WAL frames.

use crate::adb::AdbClient;
use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// SMS/MMS record with source attribution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsRecord {
    pub id: Option<i64>,
    pub address: Option<String>,
    pub body: Option<String>,
    pub msg_type: Option<i64>,
    pub timestamp: Option<i64>,
    pub read: Option<bool>,
    /// "live", "wal", "freelist", or "freeblock"
    pub source: String,
    pub is_deleted: bool,
}

/// Result of deleted SMS recovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeletedSmsResult {
    pub live_count: usize,
    pub deleted_count: usize,
    pub records: Vec<SmsRecord>,
    pub sha256: String,
    pub notes: Vec<String>,
}

/// Known paths for the SMS/MMS database on Android.
const SMS_DB_PATHS: &[&str] = &[
    "/data/data/com.android.providers.telephony/databases/mmssms.db",
    "/data/user/0/com.android.providers.telephony/databases/mmssms.db",
];

/// Pull the mmssms.db from the device and recover both live and deleted SMS.
pub fn recover_deleted_sms(adb: &AdbClient, serial: &str) -> Result<DeletedSmsResult> {
    let mut notes = Vec::new();

    // Try each known path.
    let mut db_data: Option<Vec<u8>> = None;
    let mut used_path = "";
    for path in SMS_DB_PATHS {
        match adb.exec_out(serial, &["cat", path]) {
            Ok(data) if data.len() > 100 => {
                db_data = Some(data);
                used_path = path;
                break;
            }
            Ok(_) => {
                notes.push(format!("{path}: file too small or empty"));
            }
            Err(e) => {
                notes.push(format!("{path}: {e}"));
            }
        }
    }

    let db_data = match db_data {
        Some(d) => d,
        None => {
            notes.push("Could not access mmssms.db — root may be required".into());
            return Ok(DeletedSmsResult {
                live_count: 0,
                deleted_count: 0,
                records: Vec::new(),
                sha256: String::new(),
                notes,
            });
        }
    };

    // Compute integrity hash.
    let hash = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(&db_data);
        hex::encode(h.finalize())
    };

    // Write to temp file.
    let tmp_dir = std::env::temp_dir().join("recoverkit_sms");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let local_db = tmp_dir.join("mmssms.db");
    std::fs::write(&local_db, &db_data).ok();

    // Try to pull WAL too.
    let wal_remote = format!("{used_path}-wal");
    let local_wal = tmp_dir.join("mmssms.db-wal");
    let has_wal = if let Ok(wal_data) = adb.exec_out(serial, &["cat", &wal_remote]) {
        if wal_data.len() > 32 {
            std::fs::write(&local_wal, &wal_data).ok();
            true
        } else {
            false
        }
    } else {
        false
    };

    // Parse live rows.
    let mut records = Vec::new();
    let conn = Connection::open_with_flags(
        &local_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );

    if let Ok(conn) = conn {
        // Read live SMS rows.
        if let Ok(mut stmt) = conn.prepare(
            "SELECT _id, address, body, type, date, read FROM sms ORDER BY date DESC LIMIT 10000"
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(SmsRecord {
                    id: r.get::<_, i64>(0).ok(),
                    address: r.get::<_, String>(1).ok(),
                    body: r.get::<_, String>(2).ok(),
                    msg_type: r.get::<_, i64>(3).ok(),
                    timestamp: r.get::<_, i64>(4).ok(),
                    read: r.get::<_, i64>(5).map(|v| v != 0).ok(),
                    source: "live".into(),
                    is_deleted: false,
                })
            }) {
                for r in rows.flatten() {
                    records.push(r);
                }
            }
        }

        // Read live MMS rows.
        if let Ok(mut stmt) = conn.prepare(
            "SELECT _id, sub, ct_t, date, m_type, read FROM mms ORDER BY date DESC LIMIT 5000"
        ) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(SmsRecord {
                    id: r.get::<_, i64>(0).ok(),
                    address: r.get::<_, String>(1).ok(),
                    body: r.get::<_, String>(2).ok(),
                    msg_type: r.get::<_, i64>(4).ok(),
                    timestamp: r.get::<_, i64>(3).ok(),
                    read: r.get::<_, i64>(5).map(|v| v != 0).ok(),
                    source: "live".into(),
                    is_deleted: false,
                })
            }) {
                for r in rows.flatten() {
                    records.push(r);
                }
            }
        }
    }

    let live_count = records.len();

    // Carve deleted records from freeblocks/freelist.
    let mut deleted = carve_sms_db(&local_db)?;

    // Carve from WAL.
    if has_wal {
        deleted.extend(carve_sms_wal(&local_wal)?);
    }

    // Dedup: skip carved records that match a live record by id+timestamp.
    let live_keys: std::collections::HashSet<(Option<i64>, Option<i64>)> = records
        .iter()
        .map(|r| (r.id, r.timestamp))
        .collect();

    let mut deduped_deleted = Vec::new();
    for r in deleted {
        if !live_keys.contains(&(r.id, r.timestamp)) {
            deduped_deleted.push(r);
        }
    }

    let deleted_count = deduped_deleted.len();
    records.extend(deduped_deleted);

    notes.push(format!(
        "Pulled {} bytes from {}, {} live SMS, {} deleted recovered",
        db_data.len(),
        used_path,
        live_count,
        deleted_count
    ));

    // Clean up.
    let _ = std::fs::remove_file(&local_db);
    let _ = std::fs::remove_file(&local_wal);

    Ok(DeletedSmsResult {
        live_count,
        deleted_count,
        records,
        sha256: hash,
        notes,
    })
}

// ─── SMS-specific carving ───

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";
/// Plausible SMS timestamp window (ms): 2005-01-01 .. 2100-01-01.
const TS_MIN: i64 = 1_104_537_600_000;
const TS_MAX: i64 = 4_102_444_800_000;

/// Try to interpret carved values as an SMS row.
fn sms_from_values(values: &[crate::sqlite::carve::CellValue], source: &str) -> Option<SmsRecord> {
    // SMS rows typically have: _id (int), address (text), body (text), type (int), date (int), read (int)
    let text = values
        .iter()
        .filter_map(|v| match v {
            crate::sqlite::carve::CellValue::Text(t) if !t.trim().is_empty() && t.len() <= 65_536 => Some(t.clone()),
            _ => None,
        })
        .max_by_key(|t| t.len())?;

    let timestamp = values.iter().find_map(|v| match v {
        crate::sqlite::carve::CellValue::Int(i) if (TS_MIN..TS_MAX).contains(i) => Some(*i),
        _ => None,
    })?;

    let id = values.iter().find_map(|v| match v {
        crate::sqlite::carve::CellValue::Int(i) if *i > 0 && *i < 1_000_000 => Some(*i),
        _ => None,
    });

    let address = values.iter().find_map(|v| match v {
        crate::sqlite::carve::CellValue::Text(t) if t.len() >= 4 && t.len() <= 20 && t.chars().all(|c| c.is_ascii_digit() || c == '+' || c == '-' || c == ' ') => Some(t.clone()),
        _ => None,
    });

    Some(SmsRecord {
        id,
        address,
        body: Some(text),
        msg_type: None,
        timestamp: Some(timestamp),
        read: None,
        source: source.into(),
        is_deleted: true,
    })
}

fn try_sms_record(bytes: &[u8], source: &str, out: &mut Vec<SmsRecord>, seen: &mut std::collections::HashSet<(Option<i64>, Option<i64>)>) {
    for candidate in record_candidates(bytes) {
        if let Some(values) = crate::sqlite::carve::parse_record(candidate) {
            if let Some(r) = sms_from_values(&values, source) {
                let key = (r.id, r.timestamp);
                if seen.insert(key) {
                    out.push(r);
                }
                return;
            }
        }
    }
}

fn record_candidates(bytes: &[u8]) -> Vec<&[u8]> {
    let mut cands = vec![bytes];
    if let Some((_, o1)) = crate::sqlite::carve::read_varint(bytes, 0) {
        if let Some((_, o2)) = crate::sqlite::carve::read_varint(bytes, o1) {
            if o2 < bytes.len() {
                cands.push(&bytes[o2..]);
            }
        }
    }
    cands
}

fn carve_leaf_cells_sms(page: &[u8], usable: usize, source: &str, out: &mut Vec<SmsRecord>, seen: &mut std::collections::HashSet<(Option<i64>, Option<i64>)>) {
    if page.len() < 8 || page[0] != 0x0d {
        return;
    }
    let ncells = u16::from_be_bytes([page[3], page[4]]) as usize;
    for i in 0..ncells {
        let ptr_off = 8 + i * 2;
        if ptr_off + 2 > page.len() { break; }
        let cell_off = u16::from_be_bytes([page[ptr_off], page[ptr_off + 1]]) as usize;
        if cell_off >= page.len() { continue; }
        if let Some((payload_len, o1)) = crate::sqlite::carve::read_varint(page, cell_off) {
            if let Some((_, o2)) = crate::sqlite::carve::read_varint(page, o1) {
                let payload_len = payload_len as usize;
                let x = usable.saturating_sub(35);
                let local = if payload_len <= x { payload_len } else {
                    let m = (usable.saturating_sub(12) * 32) / 255 - 23;
                    let k = m + (payload_len - m) % usable.saturating_sub(4).max(1);
                    if k <= x { k } else { m }
                };
                let end = page.len().min(o2 + local);
                if o2 < end {
                    try_sms_record(&page[o2..end], source, out, seen);
                }
            }
        }
    }
}

fn carve_freeblocks_sms(page: &[u8], source: &str, out: &mut Vec<SmsRecord>, seen: &mut std::collections::HashSet<(Option<i64>, Option<i64>)>) {
    if page.len() < 8 || page[0] != 0x0d { return; }
    let mut fb = u16::from_be_bytes([page[1], page[2]]) as usize;
    let mut guard = 0;
    while fb != 0 && fb + 4 <= page.len() && guard < 512 {
        guard += 1;
        let next = u16::from_be_bytes([page[fb], page[fb + 1]]) as usize;
        let size = u16::from_be_bytes([page[fb + 2], page[fb + 3]]) as usize;
        if size >= 6 && fb + size <= page.len() {
            try_sms_record(&page[fb + 4..fb + size], source, out, seen);
        }
        if next <= fb { break; }
        fb = next;
    }
}

fn carve_sms_db(path: &Path) -> Result<Vec<SmsRecord>> {
    let data = std::fs::read(path)?;
    if data.len() < 100 || &data[0..16] != SQLITE_MAGIC { return Ok(Vec::new()); }
    let raw_ps = u16::from_be_bytes([data[16], data[17]]);
    let page_size = if raw_ps == 1 { 65536 } else { raw_ps as usize };
    if page_size < 512 || !page_size.is_power_of_two() { return Ok(Vec::new()); }
    let reserved = data[20] as usize;
    let usable = page_size.saturating_sub(reserved);
    let n_pages = data.len() / page_size;

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Freelist pages
    let mut trunk = u32::from_be_bytes([data[32], data[33], data[34], data[35]]) as usize;
    let mut guard = 0;
    while trunk != 0 && trunk <= n_pages && guard < 4096 {
        guard += 1;
        let base = (trunk - 1) * page_size;
        if base + 8 > data.len() { break; }
        let next = u32::from_be_bytes(data[base..base + 4].try_into().unwrap()) as usize;
        let nleaf = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap()) as usize;
        for i in 0..nleaf.min((page_size - 8) / 4) {
            let p = u32::from_be_bytes(data[base + 8 + i * 4..base + 12 + i * 4].try_into().unwrap()) as usize;
            if (1..=n_pages).contains(&p) {
                carve_leaf_cells_sms(&data[(p - 1) * page_size..p * page_size], usable, "freelist", &mut out, &mut seen);
            }
        }
        if next == trunk { break; }
        trunk = next;
    }

    // Freeblocks
    for p in 1..=n_pages {
        carve_freeblocks_sms(&data[(p - 1) * page_size..p * page_size], "freeblock", &mut out, &mut seen);
    }

    Ok(out)
}

fn carve_sms_wal(wal_path: &Path) -> Result<Vec<SmsRecord>> {
    let data = std::fs::read(wal_path)?;
    if data.len() < 32 { return Ok(Vec::new()); }
    let magic = u32::from_be_bytes(data[0..4].try_into().unwrap());
    if magic != 0x377f_0682 && magic != 0x377f_0683 { return Ok(Vec::new()); }
    let page_size = u32::from_be_bytes(data[8..12].try_into().unwrap()) as usize;
    if page_size < 512 || !page_size.is_power_of_two() { return Ok(Vec::new()); }
    let salt = &data[16..24];

    let mut latest: std::collections::HashMap<u32, &[u8]> = std::collections::HashMap::new();
    let mut off = 32;
    while off + 24 + page_size <= data.len() {
        let frame = &data[off..off + 24 + page_size];
        if &frame[8..16] != salt { break; }
        let pgno = u32::from_be_bytes(frame[0..4].try_into().unwrap());
        latest.insert(pgno, &frame[24..24 + page_size]);
        off += 24 + page_size;
    }

    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for page in latest.values() {
        carve_leaf_cells_sms(page, page_size, "wal", &mut out, &mut seen);
    }
    Ok(out)
}
