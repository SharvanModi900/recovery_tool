import { useState } from "react";
import type { UnifiedEvent } from "../types";
import { fmtTime } from "../types";
import { buildUnifiedTimeline, generateForensicReport } from "../api";

interface UnifiedTimelinePanelProps {
  serial: string;
}

const CATEGORY_ICONS: Record<string, string> = {
  call: "📞", sms: "💬", whatsapp: "🔐", wifi: "📶", bluetooth: "🔵",
  power: "🔋", notification: "🔔", browser: "🌐", location: "📍",
  account: "👤", deleted_sms: "🗑️", system: "⚙️",
};

export function UnifiedTimelinePanel({ serial }: UnifiedTimelinePanelProps) {
  const [events, setEvents] = useState<UnifiedEvent[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");
  const [categoryFilter, setCategoryFilter] = useState<string>("all");
  const [reportHtml, setReportHtml] = useState<string | null>(null);
  const [reportBusy, setReportBusy] = useState(false);

  const categories = events
    ? [...new Set(events.map((e) => e.category))].sort()
    : [];

  const filtered = events
    ? events.filter((e) => {
        if (categoryFilter !== "all" && e.category !== categoryFilter) return false;
        if (filter) {
          const q = filter.toLowerCase();
          return (e.title.toLowerCase().includes(q) ||
            (e.detail || "").toLowerCase().includes(q) ||
            (e.source_app || "").toLowerCase().includes(q));
        }
        return true;
      })
    : [];

  const handleBuild = async () => {
    setBusy(true);
    try {
      const result = await buildUnifiedTimeline([], [], [], [], [], [], [], serial);
      setEvents(result);
    } catch { /* ignore */ }
    setBusy(false);
  };

  const handleReport = async () => {
    if (!events) return;
    setReportBusy(true);
    try {
      const sources = [
        { name: "Timeline", record_count: events.length, deleted_count: events.filter(e => e.is_deleted).length, sha256: null, notes: [] },
      ];
      const html = await generateForensicReport(serial, null, sources, events);
      setReportHtml(html);
    } catch { /* ignore */ }
    setReportBusy(false);
  };

  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">📊 Unified Forensic Timeline</div>
      <div className="panel-body" style={{ overflow: "auto" }}>
        {!events ? (
          <div className="fx-empty">
            <span className="fx-empty-icon">📊</span>
            <span className="fx-empty-text">
              Merge all collected data sources into one chronological timeline — calls, SMS, WhatsApp, WiFi, Bluetooth, power events, notifications, browser history, and location.
            </span>
            <button className="primary" disabled={!serial || busy} onClick={handleBuild}>
              {busy ? "Building…" : "📊 Build Unified Timeline"}
            </button>
          </div>
        ) : (
          <>
            <div className="fx-stats">
              <span className="fx-stat">{events.length} events</span>
              <span className="fx-stat hot">{events.filter(e => e.is_deleted).length} deleted</span>
              <button className="mini" onClick={handleBuild} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Rebuild</button>
              <button className="mini" onClick={handleReport} disabled={reportBusy || events.length === 0}>
                {reportBusy ? "…" : "📄 Report"}
              </button>
            </div>
            <div className="fx-toolbar">
              <select className="fx-select" value={categoryFilter} onChange={(e) => setCategoryFilter(e.target.value)}>
                <option value="all">All categories</option>
                {categories.map((c) => (
                  <option key={c} value={c}>{CATEGORY_ICONS[c] || "•"} {c}</option>
                ))}
              </select>
              <input className="fx-search" type="text" placeholder="Search events…" value={filter} onChange={(e) => setFilter(e.target.value)} />
            </div>

            {reportHtml && (
              <div className="fx-report-banner">
                <strong>📄 Report generated</strong>
                <div style={{ display: "flex", gap: 4 }}>
                  <button className="mini" onClick={() => {
                    const blob = new Blob([reportHtml], { type: "text/html" });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    a.href = url; a.download = "forensic_report.html"; a.click();
                    URL.revokeObjectURL(url);
                  }}>💾 Download</button>
                  <button className="mini" onClick={() => setReportHtml(null)}>✕</button>
                </div>
              </div>
            )}

            <div className="fx-scroll fx-list">
              {filtered.slice(0, 1000).map((e, i) => (
                <div key={i} className={`fx-row${e.is_deleted ? " deleted" : ""}`}>
                  <span className="fx-row-icon">{CATEGORY_ICONS[e.category] || "•"}</span>
                  <span className={`fx-row-label fx-cat-${e.category}`}>{e.category}</span>
                  <div className="fx-row-content">
                    <div className="fx-row-title" title={e.title}>{e.title}</div>
                    {e.detail && <div className="fx-row-detail">{e.detail}</div>}
                  </div>
                  {e.is_deleted && <span className="fx-row-label fx-cat-deleted_sms">DEL</span>}
                  <span className="fx-row-time">{fmtTime(e.timestamp)}</span>
                </div>
              ))}
              {filtered.length > 1000 && (
                <p className="hint small" style={{ textAlign: "center", padding: 8 }}>Showing 1000 of {filtered.length}</p>
              )}
              {filtered.length === 0 && (
                <div className="fx-empty" style={{ margin: 12 }}>
                  <span className="fx-empty-text">No events match the current filter. Collect data from other tabs first.</span>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
