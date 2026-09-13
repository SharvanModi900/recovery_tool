//! Generalized SQLite database pull-and-carve engine.
//!
//! Given any on-device database path, pulls the main DB + WAL via ADB
//! `exec-out cat`, parses live rows, and carves deleted records from
//! freeblocks, freelist pages, and WAL frames.

use crate::adb::AdbClient;
use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A single carved/deleted record from any SQLite database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarvedRecord {
    /// Table name the record belongs to.
    pub table: String,
    /// Column name → stringified value.
    pub row_data: HashMap<String, String>,
    /// Where the record was recovered from.
    pub source: String,
    /// Confidence: "high" (live-like), "medium" (WAL), "low" (freeblock).
    pub confidence: String,
}

/// Result of pulling and carving a generic database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CarveResult {
    /// Device path of the database.
    pub db_path: String,
    /// Tables discovered in the database.
    pub tables: Vec<String>,
    /// Live records (read through SQLite).
    pub live_records: Vec<CarvedRecord>,
    /// Deleted/carved records.
    pub carved_records: Vec<CarvedRecord>,
    /// SHA-256 of the pulled database file.
    pub sha256: String,
    /// Notes/warnings from the extraction.
    pub notes: Vec<String>,
}

/// Pull a raw file from the device via `exec-out cat`.
fn pull_file(adb: &AdbClient, serial: &str, remote_path: &str) -> Result<Vec<u8>> {
    adb.exec_out(serial, &["cat", remote_path])
}

/// Compute SHA-256 of a byte slice.
fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// List all tables in a SQLite database.
fn list_tables(conn: &Connection) -> Vec<String> {
    let mut stmt = match conn.prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Get column names for a table.
fn table_columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(&format!("PRAGMA table_info(\"{table}\")")) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |r| r.get::<_, String>(1))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Read all live rows from a table as CarvedRecords.
fn read_live_table(conn: &Connection, table: &str, columns: &[String]) -> Vec<CarvedRecord> {
    let col_list = columns.iter().map(|c| format!("\"{c}\"")).collect::<Vec<_>>().join(", ");
    let sql = format!("SELECT {col_list} FROM \"{table}\" LIMIT 10000");
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let ncols = columns.len();
    let mut records = Vec::new();
    let result = stmt.query_map([], |row| {
        let mut map = HashMap::new();
        for i in 0..ncols {
            let val: String = row.get::<_, String>(i)
                .or_else(|_| row.get::<_, i64>(i).map(|v| v.to_string()))
                .or_else(|_| row.get::<_, f64>(i).map(|v| v.to_string()))
                .unwrap_or_default();
            map.insert(columns[i].clone(), val);
        }
        Ok(CarvedRecord {
            table: table.to_string(),
            row_data: map,
            source: "live".to_string(),
            confidence: "high".to_string(),
        })
    });
    if let Ok(rows) = result {
        for r in rows.flatten() {
            records.push(r);
        }
    }
    records
}

/// Pull a database from the device and carve it.
///
/// `remote_path` is the on-device path (e.g., `/data/data/com.android.providers.telephony/databases/mmssms.db`).
/// `tables_of_interest` optionally limits which tables to read live rows from (empty = all).
pub fn pull_and_carve(
    adb: &AdbClient,
    serial: &str,
    remote_path: &str,
    tables_of_interest: &[&str],
) -> Result<CarveResult> {
    let mut notes = Vec::new();

    // Pull the main DB file.
    let db_data = match pull_file(adb, serial, remote_path) {
        Ok(d) if d.len() > 100 => d,
        Ok(_) => {
            notes.push("Database file too small or empty".into());
            return Ok(CarveResult {
                db_path: remote_path.into(),
                tables: Vec::new(),
                live_records: Vec::new(),
                carved_records: Vec::new(),
                sha256: String::new(),
                notes,
            });
        }
        Err(e) => {
            notes.push(format!("Failed to pull database: {e}"));
            return Ok(CarveResult {
                db_path: remote_path.into(),
                tables: Vec::new(),
                live_records: Vec::new(),
                carved_records: Vec::new(),
                sha256: String::new(),
                notes,
            });
        }
    };

    let hash = sha256_hex(&db_data);

    // Write to a temp file for rusqlite and the carver.
    let tmp_dir = std::env::temp_dir().join("recoverkit_carve");
    let _ = std::fs::create_dir_all(&tmp_dir);
    let db_name = Path::new(remote_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "db".into());
    let local_db = tmp_dir.join(&db_name);
    std::fs::write(&local_db, &db_data).ok();

    // Try to pull the WAL file too.
    let wal_remote = format!("{remote_path}-wal");
    let local_wal = tmp_dir.join(format!("{db_name}-wal"));
    let has_wal = if let Ok(wal_data) = pull_file(adb, serial, &wal_remote) {
        if wal_data.len() > 32 {
            std::fs::write(&local_wal, &wal_data).ok();
            true
        } else {
            false
        }
    } else {
        false
    };

    // Open read-only and enumerate tables.
    let conn = match Connection::open_with_flags(
        &local_db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            notes.push(format!("Cannot open database: {e}"));
            return Ok(CarveResult {
                db_path: remote_path.into(),
                tables: Vec::new(),
                live_records: Vec::new(),
                carved_records: Vec::new(),
                sha256: hash,
                notes,
            });
        }
    };

    let all_tables = list_tables(&conn);
    let tables_to_read: Vec<String> = if tables_of_interest.is_empty() {
        all_tables.clone()
    } else {
        all_tables
            .iter()
            .filter(|t| tables_of_interest.iter().any(|ti| ti.eq_ignore_ascii_case(t)))
            .cloned()
            .collect()
    };

    // Read live rows from each table of interest.
    let mut live_records = Vec::new();
    for table in &tables_to_read {
        let cols = table_columns(&conn, table);
        if cols.is_empty() {
            continue;
        }
        live_records.extend(read_live_table(&conn, table, &cols));
    }

    // Carve deleted records from freeblocks/freelist.
    let mut carved = carve_generic_db(&local_db)?;

    // Carve from WAL if available.
    if has_wal {
        carved.extend(carve_generic_wal(&local_wal)?);
    }

    // Clean up temp files.
    let _ = std::fs::remove_file(&local_db);
    let _ = std::fs::remove_file(&local_wal);

    notes.push(format!(
        "Pulled {} bytes, {} tables, {} live rows, {} carved records",
        db_data.len(),
        tables_to_read.len(),
        live_records.len(),
        carved.len()
    ));

    Ok(CarveResult {
        db_path: remote_path.into(),
        tables: tables_to_read,
        live_records,
        carved_records: carved,
        sha256: hash,
        notes,
    })
}

// ─── Generic carving (not WhatsApp-specific) ───

const SQLITE_MAGIC: &[u8] = b"SQLite format 3\0";

/// Carve generic records from freeblocks and freelist pages.
fn carve_generic_db(path: &Path) -> Result<Vec<CarvedRecord>> {
    let data = std::fs::read(path)?;
    if data.len() < 100 || &data[0..16] != SQLITE_MAGIC {
        return Ok(Vec::new());
    }
    let raw_ps = u16::from_be_bytes([data[16], data[17]]);
    let page_size = if raw_ps == 1 { 65536 } else { raw_ps as usize };
    if page_size < 512 || !page_size.is_power_of_two() {
        return Ok(Vec::new());
    }
    let reserved = data[20] as usize;
    let usable = page_size.saturating_sub(reserved);
    let n_pages = data.len() / page_size;

    let mut out = Vec::new();

    // 1) Freelist pages
    let mut trunk = u32::from_be_bytes([data[32], data[33], data[34], data[35]]) as usize;
    let mut guard = 0;
    while trunk != 0 && trunk <= n_pages && guard < 4096 {
        guard += 1;
        let base = (trunk - 1) * page_size;
        if base + 8 > data.len() {
            break;
        }
        let next = u32::from_be_bytes(data[base..base + 4].try_into().unwrap()) as usize;
        let nleaf = u32::from_be_bytes(data[base + 4..base + 8].try_into().unwrap()) as usize;
        for i in 0..nleaf.min((page_size - 8) / 4) {
            let p = u32::from_be_bytes(data[base + 8 + i * 4..base + 12 + i * 4].try_into().unwrap()) as usize;
            if (1..=n_pages).contains(&p) {
                carve_leaf_cells_generic(&data[(p - 1) * page_size..p * page_size], usable, "freelist", &mut out);
            }
        }
        if next == trunk {
            break;
        }
        trunk = next;
    }

    // 2) Freeblocks inside all table-leaf pages
    for p in 1..=n_pages {
        carve_freeblocks_generic(&data[(p - 1) * page_size..p * page_size], "freeblock", &mut out);
    }

    Ok(out)
}

/// Carve generic records from a WAL file.
fn carve_generic_wal(wal_path: &Path) -> Result<Vec<CarvedRecord>> {
    let data = std::fs::read(wal_path)?;
    if data.len() < 32 {
        return Ok(Vec::new());
    }
    let magic = u32::from_be_bytes(data[0..4].try_into().unwrap());
    if magic != 0x377f_0682 && magic != 0x377f_0683 {
        return Ok(Vec::new());
    }
    let page_size = u32::from_be_bytes(data[8..12].try_into().unwrap()) as usize;
    if page_size < 512 || !page_size.is_power_of_two() {
        return Ok(Vec::new());
    }
    let salt = &data[16..24];

    let mut latest: HashMap<u32, &[u8]> = HashMap::new();
    let mut off = 32;
    while off + 24 + page_size <= data.len() {
        let frame = &data[off..off + 24 + page_size];
        if &frame[8..16] != salt {
            break;
        }
        let pgno = u32::from_be_bytes(frame[0..4].try_into().unwrap());
        latest.insert(pgno, &frame[24..24 + page_size]);
        off += 24 + page_size;
    }

    let mut out = Vec::new();
    for page in latest.values() {
        carve_leaf_cells_generic(page, page_size, "wal", &mut out);
    }
    Ok(out)
}

/// Carve leaf cells from a page (generic — no WhatsApp heuristic).
fn carve_leaf_cells_generic(page: &[u8], usable: usize, source: &str, out: &mut Vec<CarvedRecord>) {
    if page.len() < 8 || page[0] != 0x0d {
        return;
    }
    let ncells = u16::from_be_bytes([page[3], page[4]]) as usize;
    for i in 0..ncells {
        let ptr_off = 8 + i * 2;
        if ptr_off + 2 > page.len() {
            break;
        }
        let cell_off = u16::from_be_bytes([page[ptr_off], page[ptr_off + 1]]) as usize;
        if cell_off >= page.len() {
            continue;
        }
        // Parse the cell: payload_len varint, rowid varint, then record.
        let payload_start = cell_off;
        if let Some((payload_len, o1)) = crate::sqlite::carve::read_varint(page, payload_start) {
            if let Some((_, o2)) = crate::sqlite::carve::read_varint(page, o1) {
                let payload_len = payload_len as usize;
                let x = usable.saturating_sub(35);
                let local = if payload_len <= x {
                    payload_len
                } else {
                    let m = (usable.saturating_sub(12) * 32) / 255 - 23;
                    let k = m + (payload_len - m) % usable.saturating_sub(4).max(1);
                    if k <= x { k } else { m }
                };
                let end = page.len().min(o2 + local);
                if o2 < end {
                    if let Some(values) = crate::sqlite::carve::parse_record(&page[o2..end]) {
                        if !values.is_empty() {
                            let mut row_data = HashMap::new();
                            for (idx, v) in values.iter().enumerate() {
                                let col_name = format!("col_{idx}");
                                let val_str = match v {
                                    crate::sqlite::carve::CellValue::Null => String::new(),
                                    crate::sqlite::carve::CellValue::Int(i) => i.to_string(),
                                    crate::sqlite::carve::CellValue::Real(f) => f.to_string(),
                                    crate::sqlite::carve::CellValue::Text(s) => s.clone(),
                                    crate::sqlite::carve::CellValue::Blob(b) => format!("<blob {}B>", b.len()),
                                };
                                row_data.insert(col_name, val_str);
                            }
                            let confidence = match source {
                                "wal" => "medium",
                                "freelist" => "low",
                                _ => "low",
                            };
                            out.push(CarvedRecord {
                                table: "unknown".into(),
                                row_data,
                                source: source.into(),
                                confidence: confidence.into(),
                            });
                        }
                    }
                }
            }
        }
    }
}

/// Carve freeblocks from a page (generic).
fn carve_freeblocks_generic(page: &[u8], source: &str, out: &mut Vec<CarvedRecord>) {
    if page.len() < 8 || page[0] != 0x0d {
        return;
    }
    let mut fb = u16::from_be_bytes([page[1], page[2]]) as usize;
    let mut guard = 0;
    while fb != 0 && fb + 4 <= page.len() && guard < 512 {
        guard += 1;
        let next = u16::from_be_bytes([page[fb], page[fb + 1]]) as usize;
        let size = u16::from_be_bytes([page[fb + 2], page[fb + 3]]) as usize;
        if size >= 6 && fb + size <= page.len() {
            let content = &page[fb + 4..fb + size];
            // Try to parse as a record directly, or skip cell prefix.
            for candidate in record_candidates(content) {
                if let Some(values) = crate::sqlite::carve::parse_record(candidate) {
                    if !values.is_empty() {
                        let mut row_data = HashMap::new();
                        for (idx, v) in values.iter().enumerate() {
                            let col_name = format!("col_{idx}");
                            let val_str = match v {
                                crate::sqlite::carve::CellValue::Null => String::new(),
                                crate::sqlite::carve::CellValue::Int(i) => i.to_string(),
                                crate::sqlite::carve::CellValue::Real(f) => f.to_string(),
                                crate::sqlite::carve::CellValue::Text(s) => s.clone(),
                                crate::sqlite::carve::CellValue::Blob(b) => format!("<blob {}B>", b.len()),
                            };
                            row_data.insert(col_name, val_str);
                        }
                        out.push(CarvedRecord {
                            table: "unknown".into(),
                            row_data,
                            source: source.into(),
                            confidence: "low".into(),
                        });
                        break;
                    }
                }
            }
        }
        if next <= fb {
            break;
        }
        fb = next;
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
