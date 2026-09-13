//! Account enumeration — lists all registered accounts on the device.
//!
//! Parses `dumpsys account` for account types, names, and sync adapter counts.
//! Proves identity (Google, WhatsApp, Telegram, email accounts).

use crate::adb::AdbClient;
use crate::error::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceAccount {
    pub account_type: String,
    pub account_name: String,
    pub authority_count: i32,
}

/// Collect all registered accounts from the device.
///
/// `dumpsys account` output structure:
///   Accounts:
///     Account {hash type=com.google name=user@gmail.com}:
///       ...
///   Per-Account SyncAdapters:
///     Account {hash type=com.google name=user@gmail.com}:
///       com.google.contacts: ...
///       com.google.calendar: ...
///
/// Two-pass: first collect accounts, then count sync adapters per account.
pub fn collect_accounts(adb: &AdbClient, serial: &str) -> Result<Vec<DeviceAccount>> {
    let out = adb.exec_out(serial, &["dumpsys", "account"])?;
    let text = String::from_utf8_lossy(&out);
    let mut accounts: Vec<DeviceAccount> = Vec::new();

    // --- Pass 1: collect account type+name pairs ---
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Account {") || trimmed.starts_with("Account{") {
            let acct_type = extract_field(trimmed, "type=").unwrap_or_default();
            let acct_name = extract_field(trimmed, "name=").unwrap_or_default();
            if !acct_type.is_empty() && !acct_name.is_empty() {
                accounts.push(DeviceAccount {
                    account_type: friendly_type(&acct_type),
                    account_name: acct_name,
                    authority_count: 0,
                });
            }
        }
    }

    // --- Pass 2: count sync adapters per account from "Per-Account SyncAdapters" section ---
    let mut in_sync_section = false;
    let mut current_acct_key = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Detect start of per-account sync adapters section
        if trimmed.contains("Per-Account SyncAdapters") || trimmed.contains("Per-Account Sync") {
            in_sync_section = true;
            continue;
        }

        if !in_sync_section {
            // Also detect top-level SyncAdapters section for global adapters
            if trimmed == "SyncAdapters:" {
                // Count these as global — add to all accounts of matching type
                continue;
            }
            continue;
        }

        // Inside per-account sync section: detect account headers
        if trimmed.starts_with("Account {") || trimmed.starts_with("Account{") {
            current_acct_key = extract_field(trimmed, "name=").unwrap_or_default();
            continue;
        }

        // Count sync adapter entries (lines like "com.google.contacts: ...")
        // They are indented and contain a colon, but are NOT Account lines
        if !current_acct_key.is_empty()
            && !trimmed.is_empty()
            && trimmed.contains(':')
            && !trimmed.starts_with("Account")
            && !trimmed.starts_with("end:")
        {
            // Find the matching account and increment its count
            for acct in accounts.iter_mut() {
                if acct.account_name == current_acct_key {
                    acct.authority_count += 1;
                    break;
                }
            }
        }

        // End of a per-account block: a blank line or a new top-level section
        if trimmed.is_empty() {
            current_acct_key.clear();
        }
    }

    Ok(accounts)
}

fn extract_field(line: &str, prefix: &str) -> Option<String> {
    let idx = line.find(prefix)?;
    let rest = &line[idx + prefix.len()..];
    let end = rest.find(|c: char| c == ' ' || c == '}' || c == ',').unwrap_or(rest.len());
    let val = &rest[..end];
    if val.is_empty() { None } else { Some(val.to_string()) }
}

fn friendly_type(t: &str) -> String {
    match t {
        "com.google" => "Google".into(),
        "com.whatsapp" => "WhatsApp".into(),
        "org.telegram.messenger" => "Telegram".into(),
        "com.facebook.auth.login" => "Facebook".into(),
        "com.twitter.android.auth.login" => "Twitter/X".into(),
        "com.snapchat.android" => "Snapchat".into(),
        "com.instagram.android" => "Instagram".into(),
        "com.microsoft.office.outlook" => "Outlook".into(),
        "com.android.exchange" => "Exchange".into(),
        "com.github.android" => "GitHub".into(),
        "com.duolingo" => "Duolingo".into(),
        "com.naukri.sync.account" => "Naukri".into(),
        "com.jio.jse.account" => "Jio".into(),
        "www.instagram.com" => "Instagram Web".into(),
        other => other.to_string(),
    }
}
