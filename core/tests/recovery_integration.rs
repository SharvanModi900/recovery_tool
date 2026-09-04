//! Integration tests: build synthetic msgstore databases with rusqlite,
//! delete rows, and verify the carving engine recovers them.

use recovery_core::sqlite::{self, carve, Source};
use std::path::{Path, PathBuf};

fn tmp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("rc-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Create a modern-schema msgstore (jid/chat/message tables).
fn create_modern_msgstore(path: &Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE jid (_id INTEGER PRIMARY KEY, user TEXT, server TEXT);
         CREATE TABLE chat (_id INTEGER PRIMARY KEY, jid_row_id INTEGER, subject TEXT);
         CREATE TABLE message (
             _id INTEGER PRIMARY KEY,
             chat_row_id INTEGER,
             from_me INTEGER,
             key_id TEXT,
             text TEXT,
             timestamp INTEGER,
             sender_jid_row_id INTEGER
         );
         CREATE TABLE wa_contacts (jid TEXT PRIMARY KEY, display_name TEXT);
         INSERT INTO jid VALUES (1, '15551234567', 's.whatsapp.net');
         INSERT INTO chat VALUES (10, 1, NULL);
         INSERT INTO wa_contacts VALUES ('15551234567@s.whatsapp.net', 'Test Contact');",
    )
    .unwrap();
    conn
}

#[test]
fn parse_modern_schema_live_rows() {
    let dir = tmp_dir("parse");
    let db = dir.join("msgstore.db");
    {
        let conn = create_modern_msgstore(&db);
        conn.execute(
            "INSERT INTO message (chat_row_id, from_me, key_id, text, timestamp)
             VALUES (10, 0, 'A', 'hello there', 1700000000000)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (chat_row_id, from_me, key_id, text, timestamp)
             VALUES (10, 1, 'B', 'my reply', 1700000002000)",
            [],
        )
        .unwrap();
    }
    let parsed = sqlite::parse_msgstore(&db).unwrap();
    assert_eq!(parsed.schema, "modern");
    assert_eq!(parsed.messages.len(), 2);
    assert_eq!(parsed.chats.len(), 1);
    assert_eq!(parsed.contacts.len(), 1);
    assert!(parsed.messages.iter().all(|m| m.source == Source::Live));
}

#[test]
fn recover_deleted_message_from_freeblock() {
    let dir = tmp_dir("freelist");
    let db = dir.join("msgstore.db");
    {
        let conn = create_modern_msgstore(&db);
        for (key, text, ts) in [
            ("A", "keep me", 1_700_000_000_000i64),
            ("B", "deleted secret", 1_700_000_001_000),
            ("C", "also keep", 1_700_000_002_000),
        ] {
            conn.execute(
                "INSERT INTO message (chat_row_id, from_me, key_id, text, timestamp)
                 VALUES (10, 0, ?1, ?2, ?3)",
                rusqlite::params![key, text, ts],
            )
            .unwrap();
        }
        conn.execute("DELETE FROM message WHERE key_id = 'B'", [])
            .unwrap();
        // Deliberately no VACUUM: the deleted record must survive in a freeblock.
    }

    let parsed = sqlite::parse_msgstore(&db).unwrap();
    assert_eq!(parsed.messages.len(), 2, "live parse sees only surviving rows");

    let recovered = sqlite::recover(&db).unwrap();
    assert_eq!(recovered.messages.len(), 3, "carving must bring back the deleted row");
    let deleted = recovered
        .messages
        .iter()
        .find(|m| m.text.as_deref() == Some("deleted secret"))
        .expect("deleted message carved");
    assert_eq!(deleted.source, Source::Freelist);
    assert_eq!(deleted.timestamp, Some(1_700_000_001_000));
    assert_eq!(recovered.recovered_deleted, 1);
}

#[test]
fn carve_wal_frames_with_uncheckpointed_rows() {
    let dir = tmp_dir("wal");
    let db = dir.join("msgstore.db");
    let wal_copy = dir.join("copy.db-wal");
    {
        let conn = create_modern_msgstore(&db);
        conn.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        conn.execute(
            "INSERT INTO message (chat_row_id, from_me, key_id, text, timestamp)
             VALUES (10, 0, 'W', 'wal only message', 1700000005000)",
            [],
        )
        .unwrap();
        // Copy the WAL before close triggers the final checkpoint.
        std::fs::copy(db.with_extension("db-wal"), &wal_copy).unwrap();
    }

    let carved = carve::carve_wal(&wal_copy, Source::Wal).unwrap();
    assert!(
        carved.iter().any(|m| m.text.as_deref() == Some("wal only message")),
        "WAL frame carving must find the uncheckpointed row, got: {carved:?}"
    );
}

#[test]
fn legacy_schema_parse() {
    let dir = tmp_dir("legacy");
    let db = dir.join("msgstore.db");
    {
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch(
            "CREATE TABLE messages (
                 _id INTEGER PRIMARY KEY,
                 key_remote_jid TEXT,
                 key_from_me INTEGER,
                 data TEXT,
                 timestamp INTEGER,
                 message_type INTEGER
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO messages (key_remote_jid, key_from_me, data, timestamp, message_type)
             VALUES ('15557654321@s.whatsapp.net', 0, 'legacy hello', 1600000000000, 0)",
            [],
        )
        .unwrap();
    }
    let parsed = sqlite::parse_msgstore(&db).unwrap();
    assert_eq!(parsed.schema, "legacy");
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].text.as_deref(), Some("legacy hello"));
}
