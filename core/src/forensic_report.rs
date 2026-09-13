//! Enhanced forensic report generator.
//!
//! Produces a comprehensive HTML report including all collected data,
//! integrity hashes, extraction metadata, and per-source summaries.

use crate::unified_timeline::UnifiedEvent;
use serde::{Deserialize, Serialize};
use std::fmt::Write as FmtWrite;

/// Summary of a single data source in the report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceReport {
    pub name: String,
    pub record_count: usize,
    pub deleted_count: usize,
    pub sha256: Option<String>,
    pub notes: Vec<String>,
}

/// The full forensic report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicReport {
    pub case_id: Option<String>,
    pub device_serial: String,
    pub device_model: Option<String>,
    pub generated_at: i64,
    pub sources: Vec<SourceReport>,
    pub timeline_summary: TimelineSummary,
    pub integrity_hashes: Vec<String>,
}

/// Summary stats for the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineSummary {
    pub total_events: usize,
    pub first_event: Option<i64>,
    pub last_event: Option<i64>,
    pub category_counts: Vec<(String, usize)>,
}

/// Build a forensic report from collected data.
pub fn build_report(
    serial: &str,
    model: Option<&str>,
    sources: Vec<SourceReport>,
    events: &[UnifiedEvent],
) -> ForensicReport {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    // Timeline summary.
    let first = events.first().map(|e| e.timestamp);
    let last = events.last().map(|e| e.timestamp);
    let mut cat_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in events {
        *cat_counts.entry(format!("{:?}", e.category)).or_default() += 1;
    }
    let mut category_counts: Vec<(String, usize)> = cat_counts.into_iter().collect();
    category_counts.sort_by(|a, b| b.1.cmp(&a.1));

    let timeline_summary = TimelineSummary {
        total_events: events.len(),
        first_event: first,
        last_event: last,
        category_counts,
    };

    // Collect integrity hashes from sources.
    let integrity_hashes: Vec<String> = sources.iter()
        .filter_map(|s| s.sha256.clone())
        .collect();

    ForensicReport {
        case_id: None,
        device_serial: serial.to_string(),
        device_model: model.map(|s| s.to_string()),
        generated_at: now,
        sources,
        timeline_summary,
        integrity_hashes,
    }
}

/// Generate an HTML report from the forensic report data.
pub fn generate_html_report(report: &ForensicReport, events: &[UnifiedEvent]) -> String {
    let mut html = String::new();
    let _ = write!(html, "<!DOCTYPE html><html><head><meta charset='utf-8'>");
    let _ = write!(html, "<title>RecoverKit Forensic Report</title>");
    let _ = write!(html, "<style>");
    let _ = write!(html, "body{{font-family:monospace;margin:20px;background:#fafafa;color:#333}}");
    let _ = write!(html, "h1{{color:#c05d00;border-bottom:2px solid #c05d00;padding-bottom:8px}}");
    let _ = write!(html, "h2{{color:#555;margin-top:24px}}");
    let _ = write!(html, "table{{border-collapse:collapse;width:100%;margin:8px 0}}");
    let _ = write!(html, "th,td{{border:1px solid #ddd;padding:4px 8px;text-align:left;font-size:12px}}");
    let _ = write!(html, "th{{background:#f0e8d8}}");
    let _ = write!(html, ".deleted{{background:#fff0e0;border-left:3px solid #e05e00}}");
    let _ = write!(html, ".pill{{display:inline-block;padding:1px 6px;border-radius:8px;font-size:10px;background:#eee}}");
    let _ = write!(html, ".hash{{font-family:monospace;font-size:10px;color:#888;word-break:break-all}}");
    let _ = write!(html, "</style></head><body>");

    // Header
    let _ = write!(html, "<h1>🔍 RecoverKit Forensic Report</h1>");
    let _ = write!(html, "<table>");
    let _ = write!(html, "<tr><th>Device Serial</th><td>{}</td></tr>", report.device_serial);
    if let Some(ref model) = report.device_model {
        let _ = write!(html, "<tr><th>Device Model</th><td>{model}</td></tr>");
    }
    let _ = write!(html, "<tr><th>Generated</th><td>{}</td></tr>", format_ts(report.generated_at));
    if let Some(ref case_id) = report.case_id {
        let _ = write!(html, "<tr><th>Case ID</th><td>{case_id}</td></tr>");
    }
    let _ = write!(html, "</table>");

    // Source summaries
    let _ = write!(html, "<h2>Data Sources</h2>");
    let _ = write!(html, "<table><tr><th>Source</th><th>Records</th><th>Deleted</th><th>SHA-256</th></tr>");
    for src in &report.sources {
        let hash_display = src.sha256.as_ref().map(|h| format!("<span class='hash'>{}</span>", &h[..16.min(h.len())])).unwrap_or_else(|| "—".into());
        let _ = write!(html, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            src.name, src.record_count, src.deleted_count, hash_display);
    }
    let _ = write!(html, "</table>");

    // Timeline summary
    let _ = write!(html, "<h2>Timeline Summary</h2>");
    let _ = write!(html, "<table>");
    let _ = write!(html, "<tr><th>Total Events</th><td>{}</td></tr>", report.timeline_summary.total_events);
    if let Some(first) = report.timeline_summary.first_event {
        let _ = write!(html, "<tr><th>First Event</th><td>{}</td></tr>", format_ts(first));
    }
    if let Some(last) = report.timeline_summary.last_event {
        let _ = write!(html, "<tr><th>Last Event</th><td>{}</td></tr>", format_ts(last));
    }
    let _ = write!(html, "</table>");

    if !report.timeline_summary.category_counts.is_empty() {
        let _ = write!(html, "<h3>Events by Category</h3><table><tr><th>Category</th><th>Count</th></tr>");
        for (cat, count) in &report.timeline_summary.category_counts {
            let _ = write!(html, "<tr><td>{cat}</td><td>{count}</td></tr>");
        }
        let _ = write!(html, "</table>");
    }

    // Integrity hashes
    if !report.integrity_hashes.is_empty() {
        let _ = write!(html, "<h2>Integrity Hashes</h2><ul>");
        for h in &report.integrity_hashes {
            let _ = write!(html, "<li class='hash'>{h}</li>");
        }
        let _ = write!(html, "</ul>");
    }

    // Timeline events (limited to 1000)
    let _ = write!(html, "<h2>Unified Timeline ({} events)</h2>", events.len().min(1000));
    let _ = write!(html, "<table><tr><th>Time</th><th>Category</th><th>Title</th><th>Detail</th><th>Deleted</th></tr>");
    for e in events.iter().take(1000) {
        let cls = if e.is_deleted { " class='deleted'" } else { "" };
        let detail = e.detail.as_deref().unwrap_or("—");
        let detail_short = if detail.len() > 100 { &detail[..100] } else { detail };
        let _ = write!(html, "<tr{cls}><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            format_ts(e.timestamp), e.category, e.title, detail_short,
            if e.is_deleted { "⚠ YES" } else { "" });
    }
    let _ = write!(html, "</table>");

    let _ = write!(html, "<hr><p style='font-size:10px;color:#999'>Generated by RecoverKit v{} — Read-only forensic extraction</p>", crate::VERSION);
    let _ = write!(html, "</body></html>");
    html
}

fn format_ts(ms: i64) -> String {
    if ms <= 0 { return "—".into(); }
    let secs = ms / 1000;
    // Simple UTC date formatting.
    let days = secs.div_euclid(86400);
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let h = (secs % 86400) / 3600;
    let min = (secs % 3600) / 60;
    let s = secs % 60;
    format!("{y:04}-{m:02}-{d:02} {h:02}:{min:02}:{s:02} UTC")
}
