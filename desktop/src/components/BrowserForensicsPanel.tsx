import { useState } from "react";
import type { BrowserForensicsResult, BrowserHistoryEntry, BookmarkEntry, DownloadEntry, CookieEntry } from "../types";
import { fmtSize, fmtTime } from "../types";
import { collectBrowserData } from "../api";

interface BrowserForensicsPanelProps {
  serial: string;
}

type SubTab = "history" | "bookmarks" | "downloads" | "cookies";

export function BrowserForensicsPanel({ serial }: BrowserForensicsPanelProps) {
  const [tab, setTab] = useState<SubTab>("history");
  const [data, setData] = useState<BrowserForensicsResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [filter, setFilter] = useState("");

  const load = async () => {
    setBusy(true);
    try { setData(await collectBrowserData(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const tabs: { key: SubTab; label: string; icon: string; count?: number }[] = [
    { key: "history", label: "History", icon: "🌐", count: data?.history.length },
    { key: "bookmarks", label: "Bookmarks", icon: "🔖", count: data?.bookmarks.length },
    { key: "downloads", label: "Downloads", icon: "📥", count: data?.downloads.length },
    { key: "cookies", label: "Cookies", icon: "🍪", count: data?.cookies.length },
  ];

  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">🌐 Browser Forensics</div>
      <div className="fx-tabs">
        {tabs.map((t) => (
          <button key={t.key} className={`fx-tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.icon} {t.label}
            {t.count != null && t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
          </button>
        ))}
      </div>
      <div className="panel-body" style={{ overflow: "auto" }}>
        {!data && !busy ? (
          <div className="fx-empty">
            <span className="fx-empty-icon">🌐</span>
            <span className="fx-empty-text">Pull Chrome's browser database for history, bookmarks, downloads, and cookies. Requires root access.</span>
            <button className="primary" disabled={!serial || busy} onClick={load}>
              {busy ? "Collecting…" : "🌐 Collect Browser Data"}
            </button>
          </div>
        ) : data ? (
          <>
            <div className="fx-stats">
              <span className="fx-stat">{data.history.length} history</span>
              <span className="fx-stat info">{data.bookmarks.length} bookmarks</span>
              <span className="fx-stat neutral">{data.downloads.length} downloads</span>
              <span className="fx-stat warn">{data.cookies.length} cookies</span>
              <button className="mini" onClick={load} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
            </div>
            {data.notes.length > 0 && (
              <div className="fx-notes">{data.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
            )}
            <div className="fx-toolbar">
              <input className="fx-search" type="text" placeholder="Filter entries…" value={filter} onChange={(e) => setFilter(e.target.value)} />
            </div>
            <div className="fx-scroll">
              {tab === "history" && <HistoryTable data={data.history} filter={filter} />}
              {tab === "bookmarks" && <BookmarksTable data={data.bookmarks} filter={filter} />}
              {tab === "downloads" && <DownloadsTable data={data.downloads} filter={filter} />}
              {tab === "cookies" && <CookiesTable data={data.cookies} filter={filter} />}
            </div>
          </>
        ) : null}
      </div>
    </div>
  );
}

function HistoryTable({ data, filter }: { data: BrowserHistoryEntry[]; filter: string }) {
  const filtered = filter
    ? data.filter((h) => h.url.toLowerCase().includes(filter.toLowerCase()) || (h.title || "").toLowerCase().includes(filter.toLowerCase()))
    : data;
  return (
    <div className="fx-list">
      {filtered.slice(0, 500).map((h, i) => (
        <div key={i} className="fx-row">
          <span className="fx-row-icon">🌐</span>
          <div className="fx-row-content">
            <div className="fx-row-title" title={h.url}>{h.title || h.url}</div>
            {h.title && <div className="fx-row-detail">{h.url}</div>}
          </div>
          <span className="fx-row-label fx-cat-browser">{h.visit_count}×</span>
          {h.last_visit && <span className="fx-row-time">{fmtTime(h.last_visit)}</span>}
        </div>
      ))}
      {filtered.length > 500 && <p className="hint small" style={{ textAlign: "center", padding: 8 }}>Showing 500 of {filtered.length}</p>}
    </div>
  );
}

function BookmarksTable({ data, filter }: { data: BookmarkEntry[]; filter: string }) {
  const filtered = filter
    ? data.filter((b) => b.url.toLowerCase().includes(filter.toLowerCase()) || (b.title || "").toLowerCase().includes(filter.toLowerCase()))
    : data;
  return (
    <div className="fx-list">
      {filtered.slice(0, 500).map((b, i) => (
        <div key={i} className="fx-row">
          <span className="fx-row-icon">🔖</span>
          <div className="fx-row-content">
            <div className="fx-row-title" title={b.url}>{b.title || b.url}</div>
            {b.folder && <div className="fx-row-detail">in {b.folder}</div>}
          </div>
          {b.created && <span className="fx-row-time">{fmtTime(b.created)}</span>}
        </div>
      ))}
    </div>
  );
}

function DownloadsTable({ data, filter }: { data: DownloadEntry[]; filter: string }) {
  const filtered = filter
    ? data.filter((d) => d.url.toLowerCase().includes(filter.toLowerCase()) || (d.filename || "").toLowerCase().includes(filter.toLowerCase()))
    : data;
  return (
    <div className="fx-list">
      {filtered.slice(0, 500).map((d, i) => (
        <div key={i} className="fx-row">
          <span className="fx-row-icon">📥</span>
          <div className="fx-row-content">
            <div className="fx-row-title">{d.filename || d.url.split("/").pop()}</div>
            <div className="fx-row-detail" title={d.url}>{d.url}</div>
          </div>
          <span className="fx-row-label fx-cat-system">{fmtSize(d.total_bytes)}</span>
          {d.start_time && <span className="fx-row-time">{fmtTime(d.start_time)}</span>}
        </div>
      ))}
    </div>
  );
}

function CookiesTable({ data, filter }: { data: CookieEntry[]; filter: string }) {
  const filtered = filter
    ? data.filter((c) => c.host_key.toLowerCase().includes(filter.toLowerCase()) || c.name.toLowerCase().includes(filter.toLowerCase()))
    : data;
  return (
    <table className="fx-table">
      <thead><tr><th>Host</th><th>Name</th><th>Flags</th><th>Expires</th></tr></thead>
      <tbody>
        {filtered.slice(0, 500).map((c, i) => (
          <tr key={i}>
            <td style={{ fontWeight: 600 }}>{c.host_key}</td>
            <td className="mono">{c.name}</td>
            <td>
              {c.is_secure && <span className="fx-row-label fx-cat-call" style={{ marginRight: 2 }}>🔒</span>}
              {c.is_httponly && <span className="fx-row-label fx-cat-system">HTTP</span>}
            </td>
            <td style={{ fontSize: 9.5 }}>{c.expires_utc ? fmtTime(c.expires_utc) : "session"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
