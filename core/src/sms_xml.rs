//! Parser for SMS exports in the `SMS Backup & Restore` XML format —
//! the de-facto export standard on Android (any SMS app can produce it
//! via the free "SMS Backup & Restore" app). Real messages, no root.
//!
//! ```xml
//! <smses count="2">
//!   <sms address="+919800000001" date="1700000001000" type="1"
//!        body="Hello" contact_name="Aarav" />
//! </smses>
//! ```

use crate::sqlite::{Chat, Contact, Message, Source};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SmsImport {
    pub messages: Vec<Message>,
    pub contacts: Vec<Contact>,
    pub chats: Vec<Chat>,
    /// MMS entries counted but not fully decoded (media bodies).
    pub mms_count: usize,
}

/// Parse an SMS Backup & Restore XML document.
pub fn parse_sms_xml(text: &str) -> SmsImport {
    let mut out = SmsImport::default();
    let mut chat_ids: Vec<String> = Vec::new();
    let mut contact_names: Vec<(String, String)> = Vec::new();

    for tag in extract_tags(text, "sms") {
        let address = attr(&tag, "address").unwrap_or_default();
        let date = attr(&tag, "date").and_then(|d| d.parse::<i64>().ok());
        let typ = attr(&tag, "type").unwrap_or_default();
        let body = unescape(&attr(&tag, "body").unwrap_or_default());
        let contact_name = attr(&tag, "contact_name").unwrap_or_default();
        if address.is_empty() {
            continue;
        }
        if !chat_ids.iter().any(|c| c == &address) {
            chat_ids.push(address.clone());
            if !contact_name.is_empty() && contact_name != "(Unknown)" {
                contact_names.push((address.clone(), contact_name.clone()));
            }
        }
        out.messages.push(Message {
            row_id: Some(out.messages.len() as i64 + 1),
            chat_jid: Some(address),
            from_me: typ == "2",
            sender: None,
            text: if body.is_empty() { None } else { Some(body) },
            timestamp: date,
            source: Source::Live,
        });
    }

    for tag in extract_tags(text, "mms") {
        let address = attr(&tag, "address").unwrap_or_default();
        let date = attr(&tag, "date").and_then(|d| d.parse::<i64>().ok());
        let typ = attr(&tag, "type").unwrap_or_default();
        if address.is_empty() {
            continue;
        }
        if !chat_ids.iter().any(|c| c == &address) {
            chat_ids.push(address.clone());
        }
        // MMS parts are media; keep a placeholder so the thread is visible.
        let part_text = attr(&tag, "text").map(|t| unescape(&t));
        out.messages.push(Message {
            row_id: Some(out.messages.len() as i64 + 1),
            chat_jid: Some(address),
            from_me: typ == "128" || typ == "2",
            sender: None,
            text: Some(part_text.filter(|t| !t.is_empty()).unwrap_or_else(|| "[MMS media message]".into())),
            timestamp: date,
            source: Source::Live,
        });
        out.mms_count += 1;
    }

    out.messages.sort_by_key(|m| m.timestamp.unwrap_or(0));
    out.chats = chat_ids
        .iter()
        .enumerate()
        .map(|(i, jid)| {
            let name = contact_names
                .iter()
                .find(|(j, _)| j == jid)
                .map(|(_, n)| n.clone())
                .unwrap_or_else(|| jid.clone());
            Chat {
                id: i as i64 + 1,
                jid: jid.clone(),
                name,
            }
        })
        .collect();
    out.contacts = contact_names
        .into_iter()
        .map(|(jid, name)| Contact { jid, name })
        .collect();
    out
}

/// Cheap, dependency-free extraction of `<tag .../>` and `<tag ...>` blocks.
fn extract_tags<'a>(text: &'a str, tag: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let open = format!("<{tag}");
    let mut search_from = 0;
    while let Some(pos) = text[search_from..].find(&open) {
        let start = search_from + pos;
        let after = start + open.len();
        // Must be `<sms ` / `<sms>` style — avoid matching `<smses`.
        let next_char = text[after..].chars().next();
        if !matches!(next_char, Some(' ') | Some('>') | Some('/') | Some('\t') | Some('\n')) {
            search_from = after;
            continue;
        }
        let Some(end) = text[start..].find('>') else {
            break;
        };
        out.push(&text[start..start + end + 1]);
        search_from = start + end + 1;
    }
    out
}

/// Read `name="value"` from a tag fragment, tolerating `&quot;` inside.
fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn unescape(s: &str) -> String {
    s.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Shape an SMS import into the standard parse result used by UI/export.
pub fn to_parse_result(import: &SmsImport, source_path: &str, sha256: &str) -> crate::sqlite::ParseResult {
    crate::sqlite::ParseResult {
        db_path: source_path.to_string(),
        schema: "sms".into(),
        sha256: sha256.to_string(),
        chats: import.chats.clone(),
        contacts: import.contacts.clone(),
        messages: import.messages.clone(),
        media: Vec::new(),
        total_messages: import.messages.len() as u64,
        truncated: false,
        recovered_deleted: 0,
    }
}

/// One-call import: parse + forensic hash.
pub fn import_from_xml(text: &str, source_path: &str) -> crate::sqlite::ParseResult {
    let import = parse_sms_xml(text);
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
    fn parses_sms_backup_xml() {
        let xml = r#"<?xml version="1.0"?>
<smses count="3">
<sms protocol="0" address="+919800000001" date="1700000001000" type="1" subject="null" body="Hello there" contact_name="Aarav" />
<sms protocol="0" address="+919800000001" date="1700000002000" type="2" subject="null" body="Hi &amp; bye" contact_name="Aarav" />
<sms protocol="0" address="+919800000002" date="1700000003000" type="1" subject="null" body="" contact_name="(Unknown)" />
</smses>"#;
        let imp = parse_sms_xml(xml);
        assert_eq!(imp.messages.len(), 3);
        assert!(imp.messages[1].from_me);
        assert_eq!(imp.messages[1].text.as_deref(), Some("Hi & bye"));
        assert_eq!(imp.chats.len(), 2);
        assert_eq!(imp.contacts.len(), 1);
        assert_eq!(imp.contacts[0].name, "Aarav");
    }

    #[test]
    fn sms_tag_not_confused_with_smses() {
        let tags = extract_tags("<smses count=\"1\"><sms address=\"x\" /></smses>", "sms");
        assert_eq!(tags.len(), 1);
    }
}
