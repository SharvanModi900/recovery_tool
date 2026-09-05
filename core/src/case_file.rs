//! Consolidated forensic case file + chain of custody.
//!
//! A **Case** groups evidence gathered from one or more devices: every data
//! source RecoverKit already collects (call logs, SMS, contacts, calendar,
//! WhatsApp, call recordings, screen captures, SIM snapshots) is bundled into
//! the case as [`EvidenceItem`]s, each stamped with an acquisition time and a
//! SHA-256 content fingerprint.
//!
//! Every action against a case appends an [`AuditEntry`] to a **tamper-evident
//! hash chain** — each entry's hash covers the previous entry's hash plus its
//! own fields, so editing or removing any row downstream breaks verification.
//!
//! The case lives in a local SQLite database (`case.db`) alongside the existing
//! `screen_captures.db` / `sim_history.db`; nothing here touches the device.

use crate::acquisition::DirectAcquisition;
use crate::error::Result;
use crate::screen_capture::ScreenCapture;
use crate::sim_forensics::SimSnapshot;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Normalize a timestamp to milliseconds. Some sources (e.g. `stat %Y` on
/// recordings) report seconds; recent millisecond values are ~1.7e12 while
/// seconds are ~1.7e9, so anything below 1e11 is treated as seconds.
fn to_ms(ts: i64) -> i64 {
    if ts > 0 && ts < 100_000_000_000 {
        ts * 1000
    } else {
        ts
    }
}

fn hash_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode(h.finalize())
}

fn hash_str(s: &str) -> String {
    hash_bytes(s.as_bytes())
}

/// SHA-256 of a file's bytes, or `None` when it cannot be read.
fn hash_file(path: &str) -> Option<String> {
    std::fs::read(path).ok().map(|b| hash_bytes(&b))
}

/// SHA-256 over the JSON serialization of any serializable value.
fn hash_json<T: Serialize>(value: &T) -> (String, i64) {
    match serde_json::to_string(value) {
        Ok(s) => {
            let len = s.len() as i64;
            (hash_str(&s), len)
        }
        Err(_) => (String::new(), 0),
    }
}

/// A forensic case grouping evidence from one or more devices.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Case {
    pub id: i64,
    pub case_number: String,
    pub title: String,
    pub examiner: String,
    pub subject: String,
    pub device_serial: Option<String>,
    pub device_model: Option<String>,
    pub created_at: i64, // Unix ms
    pub closed_at: Option<i64>,
    pub notes: Option<String>,
}

/// One piece of evidence in a case — a dataset, a file, or a snapshot.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub id: i64,
    pub case_id: i64,
    pub timestamp: i64, // acquisition time, Unix ms
    pub category: String, // call_log | sms | contact | calendar | whatsapp_* | recording | screen_capture | sim | note
    pub source: String, // human-readable origin (content URI, file path, …)
    pub description: String,
    pub item_count: i64,
    pub size_bytes: i64,
    pub sha256: String,
    pub local_path: Option<String>,
}

/// A chain-of-custody / audit log entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub case_id: i64,
    pub timestamp: i64,
    pub action: String,
    pub detail: String,
    pub actor: String,
    pub hash_chain: String, // SHA-256(prev_hash | ts | action | detail | actor)
}

/// Per-category roll-up for the case dashboard.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: String,
    pub evidence_items: i64,
    pub records: i64,
}

/// Everything the case dashboard needs in one call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CaseSummary {
    pub case_info: Case,
    pub evidence_count: i64,
    pub total_records: i64,
    pub audit_count: i64,
    pub by_category: Vec<CategoryCount>,
    pub earliest: Option<i64>,
    pub latest: Option<i64>,
    pub chain_valid: bool,
}

/// Result of verifying the chain of custody.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChainStatus {
    pub valid: bool,
    pub entries: i64,
    pub broken_at: Option<i64>, // audit id of the first inconsistent entry
    pub message: String,
}

/// A single time-stamped event in the unified cross-source timeline.
///
/// Where [`EvidenceItem`] is an *aggregate* bundle for custody (one row per
/// dataset, SHA-256 fingerprinted), a `TimelineEvent` is one *record* — a call,
/// an SMS, a WhatsApp message, a calendar entry, a recording, a capture, a SIM
/// snapshot — merged across every source into one investigative view.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub id: i64,
    pub case_id: i64,
    pub timestamp: i64, // event time, Unix ms
    pub kind: String, // call | sms | calendar | whatsapp_chat | whatsapp_call | whatsapp_message | recording | capture | sim
    pub title: String, // short human label
    pub detail: String, // searchable body (number, text, name, path, …)
    pub source: String, // origin
    pub local_path: Option<String>, // local file to open/play (captures), if any
}

/// A kind facet count for the timeline filter chips.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KindCount {
    pub kind: String,
    pub count: i64,
}

/// One page of timeline query results plus facets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TimelinePage {
    pub events: Vec<TimelineEvent>,
    pub total: i64, // total matches for the filter (ignoring limit/offset)
    pub kinds: Vec<KindCount>, // per-kind counts matching the text/date filter
    pub returned: i64,
}

/// Genesis seed for the first link of a case's hash chain.
const GENESIS: &str = "RECOVERKIT-GENESIS";

fn compute_link(prev: &str, ts: i64, action: &str, detail: &str, actor: &str) -> String {
    hash_str(&format!("{prev}|{ts}|{action}|{detail}|{actor}"))
}

/// Persistent case + evidence + chain-of-custody store (mirrors `sim_history.db`).
pub struct CaseDB {
    db_path: PathBuf,
}

impl CaseDB {
    pub fn new(base_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_dir)?;
        let db_path = base_dir.join("case.db");
        let db = Connection::open(&db_path)?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS cases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                case_number TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                examiner TEXT NOT NULL DEFAULT '',
                subject TEXT NOT NULL DEFAULT '',
                device_serial TEXT,
                device_model TEXT,
                created_at INTEGER NOT NULL,
                closed_at INTEGER,
                notes TEXT
            )",
            [],
        )?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS evidence (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                case_id INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                category TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT '',
                description TEXT NOT NULL DEFAULT '',
                item_count INTEGER DEFAULT 0,
                size_bytes INTEGER DEFAULT 0,
                sha256 TEXT NOT NULL DEFAULT '',
                local_path TEXT
            )",
            [],
        )?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                case_id INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                action TEXT NOT NULL,
                detail TEXT NOT NULL DEFAULT '',
                actor TEXT NOT NULL DEFAULT '',
                hash_chain TEXT NOT NULL
            )",
            [],
        )?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS timeline (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                case_id INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                kind TEXT NOT NULL,
                title TEXT NOT NULL DEFAULT '',
                detail TEXT NOT NULL DEFAULT '',
                source TEXT NOT NULL DEFAULT '',
                local_path TEXT
            )",
            [],
        )?;
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_timeline_case_ts ON timeline (case_id, timestamp)",
            [],
        )?;
        Ok(Self { db_path })
    }

    // ── Cases ────────────────────────────────────────────────────────────

    /// Create a case; auto-generates a case number when one is not supplied.
    pub fn create_case(
        &self,
        case_number: &str,
        title: &str,
        examiner: &str,
        subject: &str,
        device_serial: Option<&str>,
        device_model: Option<&str>,
        notes: Option<&str>,
    ) -> Result<Case> {
        let ts = now_ms();
        let number = if case_number.trim().is_empty() {
            format!("CASE-{}", ts)
        } else {
            case_number.trim().to_string()
        };
        let c = Case {
            id: 0,
            case_number: number,
            title: title.trim().to_string(),
            examiner: examiner.trim().to_string(),
            subject: subject.trim().to_string(),
            device_serial: device_serial.map(|s| s.to_string()),
            device_model: device_model.map(|s| s.to_string()),
            created_at: ts,
            closed_at: None,
            notes: notes.map(|s| s.to_string()),
        };
        let db = Connection::open(&self.db_path)?;
        db.execute(
            "INSERT INTO cases (case_number, title, examiner, subject, device_serial, device_model, created_at, closed_at, notes)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                c.case_number, c.title, c.examiner, c.subject,
                c.device_serial, c.device_model, c.created_at, c.closed_at, c.notes
            ],
        )?;
        let mut saved = c;
        saved.id = db.last_insert_rowid();
        self.add_audit(saved.id, "CASE_CREATED", &format!("Case {} opened", saved.case_number), examiner)?;
        Ok(saved)
    }

    const CASE_COLS: &'static str =
        "id, case_number, title, examiner, subject, device_serial, device_model, created_at, closed_at, notes";

    fn row_to_case(row: &rusqlite::Row) -> rusqlite::Result<Case> {
        Ok(Case {
            id: row.get(0)?,
            case_number: row.get(1)?,
            title: row.get(2)?,
            examiner: row.get(3)?,
            subject: row.get(4)?,
            device_serial: row.get(5)?,
            device_model: row.get(6)?,
            created_at: row.get(7)?,
            closed_at: row.get(8)?,
            notes: row.get(9)?,
        })
    }

    pub fn list_cases(&self) -> Result<Vec<Case>> {
        let db = Connection::open(&self.db_path)?;
        let sql = format!("SELECT {} FROM cases ORDER BY created_at DESC", Self::CASE_COLS);
        let mut stmt = db.prepare(&sql)?;
        let rows = stmt.query_map([], Self::row_to_case)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn get_case(&self, id: i64) -> Result<Option<Case>> {
        let db = Connection::open(&self.db_path)?;
        let sql = format!("SELECT {} FROM cases WHERE id = ?1", Self::CASE_COLS);
        let mut stmt = db.prepare(&sql)?;
        let mut rows = stmt.query_map(params![id], Self::row_to_case)?;
        Ok(rows.next().transpose()?)
    }

    pub fn close_case(&self, id: i64, actor: &str) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        let ts = now_ms();
        db.execute("UPDATE cases SET closed_at = ?1 WHERE id = ?2 AND closed_at IS NULL", params![ts, id])?;
        drop(db);
        self.add_audit(id, "CASE_CLOSED", "Case marked closed", actor)?;
        Ok(())
    }

    pub fn delete_case(&self, id: i64) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        db.execute("DELETE FROM evidence WHERE case_id = ?1", params![id])?;
        db.execute("DELETE FROM timeline WHERE case_id = ?1", params![id])?;
        db.execute("DELETE FROM audit WHERE case_id = ?1", params![id])?;
        db.execute("DELETE FROM cases WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ── Evidence ─────────────────────────────────────────────────────────

    pub fn add_evidence(&self, e: &EvidenceItem) -> Result<EvidenceItem> {
        let db = Connection::open(&self.db_path)?;
        db.execute(
            "INSERT INTO evidence (case_id, timestamp, category, source, description, item_count, size_bytes, sha256, local_path)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                e.case_id, e.timestamp, e.category, e.source, e.description,
                e.item_count, e.size_bytes, e.sha256, e.local_path
            ],
        )?;
        let mut saved = e.clone();
        saved.id = db.last_insert_rowid();
        Ok(saved)
    }

    pub fn get_evidence(&self, case_id: i64) -> Result<Vec<EvidenceItem>> {
        let db = Connection::open(&self.db_path)?;
        let mut stmt = db.prepare(
            "SELECT id, case_id, timestamp, category, source, description, item_count, size_bytes, sha256, local_path
             FROM evidence WHERE case_id = ?1 ORDER BY timestamp DESC, id DESC",
        )?;
        let rows = stmt.query_map(params![case_id], |row| {
            Ok(EvidenceItem {
                id: row.get(0)?,
                case_id: row.get(1)?,
                timestamp: row.get(2)?,
                category: row.get(3)?,
                source: row.get(4)?,
                description: row.get(5)?,
                item_count: row.get(6)?,
                size_bytes: row.get(7)?,
                sha256: row.get(8)?,
                local_path: row.get(9)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    // ── Timeline (unified cross-source investigation view) ───────────────

    /// Insert a batch of timeline events in one transaction.
    pub fn add_timeline_events(&self, events: &[TimelineEvent]) -> Result<usize> {
        if events.is_empty() {
            return Ok(0);
        }
        let mut db = Connection::open(&self.db_path)?;
        let tx = db.transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO timeline (case_id, timestamp, kind, title, detail, source, local_path)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
            )?;
            for e in events {
                stmt.execute(params![e.case_id, e.timestamp, e.kind, e.title, e.detail, e.source, e.local_path])?;
            }
        }
        tx.commit()?;
        Ok(events.len())
    }

    /// Remove every timeline event of one kind for a case (used before re-ingest).
    pub fn clear_timeline_kind(&self, case_id: i64, kind: &str) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        db.execute("DELETE FROM timeline WHERE case_id = ?1 AND kind = ?2", params![case_id, kind])?;
        Ok(())
    }

    /// Drop all evidence + timeline rows for a case (keeps the audit chain) so a
    /// re-consolidation rebuilds a clean bundle instead of duplicating rows.
    pub fn clear_case_data(&self, case_id: i64) -> Result<()> {
        let db = Connection::open(&self.db_path)?;
        db.execute("DELETE FROM evidence WHERE case_id = ?1", params![case_id])?;
        db.execute("DELETE FROM timeline WHERE case_id = ?1", params![case_id])?;
        Ok(())
    }

    /// Query the merged timeline with optional kind / free-text / date-range
    /// filters. Free text matches title, detail and source (case-insensitive).
    pub fn query_timeline(
        &self,
        case_id: i64,
        kind: Option<&str>,
        q: Option<&str>,
        from: Option<i64>,
        to: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<TimelinePage> {
        let lim = if limit <= 0 { 500 } else { limit.min(5000) };
        let off = if offset < 0 { 0 } else { offset };
        let qpat: Option<String> = q
            .map(|s| format!("%{}%", s.trim()))
            .filter(|s| !s.trim_matches('%').is_empty());
        let qpat_ref = qpat.as_deref();
        let db = Connection::open(&self.db_path)?;

        let total: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM timeline
                 WHERE case_id = ?1
                   AND (?2 IS NULL OR kind = ?2)
                   AND (?3 IS NULL OR (title LIKE ?3 OR detail LIKE ?3 OR source LIKE ?3))
                   AND (?4 IS NULL OR timestamp >= ?4)
                   AND (?5 IS NULL OR timestamp <= ?5)",
                params![case_id, kind, qpat_ref, from, to],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let kinds: Vec<KindCount> = {
            let mut stmt = db.prepare(
                "SELECT kind, COUNT(*) FROM timeline
                 WHERE case_id = ?1
                   AND (?2 IS NULL OR (title LIKE ?2 OR detail LIKE ?2 OR source LIKE ?2))
                   AND (?3 IS NULL OR timestamp >= ?3)
                   AND (?4 IS NULL OR timestamp <= ?4)
                 GROUP BY kind ORDER BY COUNT(*) DESC",
            )?;
            let rows = stmt.query_map(params![case_id, qpat_ref, from, to], |row| {
                Ok(KindCount { kind: row.get(0)?, count: row.get(1)? })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let events: Vec<TimelineEvent> = {
            let mut stmt = db.prepare(
                "SELECT id, case_id, timestamp, kind, title, detail, source, local_path FROM timeline
                 WHERE case_id = ?1
                   AND (?2 IS NULL OR kind = ?2)
                   AND (?3 IS NULL OR (title LIKE ?3 OR detail LIKE ?3 OR source LIKE ?3))
                   AND (?4 IS NULL OR timestamp >= ?4)
                   AND (?5 IS NULL OR timestamp <= ?5)
                 ORDER BY timestamp DESC, id DESC
                 LIMIT ?6 OFFSET ?7",
            )?;
            let rows = stmt.query_map(params![case_id, kind, qpat_ref, from, to, lim, off], |row| {
                Ok(TimelineEvent {
                    id: row.get(0)?,
                    case_id: row.get(1)?,
                    timestamp: row.get(2)?,
                    kind: row.get(3)?,
                    title: row.get(4)?,
                    detail: row.get(5)?,
                    source: row.get(6)?,
                    local_path: row.get(7)?,
                })
            })?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        let returned = events.len() as i64;
        Ok(TimelinePage { events, total, kinds, returned })
    }

    // ── Chain of custody ─────────────────────────────────────────────────

    /// Append an audit entry, chaining its hash to the previous entry.
    pub fn add_audit(&self, case_id: i64, action: &str, detail: &str, actor: &str) -> Result<AuditEntry> {
        let ts = now_ms();
        let db = Connection::open(&self.db_path)?;
        let prev: String = db
            .query_row(
                "SELECT hash_chain FROM audit WHERE case_id = ?1 ORDER BY id DESC LIMIT 1",
                params![case_id],
                |r| r.get(0),
            )
            .unwrap_or_else(|_| GENESIS.to_string());
        let link = compute_link(&prev, ts, action, detail, actor);
        db.execute(
            "INSERT INTO audit (case_id, timestamp, action, detail, actor, hash_chain)
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![case_id, ts, action, detail, actor, link],
        )?;
        Ok(AuditEntry {
            id: db.last_insert_rowid(),
            case_id,
            timestamp: ts,
            action: action.to_string(),
            detail: detail.to_string(),
            actor: actor.to_string(),
            hash_chain: link,
        })
    }

    pub fn get_audit(&self, case_id: i64) -> Result<Vec<AuditEntry>> {
        let db = Connection::open(&self.db_path)?;
        let mut stmt = db.prepare(
            "SELECT id, case_id, timestamp, action, detail, actor, hash_chain
             FROM audit WHERE case_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![case_id], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                case_id: row.get(1)?,
                timestamp: row.get(2)?,
                action: row.get(3)?,
                detail: row.get(4)?,
                actor: row.get(5)?,
                hash_chain: row.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Recompute the whole chain and report the first inconsistency, if any.
    pub fn verify_chain(&self, case_id: i64) -> Result<ChainStatus> {
        let entries = self.get_audit(case_id)?;
        let mut prev = GENESIS.to_string();
        for e in &entries {
            let expected = compute_link(&prev, e.timestamp, &e.action, &e.detail, &e.actor);
            if expected != e.hash_chain {
                return Ok(ChainStatus {
                    valid: false,
                    entries: entries.len() as i64,
                    broken_at: Some(e.id),
                    message: format!("Chain broken at audit entry #{} ({}) — the log was altered.", e.id, e.action),
                });
            }
            prev = e.hash_chain.clone();
        }
        Ok(ChainStatus {
            valid: true,
            entries: entries.len() as i64,
            broken_at: None,
            message: format!("Chain intact — {} entr{} verified.", entries.len(), if entries.len() == 1 { "y" } else { "ies" }),
        })
    }

    // ── Summary ──────────────────────────────────────────────────────────

    pub fn summary(&self, case_id: i64) -> Result<CaseSummary> {
        let case_info = self.get_case(case_id)?.unwrap_or_default();
        let db = Connection::open(&self.db_path)?;

        let (evidence_count, total_records, earliest, latest): (i64, i64, Option<i64>, Option<i64>) = db
            .query_row(
                "SELECT COUNT(*), COALESCE(SUM(item_count),0), MIN(timestamp), MAX(timestamp) FROM evidence WHERE case_id = ?1",
                params![case_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap_or((0, 0, None, None));

        let audit_count: i64 = db
            .query_row("SELECT COUNT(*) FROM audit WHERE case_id = ?1", params![case_id], |r| r.get(0))
            .unwrap_or(0);

        let mut stmt = db.prepare(
            "SELECT category, COUNT(*), COALESCE(SUM(item_count),0) FROM evidence WHERE case_id = ?1 GROUP BY category ORDER BY COUNT(*) DESC",
        )?;
        let by_category = stmt
            .query_map(params![case_id], |row| {
                Ok(CategoryCount {
                    category: row.get(0)?,
                    evidence_items: row.get(1)?,
                    records: row.get(2)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let chain_valid = self.verify_chain(case_id)?.valid;

        Ok(CaseSummary {
            case_info,
            evidence_count,
            total_records,
            audit_count,
            by_category,
            earliest,
            latest,
            chain_valid,
        })
    }
}

// ── Ingestion: fold already-collected sources into a case ─────────────────

fn evidence(case_id: i64, category: &str, source: &str, description: String, item_count: i64, size_bytes: i64, sha256: String, local_path: Option<String>) -> EvidenceItem {
    EvidenceItem {
        id: 0,
        case_id,
        timestamp: now_ms(),
        category: category.to_string(),
        source: source.to_string(),
        description,
        item_count,
        size_bytes,
        sha256,
        local_path,
    }
}

/// Bundle every non-empty collection from a direct-acquisition run.
/// Returns the number of evidence items added.
pub fn ingest_direct_acquisition(db: &CaseDB, case_id: i64, acq: &DirectAcquisition, actor: &str) -> Result<usize> {
    let mut added = 0usize;
    {
        let mut push = |category: &str, source: &str, label: &str, count: i64, hash: String, size: i64| -> Result<()> {
            if count > 0 {
                db.add_evidence(&evidence(
                    case_id,
                    category,
                    source,
                    format!("{label}: {count} record{}", if count == 1 { "" } else { "s" }),
                    count,
                    size,
                    hash,
                    None,
                ))?;
                added += 1;
            }
            Ok(())
        };

        let (h, s) = hash_json(&acq.call_logs);
        push("call_log", "content://call_log/calls", "Call log", acq.call_logs.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.sms_messages);
        push("sms", "content://sms", "SMS/MMS", acq.sms_messages.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.contacts);
        push("contact", "content://contacts", "Contacts", acq.contacts.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.calendar_events);
        push("calendar", "content://com.android.calendar", "Calendar events", acq.calendar_events.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.whatsapp_chats);
        push("whatsapp_chat", "WhatsApp msgstore", "WhatsApp chats", acq.whatsapp_chats.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.whatsapp_calls);
        push("whatsapp_call", "WhatsApp msgstore", "WhatsApp calls", acq.whatsapp_calls.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.whatsapp_media);
        push("whatsapp_media", "WhatsApp Media", "WhatsApp media files", acq.whatsapp_media.len() as i64, h, s)?;
        let (h, s) = hash_json(&acq.call_recordings);
        push("recording", "/sdcard (call recordings)", "Call recordings", acq.call_recordings.len() as i64, h, s)?;
        if !acq.notes.is_empty() {
            let joined = acq.notes.join("\n");
            let count = acq.notes.len() as i64;
            let (h, s) = (hash_str(&joined), joined.len() as i64);
            push("note", "device notes", "Notes", count, h, s)?;
        }
    }

    // Per-record timeline events merged across every direct-acquisition source.
    let mut ev: Vec<TimelineEvent> = Vec::new();
    {
        let mut tev = |kind: &str, ts: i64, title: String, detail: String, source: String, path: Option<String>| {
            ev.push(TimelineEvent {
                id: 0,
                case_id,
                timestamp: to_ms(ts),
                kind: kind.to_string(),
                title,
                detail,
                source,
                local_path: path,
            });
        };
        for c in &acq.call_logs {
            tev("call", c.timestamp,
                format!("{} call · {}", c.call_type, c.name.clone().unwrap_or_else(|| c.number.clone())),
                format!("{} · {}s{}", c.number, c.duration, c.name.clone().map(|n| format!(" · {}", n)).unwrap_or_default()),
                c.source.clone(), None);
        }
        for m in &acq.sms_messages {
            tev("sms", m.timestamp,
                format!("SMS {} · {}", m.msg_type, m.address),
                m.body.clone().unwrap_or_default(),
                m.source.clone(), None);
        }
        for e in &acq.calendar_events {
            tev("calendar", e.start_time,
                e.title.clone().unwrap_or_else(|| "Calendar event".to_string()),
                format!("{}{}", e.location.clone().map(|l| format!("@ {} ", l)).unwrap_or_default(), e.description.clone().unwrap_or_default()),
                e.source.clone(), None);
        }
        for ch in &acq.whatsapp_chats {
            tev("whatsapp_chat", ch.last_timestamp,
                ch.name.clone().unwrap_or_else(|| ch.jid.clone()),
                format!("{} messages · last: {}", ch.message_count, ch.last_message.clone().unwrap_or_default()),
                ch.source.clone(), None);
        }
        for wc in &acq.whatsapp_calls {
            tev("whatsapp_call", wc.timestamp,
                format!("WhatsApp {} call · {}", wc.call_type, wc.name.clone().unwrap_or_else(|| wc.jid.clone())),
                format!("{} · {}s", wc.jid, wc.duration),
                wc.source.clone(), None);
        }
        for r in &acq.call_recordings {
            let ts = if r.timestamp > 0 { r.timestamp } else { r.matched_timestamp.unwrap_or(0) };
            let mut meta: Vec<String> = Vec::new();
            if let Some(n) = &r.matched_number { meta.push(n.clone()); }
            if let Some(nm) = &r.matched_name { meta.push(nm.clone()); }
            if let Some(ct) = &r.matched_call_type { meta.push(format!("{} call", ct)); }
            // `source` holds the on-device path so the timeline can pull & play it.
            tev("recording", ts,
                format!("Recording · {}", r.filename),
                meta.join(" · "),
                r.path.clone(), None);
        }
    }
    let tl = db.add_timeline_events(&ev)?;

    db.add_audit(
        case_id,
        "EVIDENCE_ADDED",
        &format!("{added} evidence item(s) + {tl} timeline event(s) bundled from direct acquisition"),
        actor,
    )?;
    Ok(added)
}

/// Bundle persisted screen captures (hashes the actual PNG bytes on disk).
pub fn ingest_captures(db: &CaseDB, case_id: i64, caps: &[ScreenCapture], actor: &str) -> Result<usize> {
    let mut added = 0usize;
    let mut ev: Vec<TimelineEvent> = Vec::new();
    for c in caps {
        let sha = hash_file(&c.file_path).unwrap_or_else(|| hash_str(&format!("{}|{}|{}", c.file_path, c.timestamp, c.file_size)));
        let desc = match &c.note {
            Some(n) if !n.is_empty() => format!("Screen capture — {}", n),
            _ => {
                if c.dark_frame {
                    "Screen capture (dark frame / locked)".to_string()
                } else {
                    "Screen capture".to_string()
                }
            }
        };
        db.add_evidence(&evidence(case_id, "screen_capture", &c.file_path, desc, 1, c.file_size, sha, Some(c.file_path.clone())))?;
        added += 1;
        ev.push(TimelineEvent {
            id: 0,
            case_id,
            timestamp: to_ms(c.timestamp),
            kind: "capture".to_string(),
            title: format!("Screen capture · {}", c.device_model.clone().unwrap_or_else(|| c.device_serial.clone())),
            detail: c.note.clone().unwrap_or_else(|| if c.dark_frame { "dark frame (locked)".to_string() } else { String::new() }),
            source: c.file_path.clone(),
            local_path: Some(c.file_path.clone()),
        });
    }
    let tl = db.add_timeline_events(&ev)?;
    if added > 0 {
        db.add_audit(case_id, "EVIDENCE_ADDED", &format!("{added} screen capture(s) bundled ({tl} timeline events)"), actor)?;
    }
    Ok(added)
}

/// Bundle SIM/network snapshots from the persistent history store.
pub fn ingest_sim(db: &CaseDB, case_id: i64, snaps: &[SimSnapshot], actor: &str) -> Result<usize> {
    let mut added = 0usize;
    let mut ev: Vec<TimelineEvent> = Vec::new();
    for s in snaps {
        let (h, size) = hash_json(s);
        let sha = if s.fingerprint.is_empty() { h } else { s.fingerprint.clone() };
        let carrier = s.carrier_name.clone().or(s.operator_numeric.clone()).unwrap_or_else(|| "unknown".into());
        let iccid = s.iccid.clone().unwrap_or_default();
        let short = if iccid.len() > 8 { format!("{}…{}", &iccid[..4], &iccid[iccid.len() - 4..]) } else { iccid };
        let desc = format!("SIM snapshot — {} ({})", carrier, if short.is_empty() { "no ICCID".into() } else { short });
        db.add_evidence(&evidence(case_id, "sim", "dumpsys isub / telephony", desc, 1, size, sha, None))?;
        added += 1;
        ev.push(TimelineEvent {
            id: 0,
            case_id,
            timestamp: to_ms(s.timestamp),
            kind: "sim".to_string(),
            title: format!("SIM snapshot · {}", carrier),
            detail: format!("ICCID {} · country {}", s.iccid.clone().unwrap_or_default(), s.country_iso.clone().unwrap_or_default()),
            source: "dumpsys isub".to_string(),
            local_path: None,
        });
    }
    let tl = db.add_timeline_events(&ev)?;
    if added > 0 {
        db.add_audit(case_id, "EVIDENCE_ADDED", &format!("{added} SIM snapshot(s) bundled ({tl} timeline events)"), actor)?;
    }
    Ok(added)
}

// ── Report builders ──────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A JSON case report (case + summary + evidence + audit + chain status).
pub fn build_report_json(db: &CaseDB, case_id: i64) -> Result<String> {
    let summary = db.summary(case_id)?;
    let evidence = db.get_evidence(case_id)?;
    let audit = db.get_audit(case_id)?;
    let chain = db.verify_chain(case_id)?;
    let report = serde_json::json!({
        "report": "RecoverKit consolidated case file",
        "generated_at": now_ms(),
        "case": summary.case_info,
        "summary": {
            "evidence_count": summary.evidence_count,
            "total_records": summary.total_records,
            "audit_count": summary.audit_count,
            "by_category": summary.by_category,
            "earliest": summary.earliest,
            "latest": summary.latest,
        },
        "chain_of_custody_valid": chain.valid,
        "chain_status": chain.message,
        "evidence": evidence,
        "audit": audit,
    });
    let json = serde_json::to_string_pretty(&report)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    Ok(json)
}

/// A self-contained HTML case report with the evidence table and custody log.
pub fn build_report_html(db: &CaseDB, case_id: i64) -> Result<String> {
    let s = db.summary(case_id)?;
    let c = &s.case_info;
    let evidence = db.get_evidence(case_id)?;
    let audit = db.get_audit(case_id)?;
    let chain = db.verify_chain(case_id)?;

    let fmt = |ms: Option<i64>| ms.map(crate_ts).unwrap_or_else(|| "—".into());
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset='utf-8'><title>");
    html.push_str(&esc(&c.case_number));
    html.push_str(" — RecoverKit Case</title><style>");
    html.push_str("body{font-family:Segoe UI,Arial,sans-serif;background:#fff8f0;color:#3a2e1f;margin:0;padding:24px}");
    html.push_str("h1{color:#c77700;font-size:22px;margin:0 0 4px}h2{color:#3a2e1f;font-size:15px;margin:22px 0 8px;border-bottom:2px solid #f3e2c8;padding-bottom:4px}");
    html.push_str(".muted{color:#8a7a60;font-size:12px}table{border-collapse:collapse;width:100%;font-size:12px;margin-top:6px}");
    html.push_str("th,td{border:1px solid #f3e2c8;padding:5px 8px;text-align:left;vertical-align:top}th{background:#fbe6c4;color:#3a2e1f}");
    html.push_str(".mono{font-family:Consolas,monospace;font-size:11px}.pill{display:inline-block;padding:2px 10px;border-radius:12px;background:#fbe6c4;font-size:12px;margin:2px 4px 2px 0}");
    html.push_str(".ok{background:#e6f4ea;color:#1e7d3c}.bad{background:#fdecea;color:#8c2f22}");
    html.push_str("</style></head><body>");
    html.push_str(&format!("<h1>RecoverKit — Consolidated Case File</h1><div class='muted'>Generated {} · read-only acquisition</div>", esc(&crate_ts(now_ms()))));

    html.push_str("<h2>Case</h2><table>");
    html.push_str(&format!("<tr><th>Case #</th><td>{}</td><th>Title</th><td>{}</td></tr>", esc(&c.case_number), esc(&c.title)));
    html.push_str(&format!("<tr><th>Examiner</th><td>{}</td><th>Subject</th><td>{}</td></tr>", esc(&c.examiner), esc(&c.subject)));
    html.push_str(&format!("<tr><th>Device</th><td>{} {}</td><th>Opened</th><td>{}</td></tr>", esc(c.device_model.as_deref().unwrap_or("")), esc(c.device_serial.as_deref().unwrap_or("")), esc(&fmt(Some(c.created_at)))));
    html.push_str(&format!("<tr><th>Closed</th><td>{}</td><th>Notes</th><td>{}</td></tr>", esc(&fmt(c.closed_at)), esc(c.notes.as_deref().unwrap_or(""))));
    html.push_str("</table>");

    html.push_str("<h2>Summary</h2><div>");
    html.push_str(&format!("<span class='pill'>{} evidence items</span><span class='pill'>{} records</span><span class='pill'>{} audit entries</span>", s.evidence_count, s.total_records, s.audit_count));
    html.push_str(&format!("<span class='pill {}'>{}</span>", if chain.valid { "ok" } else { "bad" }, esc(&chain.message)));
    html.push_str("</div>");
    if !s.by_category.is_empty() {
        html.push_str("<table><tr><th>Category</th><th>Evidence items</th><th>Records</th></tr>");
        for cc in &s.by_category {
            html.push_str(&format!("<tr><td>{}</td><td>{}</td><td>{}</td></tr>", esc(&cc.category), cc.evidence_items, cc.records));
        }
        html.push_str("</table>");
    }

    html.push_str("<h2>Evidence</h2><table><tr><th>#</th><th>Acquired</th><th>Category</th><th>Description</th><th>Records</th><th>Size</th><th>SHA-256</th></tr>");
    for (i, e) in evidence.iter().enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}<div class='muted'>{}</div></td><td>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            i + 1,
            esc(&crate_ts(e.timestamp)),
            esc(&e.category),
            esc(&e.description),
            esc(&e.source),
            e.item_count,
            e.size_bytes,
            esc(&e.sha256)
        ));
    }
    html.push_str("</table>");

    html.push_str("<h2>Chain of Custody</h2><table><tr><th>#</th><th>Time</th><th>Action</th><th>Detail</th><th>Actor</th><th class='mono'>Hash link</th></tr>");
    for (i, a) in audit.iter().enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class='mono'>{}</td></tr>",
            i + 1,
            esc(&crate_ts(a.timestamp)),
            esc(&a.action),
            esc(&a.detail),
            esc(&a.actor),
            esc(&a.hash_chain)
        ));
    }
    html.push_str("</table>");
    html.push_str(&format!("<div class='muted' style='margin-top:16px'>Each hash link covers the previous link plus this entry's fields; altering any row breaks verification. Chain status: <b class='{}'>{}</b></div>", if chain.valid { "ok" } else { "bad" }, esc(&chain.message)));
    html.push_str("</body></html>");
    Ok(html)
}

/// Milliseconds-since-epoch → `YYYY-MM-DD HH:MM:SS` (UTC) without extra crates.
fn crate_ts(ms: i64) -> String {
    let secs = ms / 1000;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (h, mi, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {h:02}:{mi:02}:{s:02}")
}
