import { useState } from "react";
import type { DeletedSmsResult, ThumbnailRecoveryResult, CarveResult } from "../types";
import { fmtSize, fmtTime } from "../types";
import { recoverDeletedSms, recoverThumbnails, carveDeviceDb } from "../api";

interface DeletedDataPanelProps {
  serial: string;
}

type SubTab = "sms" | "thumbnails" | "carver";

export function DeletedDataPanel({ serial }: DeletedDataPanelProps) {
  const [tab, setTab] = useState<SubTab>("sms");
  const [sms, setSms] = useState<DeletedSmsResult | null>(null);
  const [thumbs, setThumbs] = useState<ThumbnailRecoveryResult | null>(null);
  const [carveResult, setCarveResult] = useState<CarveResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [dbPath, setDbPath] = useState("");
  const [carveTables, setCarveTables] = useState("");

  const loadSms = async () => {
    setBusy(true);
    try { setSms(await recoverDeletedSms(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const loadThumbs = async () => {
    setBusy(true);
    try { setThumbs(await recoverThumbnails(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const doCarve = async () => {
    if (!dbPath.trim()) return;
    setBusy(true);
    try {
      const tables = carveTables.split(",").map(t => t.trim()).filter(Boolean);
      setCarveResult(await carveDeviceDb(serial, dbPath.trim(), tables));
    } catch { /* ignore */ }
    setBusy(false);
  };

  const tabs: { key: SubTab; label: string; icon: string; count?: number }[] = [
    { key: "sms", label: "Deleted SMS", icon: "💬", count: sms?.deleted_count },
    { key: "thumbnails", label: "Thumbnails", icon: "🖼️", count: thumbs?.total_count },
    { key: "carver", label: "DB Carver", icon: "⛏️", count: carveResult?.carved_records.length },
  ];

  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">🗑️ Deleted Data Recovery</div>
      <div className="fx-tabs">
        {tabs.map((t) => (
          <button key={t.key} className={`fx-tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.icon} {t.label}
            {t.count != null && t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
          </button>
        ))}
      </div>
      <div className="panel-body" style={{ overflow: "auto" }}>
        {tab === "sms" && <SmsSection sms={sms} busy={busy} serial={serial} onLoad={loadSms} />}
        {tab === "thumbnails" && <ThumbSection thumbs={thumbs} busy={busy} onLoad={loadThumbs} />}
        {tab === "carver" && (
          <CarverSection
            result={carveResult} busy={busy} dbPath={dbPath} tables={carveTables}
            onDbPathChange={setDbPath} onTablesChange={setCarveTables} onCarve={doCarve}
          />
        )}
      </div>
    </div>
  );
}

function SmsSection({ sms, busy, serial, onLoad }: { sms: DeletedSmsResult | null; busy: boolean; serial: string; onLoad: () => void }) {
  const [showDeleted, setShowDeleted] = useState(true);
  const [filter, setFilter] = useState("");

  if (!sms) {
    return (
      <div className="fx-empty">
        <span className="fx-empty-icon">💬</span>
        <span className="fx-empty-text">Pull and carve the SMS/MMS database for deleted messages. Requires root access.</span>
        <button className="primary" disabled={!serial || busy} onClick={onLoad}>
          {busy ? "Recovering…" : "🔍 Recover Deleted SMS"}
        </button>
      </div>
    );
  }

  const filtered = sms.records.filter((r) => {
    if (showDeleted && !r.is_deleted) return false;
    if (filter && !r.body?.toLowerCase().includes(filter.toLowerCase()) && !r.address?.includes(filter)) return false;
    return true;
  });

  const typeLabel = (t: number | null) => {
    switch (t) { case 1: return "Inbox"; case 2: return "Sent"; case 3: return "Draft"; case 5: return "Pending"; default: return `Type ${t}`; }
  };

  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{sms.live_count} live</span>
        <span className="fx-stat hot">{sms.deleted_count} deleted</span>
        <button className="mini" onClick={onLoad} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      <div className="fx-toolbar">
        <label style={{ fontSize: 10, display: "flex", alignItems: "center", gap: 3 }}>
          <input type="checkbox" checked={showDeleted} onChange={(e) => setShowDeleted(e.target.checked)} />
          Deleted only
        </label>
        <input className="fx-search" type="text" placeholder="Filter by text or number…" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      {sms.notes.length > 0 && (
        <div className="fx-notes">{sms.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
      )}
      <div className="fx-scroll fx-list">
        {filtered.slice(0, 500).map((r, i) => (
          <div key={i} className={`fx-row${r.is_deleted ? " deleted" : ""}`}>
            <span className="fx-row-icon">{r.msg_type === 1 ? "📥" : r.msg_type === 2 ? "📤" : "📝"}</span>
            <span className={`fx-row-label ${r.msg_type === 1 ? "fx-cat-call" : r.msg_type === 2 ? "fx-cat-sms" : "fx-cat-system"}`}>{typeLabel(r.msg_type)}</span>
            <div className="fx-row-content">
              <span className="fx-row-title">{r.address || "(unknown)"}</span>
              {r.body && <div className="fx-row-detail">{r.body}</div>}
            </div>
            {r.timestamp && <span className="fx-row-time">{fmtTime(r.timestamp)}</span>}
          </div>
        ))}
        {filtered.length > 500 && <p className="hint small" style={{ textAlign: "center", padding: 8 }}>Showing 500 of {filtered.length} records</p>}
      </div>
    </>
  );
}

function ThumbSection({ thumbs, busy, onLoad }: { thumbs: ThumbnailRecoveryResult | null; busy: boolean; onLoad: () => void }) {
  if (!thumbs) {
    return (
      <div className="fx-empty">
        <span className="fx-empty-icon">🖼️</span>
        <span className="fx-empty-text">Scan device for thumbnail caches — even deleted photos leave thumbnails behind.</span>
        <button className="primary" disabled={busy} onClick={onLoad}>
          {busy ? "Scanning…" : "🔍 Scan Thumbnail Caches"}
        </button>
      </div>
    );
  }

  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{thumbs.total_count} thumbnails</span>
        <span className="fx-stat neutral">{fmtSize(thumbs.total_size)}</span>
        <button className="mini" onClick={onLoad} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {thumbs.notes.length > 0 && (
        <div className="fx-notes">{thumbs.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
      )}
      <div className="fx-scroll">
        <table className="fx-table">
          <thead><tr><th>File</th><th>Size</th><th>Original</th></tr></thead>
          <tbody>
            {thumbs.thumbnails.slice(0, 200).map((t, i) => (
              <tr key={i}>
                <td style={{ fontWeight: 500 }}>{t.path.split("/").pop()}</td>
                <td>{fmtSize(t.file_size)}</td>
                <td style={{ color: "var(--saffron-deep)" }}>{t.original_path ? `→ ${t.original_path.split("/").pop()}` : "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {thumbs.thumbnails.length > 200 && <p className="hint small" style={{ textAlign: "center", padding: 8 }}>Showing 200 of {thumbs.thumbnails.length}</p>}
      </div>
    </>
  );
}

function CarverSection({
  result, busy, dbPath, tables, onDbPathChange, onTablesChange, onCarve,
}: {
  result: CarveResult | null; busy: boolean;
  dbPath: string; tables: string;
  onDbPathChange: (v: string) => void; onTablesChange: (v: string) => void;
  onCarve: () => void;
}) {
  return (
    <>
      <div className="fx-form">
        <label>On-device database path</label>
        <input type="text" placeholder="/data/data/com.example.app/databases/mydb.db" value={dbPath} onChange={(e) => onDbPathChange(e.target.value)} />
        <label>Tables (comma-separated, empty = all)</label>
        <input type="text" placeholder="e.g. messages, contacts" value={tables} onChange={(e) => onTablesChange(e.target.value)} />
        <button className="primary" disabled={busy || !dbPath.trim()} onClick={onCarve}>
          {busy ? "Carving…" : "⛏️ Pull & Carve Database"}
        </button>
      </div>

      {result && (
        <>
          <div className="fx-stats">
            <span className="fx-stat">{result.live_records.length} live</span>
            <span className="fx-stat hot">{result.carved_records.length} carved</span>
            <span className="fx-stat neutral" style={{ fontSize: 9 }}>SHA-256: {result.sha256.slice(0, 16)}…</span>
          </div>
          {result.notes.length > 0 && (
            <div className="fx-notes">{result.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
          )}
          {result.carved_records.length > 0 && (
            <div className="fx-scroll fx-list">
              {result.carved_records.slice(0, 100).map((r, i) => (
                <div key={i} className="fx-row">
                  <span className="fx-row-icon">⛏️</span>
                  <span className={`fx-row-label ${r.confidence === "high" ? "fx-cat-call" : r.confidence === "medium" ? "fx-cat-power" : "fx-cat-deleted_sms"}`}>{r.confidence}</span>
                  <div className="fx-row-content">
                    <span className="fx-row-title">{r.table}</span>
                    <div className="fx-row-detail">
                      {Object.entries(r.row_data).slice(0, 4).map(([k, v]) => (
                        <span key={k} style={{ marginRight: 6 }}><em>{k}:</em> {v.length > 30 ? v.slice(0, 30) + "…" : v}</span>
                      ))}
                    </div>
                  </div>
                  <span className={`fx-row-label fx-cat-system`}>{r.source}</span>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </>
  );
}
