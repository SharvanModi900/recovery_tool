//! Parser for `.vcf` (vCard) contact exports — every Android Contacts app
//! can export its full address book this way (Contacts → ⋯ → Export).
//! Real contact evidence, no root.

use crate::sqlite::{Chat, Contact, ParseResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VcfImport {
    pub contacts: Vec<Contact>,
}

/// Parse a vCard file (2.1/3.0/4.0 — tolerant line-based parsing).
pub fn parse_vcf(text: &str) -> VcfImport {
    let mut out = VcfImport::default();
    let mut name: Option<String> = None;
    let mut phones: Vec<String> = Vec::new();
    let mut org: Option<String> = None;

    let flush = |out: &mut VcfImport, name: &mut Option<String>, phones: &mut Vec<String>, org: &mut Option<String>| {
        let display = name.take().or_else(|| org.take()).unwrap_or_default();
        for p in phones.drain(..) {
            let clean: String = p.chars().filter(|c| c.is_ascii_digit() || *c == '+').collect();
            if clean.is_empty() {
                continue;
            }
            let label = if display.is_empty() { clean.clone() } else { display.clone() };
            out.contacts.push(Contact { jid: clean, name: label });
        }
        phones.clear();
        *org = None;
    };

    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.split(';').next().unwrap_or("").to_ascii_uppercase();
        match key.as_str() {
            "BEGIN" => {
                if value.trim().eq_ignore_ascii_case("VCARD") {
                    name = None;
                    org = None;
                }
            }
            "END" => {
                if value.trim().eq_ignore_ascii_case("VCARD") {
                    flush(&mut out, &mut name, &mut phones, &mut org);
                }
            }
            "FN" => {
                if !value.trim().is_empty() {
                    name = Some(value.trim().to_string());
                }
            }
            "N" => {
                // "Last;First;;;" fallback when FN is missing.
                if name.is_none() {
                    let parts: Vec<&str> = value.split(';').collect();
                    let joined = parts
                        .iter()
                        .take(2)
                        .rev()
                        .filter(|p| !p.trim().is_empty())
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !joined.is_empty() {
                        name = Some(joined);
                    }
                }
            }
            "ORG" => {
                if org.is_none() && !value.trim().is_empty() {
                    org = Some(value.trim().to_string());
                }
            }
            "TEL" => phones.push(value.trim().to_string()),
            _ => {}
        }
    }
    // File may end without a final END:VCARD line.
    flush(&mut out, &mut name, &mut phones, &mut org);
    out
}

/// Shape into the standard parse result (schema `contacts`).
pub fn to_parse_result(import: &VcfImport, source_path: &str, sha256: &str) -> ParseResult {
    let mut chats: Vec<Chat> = Vec::new();
    if !import.contacts.is_empty() {
        chats.push(Chat {
            id: 1,
            jid: "address-book".into(),
            name: "Address book".into(),
        });
    }
    ParseResult {
        db_path: source_path.to_string(),
        schema: "contacts".into(),
        sha256: sha256.to_string(),
        chats,
        contacts: import.contacts.clone(),
        messages: Vec::new(),
        media: Vec::new(),
        total_messages: 0,
        truncated: false,
        recovered_deleted: 0,
    }
}

/// One-call import: parse + forensic hash.
pub fn import_from_vcf(text: &str, source_path: &str) -> ParseResult {
    let import = parse_vcf(text);
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
    fn parses_two_cards() {
        let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:Aarav Sharma\nTEL;TYPE=CELL:+91 98000 00001\nEND:VCARD\n\
                   BEGIN:VCARD
VERSION:3.0
N:Patel;Priya;;;
TEL:+919800000002
TEL:+919800000003
END:VCARD
";
        let imp = parse_vcf(vcf);
        assert_eq!(imp.contacts.len(), 3);
        assert_eq!(imp.contacts[0].jid, "+919800000001");
        assert_eq!(imp.contacts[0].name, "Aarav Sharma");
        assert_eq!(imp.contacts[1].name, "Priya Patel");
        assert_eq!(imp.contacts[2].jid, "+919800000003");
    }
}
