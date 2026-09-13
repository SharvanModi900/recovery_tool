//! Keyboard dictionary forensics — reads user-typed words.
//!
//! Reads the user dictionary via content provider. Reveals frequently
//! typed names, addresses, slang. No root required for the content provider.

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictionaryWord {
    pub word: String,
    pub frequency: i32,
    pub locale: Option<String>,
    pub app_id: Option<String>,
}

/// Collect user dictionary words from the device.
pub fn collect_dictionary(adb: &AdbClient, serial: &str) -> Result<Vec<DictionaryWord>> {
    let out = adb.exec_out(serial, &[
        "content", "query",
        "--uri", "content://user_dictionary/words",
        "--projection", "word:frequency:locale:appid",
    ])?;
    let text = String::from_utf8_lossy(&out);
    let mut words = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("Row:") {
            continue;
        }

        let mut word = String::new();
        let mut freq = 0i32;
        let mut locale: Option<String> = None;
        let mut app_id: Option<String> = None;

        // Parse "Row: N word=value, frequency=value, locale=value, appid=value"
        // Strip the "Row: N " prefix
        let row_content = if let Some(idx) = trimmed.find("Row:") {
            let rest = &trimmed[idx + 4..];
            // Skip the row number
            rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == ':')
        } else {
            continue;
        };

        for pair in row_content.split(", ") {
            if let Some((key, val)) = pair.split_once('=') {
                match key.trim() {
                    "word" => word = val.trim().to_string(),
                    "frequency" => freq = val.trim().parse().unwrap_or(0),
                    "locale" => {
                        let v = val.trim();
                        if !v.is_empty() && v != "null" {
                            locale = Some(v.to_string());
                        }
                    }
                    "appid" => {
                        let v = val.trim();
                        if !v.is_empty() && v != "null" {
                            app_id = Some(v.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        if !word.is_empty() {
            words.push(DictionaryWord { word, frequency: freq, locale, app_id });
        }
    }

    // Sort by frequency descending
    words.sort_by(|a, b| b.frequency.cmp(&a.frequency));

    Ok(words)
}
