//! Parser for WhatsApp "Export chat" `.txt` files (Settings → Chats →
//! Export chat). These exports are real, unencrypted transcripts — no
//! root or key file needed — and come in two common line formats:
//!
//! ```text
//! 29/08/2026, 10:15 - Aarav: Hello there
//! [29/08/2026, 10:15:32] Aarav: Hello there
//! ```
//!
//! Lines that don't start a new message are continuation lines of the
//! previous message (multi-line texts).

use crate::sqlite::{Chat, Contact, Message, Source};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TxtImport {
    /// Chat title (file name stem by default).
    pub chat_name: String,
    /// Distinct sender names in order of appearance.
    pub senders: Vec<String>,
    pub messages: Vec<Message>,
    /// Lines that looked like system notices (encryption notice, joins…).
    pub skipped_system: usize,
}

/// Parse one exported chat file. Timestamps are interpreted as UTC ms.
pub fn parse_export(text: &str, chat_name: &str) -> TxtImport {
    let mut out = TxtImport {
        chat_name: chat_name.to_string(),
        ..Default::default()
    };
    let mut sender_order: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_end();
        if line.is_empty() {
            continue;
        }
        match parse_line(line) {
            Some((ts, sender, msg)) => {
                if sender.is_empty() || msg.contains("<Media omitted>") {
                    out.skipped_system += 1;
                    continue;
                }
                if !sender_order.iter().any(|s| s == &sender) {
                    sender_order.push(sender.clone());
                }
                out.messages.push(Message {
                    row_id: Some(out.messages.len() as i64 + 1),
                    chat_jid: Some(chat_name.to_string()),
                    from_me: false,
                    sender: Some(sender),
                    text: Some(msg),
                    timestamp: ts,
                    source: Source::Live,
                });
            }
            None => {
                // Continuation of the previous message.
                if let Some(last) = out.messages.last_mut() {
                    if let Some(t) = &mut last.text {
                        t.push('\n');
                        t.push_str(line);
                    }
                }
            }
        }
    }
    out.senders = sender_order;
    out
}

/// Try to parse a line as the start of a message; returns
/// `(timestamp_ms, sender, text)` or `None` for continuation lines.
fn parse_line(line: &str) -> Option<(Option<i64>, String, String)> {
    // Format A: "[date, time] sender: text"   Format B: "date, time - sender: text"
    let (datetime, rest) = if let Some(r) = line.strip_prefix('[') {
        let close = r.find(']')?;
        (
            r[..close].to_string(),
            r[close + 1..].trim_start().to_string(),
        )
    } else {
        let dash = line.find(" - ")?;
        (line[..dash].to_string(), line[dash + 3..].to_string())
    };

    let colon = rest.find(':')?;
    let sender = rest[..colon].trim().to_string();
    let text = rest[colon + 1..].trim().to_string();
    if sender.is_empty() || sender.len() > 80 {
        return Some((None, String::new(), text)); // system-ish line
    }
    Some((parse_datetime(&datetime), sender, text))
}

/// `29/08/2026, 10:15(:32)( am/pm)` → epoch milliseconds (assumed UTC).
fn parse_datetime(s: &str) -> Option<i64> {
    let mut parts = s.splitn(2, ',');
    let date = parts.next()?.trim();
    let time = parts.next().unwrap_or("").trim();

    let d: Vec<&str> = date.split('/').collect();
    if d.len() != 3 {
        return None;
    }
    let (mut a, mut b) = (d[0].parse::<i64>().ok()?, d[1].parse::<i64>().ok()?);
    let year = d[2].parse::<i64>().ok()?;
    // Exports vary by locale: guess dd/mm vs mm/dd when unambiguous.
    if a > 12 && b <= 12 {
        // a is day
    } else if b > 12 && a <= 12 {
        std::mem::swap(&mut a, &mut b); // b was day
    }
    // else assume dd/mm. Now a=day, b=month.
    let (day, month) = (a, b);
    if !(1..=31).contains(&day) || !(1..=12).contains(&month) {
        return None;
    }

    let pm = time.to_lowercase().contains("pm");
    let am = time.to_lowercase().contains("am");
    let cleaned: String = time.chars().filter(|c| c.is_ascii_digit() || *c == ':').collect();
    let t: Vec<&str> = cleaned.split(':').collect();
    if t.len() < 2 {
        return None;
    }
    let mut hour = t[0].parse::<i64>().ok()?;
    let minute = t[1].parse::<i64>().ok()?;
    let second = t.get(2).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    if pm && hour < 12 {
        hour += 12;
    }
    if am && hour == 12 {
        hour = 0;
    }

    // Days-since-epoch via civil-from-days (Howard Hinnant).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some((days * 86_400 + hour * 3600 + minute * 60 + second) * 1000)
}

/// Convenience: turn an import into the chat/contact shape the UI expects.
pub fn to_parse_result(import: &TxtImport, source_path: &str, sha256: &str) -> crate::sqlite::ParseResult {
    let contacts: Vec<Contact> = import
        .senders
        .iter()
        .map(|s| Contact {
            jid: s.clone(),
            name: s.clone(),
        })
        .collect();
    crate::sqlite::ParseResult {
        db_path: source_path.to_string(),
        schema: "export".into(),
        sha256: sha256.to_string(),
        chats: vec![Chat {
            id: 1,
            jid: import.chat_name.clone(),
            name: import.chat_name.clone(),
        }],
        contacts,
        messages: import.messages.clone(),
        media: Vec::new(),
        total_messages: import.messages.len() as u64,
        truncated: false,
        recovered_deleted: 0,
    }
}

/// One-call import: parse the exported text, hash it (forensic integrity)
/// and return the full parse result.
pub fn import_from_text(text: &str, chat_name: &str, source_path: &str) -> crate::sqlite::ParseResult {
    let import = parse_export(text, chat_name);
    let sha = {
        use sha2::Digest;
        let mut h = sha2::Sha256::new();
        h.update(text.as_bytes());
        hex::encode(h.finalize())
    };
    to_parse_result(&import, source_path, &sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_formats_and_continuations() {
        let text = "\
29/08/2026, 10:15 - Aarav: Hello there
29/08/2026, 10:16 - Priya: line one
line two of same message
[29/08/2026, 10:17:32] Aarav: second message
29/08/2026, 10:18 - +91 98000 00002 created group \"Team\"
";
        let imp = parse_export(text, "Test chat");
        assert_eq!(imp.messages.len(), 3);
        assert_eq!(imp.messages[0].sender.as_deref(), Some("Aarav"));
        assert!(imp.messages[1].text.as_deref().unwrap().contains("line two"));
        assert_eq!(imp.messages[2].sender.as_deref(), Some("Aarav"));
        assert_eq!(imp.senders.len(), 2);
    }

    #[test]
    fn parses_pm_times() {
        let ts = parse_datetime("29/08/2026, 1:05 pm").unwrap();
        // 2026-08-29 13:05 UTC
        assert_eq!(ts / 1000 % 86400, 13 * 3600 + 5 * 60);
    }
}
