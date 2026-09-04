//! WhatsApp `msgstore.db` parsing and deleted-record recovery.
//!
//! - [`parse_msgstore`] reads live rows via rusqlite (read-only open).
//! - [`recover`] additionally carves deleted records from freeblocks, the
//!   freelist and the WAL file via [`carve`].

pub mod carve;

use crate::error::Result;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// Where a recovered message came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    /// Live row read through SQLite.
    Live,
    /// Carved from a WAL frame (row not present in the main db file).
    Wal,
    /// Carved from a page freeblock / freelist page (deleted row).
    Freelist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    pub jid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub jid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub row_id: Option<i64>,
    pub chat_jid: Option<String>,
    pub from_me: bool,
    pub sender: Option<String>,
    pub text: Option<String>,
    /// Milliseconds since epoch (WhatsApp convention).
    pub timestamp: Option<i64>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaRef {
    pub message_row_id: i64,
    pub file_path: Option<String>,
    pub mime_type: Option<String>,
    pub size: Option<i64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ParseResult {
    pub db_path: String,
    /// `modern`, `legacy` or `unknown`.
    pub schema: String,
    /// SHA-256 of the parsed database file (forensic integrity).
    pub sha256: String,
    pub chats: Vec<Chat>,
    pub contacts: Vec<Contact>,
    pub messages: Vec<Message>,
    pub media: Vec<MediaRef>,
    pub total_messages: u64,
    /// True when more rows exist than the messages returned (see MESSAGE_LIMIT).
    pub truncated: bool,
    pub recovered_deleted: usize,
}

/// Statistical summary of a recovery result (analytics layer).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Analysis {
    pub messages: usize,
    pub deleted: usize,
    pub chats_count: usize,
    pub first_ts: Option<i64>,
    pub last_ts: Option<i64>,
    /// Fraction of outgoing messages (0.0–1.0).
    pub from_me_ratio: f64,
    /// Top chats: (jid, display name, message count), descending.
    pub top_chats: Vec<(String, String, usize)>,
    /// Activity per calendar day: (yyyy-mm-dd, count), ascending.
    pub per_day: Vec<(String, usize)>,
}

/// Build a jid → display-name map from chats and contacts.
/// Contacts are indexed under both the full jid (`user@server`) and the bare
/// user part, since chat jids often come back without the server suffix.
pub fn jid_names(r: &ParseResult) -> HashMap<String, String> {
    let mut names: HashMap<String, String> = HashMap::new();
    for c in &r.contacts {
        names.insert(c.jid.clone(), c.name.clone());
        if let Some(bare) = c.jid.split_once('@') {
            names.entry(bare.0.to_string()).or_insert_with(|| c.name.clone());
        }
    }
    for c in &r.chats {
        if !c.name.is_empty() && c.name != c.jid {
            names.entry(c.jid.clone()).or_insert_with(|| c.name.clone());
        }
    }
    names
}

/// Milliseconds-since-epoch → `YYYY-MM-DD` (UTC, civil-from-days algorithm).
pub fn ms_to_date(ms: i64) -> String {
    let days = (ms / 1000).div_euclid(86_400);
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
    format!("{y:04}-{m:02}-{d:02}")
}

/// Compute analytics over a parse/recovery result.
pub fn analyze(r: &ParseResult) -> Analysis {
    let names = jid_names(r);
    let mut by_chat: HashMap<String, usize> = HashMap::new();
    let mut by_day: BTreeMap<String, usize> = BTreeMap::new();
    let (mut first, mut last, mut from_me) = (None::<i64>, None::<i64>, 0usize);
    for m in &r.messages {
        if let Some(ts) = m.timestamp {
            *by_chat.entry(m.chat_jid.clone().unwrap_or_default()).or_default() += 1;
            *by_day.entry(ms_to_date(ts)).or_default() += 1;
            first = Some(first.map_or(ts, |f| f.min(ts)));
            last = Some(last.map_or(ts, |l| l.max(ts)));
        }
        if m.from_me {
            from_me += 1;
        }
    }
    let mut top_chats: Vec<(String, String, usize)> = by_chat
        .into_iter()
        .map(|(jid, count)| (jid.clone(), names.get(&jid).cloned().unwrap_or_default(), count))
        .collect();
    top_chats.sort_by(|a, b| b.2.cmp(&a.2));
    top_chats.truncate(5);
    Analysis {
        messages: r.messages.len(),
        deleted: r.recovered_deleted,
        chats_count: r.chats.len(),
        first_ts: first,
        last_ts: last,
        from_me_ratio: if r.messages.is_empty() { 0.0 } else { from_me as f64 / r.messages.len() as f64 },
        top_chats,
        per_day: by_day.into_iter().collect(),
    }
}

/// Cap on rows returned in one parse to keep CLI/UI payloads manageable.
pub const MESSAGE_LIMIT: u64 = 50_000;

fn open_read_only(path: &Path) -> Result<Connection> {
    Ok(Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?)
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .unwrap_or(0)
        > 0
}

fn columns(conn: &Connection, table: &str) -> Vec<String> {
    let mut stmt = match conn.prepare(&format!("PRAGMA table_info({table})")) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    stmt.query_map([], |r| r.get::<_, String>(1))
        .map(|rows| rows.flatten().collect())
        .unwrap_or_default()
}

/// Parse live messages from a msgstore database (modern or legacy schema).
pub fn parse_msgstore(path: &Path) -> Result<ParseResult> {
    let conn = open_read_only(path)?;
    // Forensic integrity hash of the source file.
    let sha256 = std::fs::read(path)
        .map(|bytes| {
            use sha2::Digest;
            let mut h = sha2::Sha256::new();
            h.update(&bytes);
            hex::encode(h.finalize())
        })
        .unwrap_or_default();
    let mut result = ParseResult {
        db_path: path.display().to_string(),
        sha256,
        ..Default::default()
    };

    if table_exists(&conn, "message") && table_exists(&conn, "chat") {
        result.schema = "modern".into();
        parse_modern(&conn, &mut result);
    } else if table_exists(&conn, "messages") {
        result.schema = "legacy".into();
        parse_legacy(&conn, &mut result);
    } else {
        result.schema = "unknown".into();
    }

    result.contacts = parse_contacts(&conn);
    result.media = parse_media(&conn);
    Ok(result)
}

fn parse_modern(conn: &Connection, result: &mut ParseResult) {
    let msg_cols = columns(conn, "message");
    // Modern WhatsApp uses `text`; older builds used `data`.
    let text_col = if msg_cols.iter().any(|c| c == "text") {
        "text"
    } else if msg_cols.iter().any(|c| c == "data") {
        "data"
    } else {
        return;
    };
    let sender_join = if msg_cols.iter().any(|c| c == "sender_jid_row_id") {
        "LEFT JOIN jid sj ON sj._id = m.sender_jid_row_id"
    } else {
        ""
    };
    let sender_sel = if sender_join.is_empty() { "NULL" } else { "sj.user" };

    // Chats: jid.user is NULL for groups, fall back to subject.
    if let Ok(mut stmt) = conn.prepare(
        "SELECT c._id, coalesce(j.user, ''), coalesce(c.subject, j.user, '')
         FROM chat c LEFT JOIN jid j ON j._id = c.jid_row_id",
    ) {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(Chat {
                id: r.get(0)?,
                jid: r.get(1)?,
                name: r.get(2)?,
            })
        }) {
            result.chats = rows.flatten().collect();
        }
    }

    let sql = format!(
        "SELECT m._id, m.from_me, m.{text_col}, m.timestamp,
                coalesce(j.user, '', cast(c.jid_row_id as text)),
                {sender_sel}
         FROM message m
         JOIN chat c ON c._id = m.chat_row_id
         LEFT JOIN jid j ON j._id = c.jid_row_id
         {sender_join}
         ORDER BY m.timestamp ASC
         LIMIT {}",
        MESSAGE_LIMIT + 1
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |r| {
        Ok(Message {
            row_id: Some(r.get(0)?),
            from_me: r.get::<_, i64>(1)? != 0,
            text: r.get(2)?,
            timestamp: r.get(3)?,
            chat_jid: Some(r.get(4)?),
            sender: r.get(5)?,
            source: Source::Live,
        })
    }) else {
        return;
    };
    for m in rows.flatten() {
        result.messages.push(m);
    }
    if result.messages.len() as u64 > MESSAGE_LIMIT {
        result.truncated = true;
        result.messages.truncate(MESSAGE_LIMIT as usize);
    }
    result.total_messages = conn
        .query_row("SELECT count(*) FROM message", [], |r| r.get::<_, i64>(0))
        .unwrap_or(result.messages.len() as i64) as u64;
}

fn parse_legacy(conn: &Connection, result: &mut ParseResult) {
    let sql = format!(
        "SELECT _id, key_remote_jid, key_from_me, data, timestamp
         FROM messages
         WHERE message_type = 0 OR message_type IS NULL
         ORDER BY timestamp ASC
         LIMIT {}",
        MESSAGE_LIMIT + 1
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        // Very old variants may lack message_type.
        if let Ok(mut s2) =
            conn.prepare("SELECT _id, key_remote_jid, key_from_me, data, timestamp FROM messages")
        {
            if let Ok(rows) = s2.query_map([], legacy_row) {
                result.messages = rows.flatten().collect();
            }
        }
        return;
    };
    let Ok(rows) = stmt.query_map([], legacy_row) else {
        return;
    };
    for m in rows.flatten() {
        result.messages.push(m);
    }
    if result.messages.len() as u64 > MESSAGE_LIMIT {
        result.truncated = true;
        result.messages.truncate(MESSAGE_LIMIT as usize);
    }
    result.total_messages = conn
        .query_row("SELECT count(*) FROM messages", [], |r| r.get::<_, i64>(0))
        .unwrap_or(result.messages.len() as i64) as u64;

    // Build pseudo-chat list from distinct remote jids.
    let mut jids: HashSet<String> = HashSet::new();
    for m in &result.messages {
        if let Some(j) = &m.chat_jid {
            jids.insert(j.clone());
        }
    }
    result.chats = jids
        .into_iter()
        .enumerate()
        .map(|(i, jid)| Chat {
            id: i as i64,
            name: jid.clone(),
            jid,
        })
        .collect();
}

fn legacy_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Message> {
    Ok(Message {
        row_id: Some(r.get(0)?),
        chat_jid: Some(r.get(1)?),
        from_me: r.get::<_, i64>(2)? != 0,
        sender: None,
        text: r.get(3)?,
        timestamp: r.get(4)?,
        source: Source::Live,
    })
}

fn parse_contacts(conn: &Connection) -> Vec<Contact> {
    for (table, jid_col, name_col) in [
        ("wa_contacts", "jid", "display_name"),
        ("contacts", "jid", "display_name"),
    ] {
        if !table_exists(conn, table) {
            continue;
        }
        let sql = format!("SELECT {jid_col}, {name_col} FROM {table}");
        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok(Contact {
                    jid: r.get(0)?,
                    name: r.get(1)?,
                })
            }) {
                return rows.flatten().collect();
            }
        }
    }
    Vec::new()
}

fn parse_media(conn: &Connection) -> Vec<MediaRef> {
    if !table_exists(conn, "message_media") {
        return Vec::new();
    }
    let cols = columns(conn, "message_media");
    let size_col = if cols.iter().any(|c| c == "media_size") {
        "media_size"
    } else if cols.iter().any(|c| c == "file_length") {
        "file_length"
    } else {
        "NULL"
    };
    let sql = format!(
        "SELECT message_row_id, file_path, mime_type, {size_col} FROM message_media LIMIT 50000"
    );
    let Ok(mut stmt) = conn.prepare(&sql) else {
        return Vec::new();
    };
    stmt.query_map([], |r| {
        Ok(MediaRef {
            message_row_id: r.get(0)?,
            file_path: r.get(1)?,
            mime_type: r.get(2)?,
            size: r.get(3)?,
        })
    })
    .map(|rows| rows.flatten().collect())
    .unwrap_or_default()
}

/// Parse live rows **and** carve deleted records (freeblocks, freelist pages,
/// WAL frames). Carved rows that duplicate a live message are dropped.
pub fn recover(path: &Path) -> Result<ParseResult> {
    let mut result = parse_msgstore(path)?;

    // Dedup key over live rows.
    let mut live_keys: HashSet<(i64, String)> = HashSet::new();
    for m in &result.messages {
        if let Some(ts) = m.timestamp {
            live_keys.insert((ts, m.text.clone().unwrap_or_default()));
        }
    }

    let mut carved = carve::carve_db(path, Source::Freelist)?;

    let wal_str = format!("{}-wal", path.display());
    let wal_path = Path::new(&wal_str);
    if wal_path.exists() {
        carved.extend(carve::carve_wal(wal_path, Source::Wal)?);
    }

    let mut added = 0;
    for m in carved {
        let key = (m.timestamp.unwrap_or(0), m.text.clone().unwrap_or_default());
        // Skip carve output that duplicates a live row (same text+timestamp).
        if live_keys.contains(&key) {
            continue;
        }
        if m.text.is_none() && m.timestamp.is_none() {
            continue;
        }
        live_keys.insert(key);
        added += 1;
        result.messages.push(m);
    }
    result.recovered_deleted = added;
    Ok(result)
}
