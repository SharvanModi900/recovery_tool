import { useState, useMemo, type ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { DirectAcquisition, CallLog, SmsMessage, ContactEntry, CallRecording, CalendarEvent, WhatsAppMedia } from "../types";
import { AudioPlayer, MediaBadge, TypeBadge } from "./index";
import * as api from "../api";

type Tab = "calls" | "sms" | "contacts" | "recordings" | "whatsapp" | "calendar";
type SortDir = "asc" | "desc";

function useSort<T>(defaultKey: keyof T, defaultDir: SortDir = "desc") {
  const [key, setKey] = useState<keyof T>(defaultKey);
  const [dir, setDir] = useState<SortDir>(defaultDir);
  const toggle = (k: keyof T) => {
    if (k === key) setDir(d => (d === "asc" ? "desc" : "asc"));
    else { setKey(k); setDir("asc"); }
  };
  const sort = (arr: T[]) => [...arr].sort((a, b) => {
    const av = a[key] as any, bv = b[key] as any;
    if (av == null && bv == null) return 0;
    if (av == null) return 1;
    if (bv == null) return -1;
    const cmp = typeof av === "number" ? av - bv : String(av).localeCompare(String(bv), undefined, { numeric: true });
    return dir === "asc" ? cmp : -cmp;
  });
  const arrow = (k: keyof T) => (k === key ? (dir === "asc" ? " \u25B2" : " \u25BC") : "");
  return { key, dir, toggle, sort, arrow };
}

const PAGE_SIZE = 50;

function usePagination<T>(items: T[]) {
  const [page, setPage] = useState(1);
  const total = items.length;
  const pages = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const safePage = Math.min(page, pages);
  const start = (safePage - 1) * PAGE_SIZE;
  const slice = items.slice(start, start + PAGE_SIZE);
  return { slice, page: safePage, pages, total, setPage, start };
}

function PaginationBar({ page, pages, total, setPage }: {
  page: number; pages: number; total: number; start: number; setPage: (p: number) => void;
}) {
  if (pages <= 1) return null;
  const btn = (disabled: boolean): React.CSSProperties => ({
    padding: "2px 8px", border: "1px solid var(--border)", borderRadius: 4,
    background: disabled ? "transparent" : "var(--bg-hover)", color: disabled ? "var(--text-muted)" : "var(--text-primary)",
    cursor: disabled ? "default" : "pointer", fontSize: 12, opacity: disabled ? 0.5 : 1,
  });
  return (
    <div style={{ display: "flex", alignItems: "center", justifyContent: "center", gap: 6, padding: "6px 0" }}>
      <button style={btn(page <= 1)} disabled={page <= 1} onClick={() => setPage(1)}>{"\u00AB"}</button>
      <button style={btn(page <= 1)} disabled={page <= 1} onClick={() => setPage(page - 1)}>{"\u2039"}</button>
      <span style={{ fontSize: 12, color: "var(--text-secondary)", minWidth: 120, textAlign: "center" }}>
        Page {page} / {pages} ({total} items)
      </span>
      <button style={btn(page >= pages)} disabled={page >= pages} onClick={() => setPage(page + 1)}>{"\u203A"}</button>
      <button style={btn(page >= pages)} disabled={page >= pages} onClick={() => setPage(pages)}>{"\u00BB"}</button>
    </div>
  );
}

function Th({ children, sortKey, sort, arrow, ...rest }: {
  children: ReactNode; sortKey: any; sort: (k: any) => void; arrow: (k: any) => string;
  [k: string]: any;
}) {
  return <th {...rest} style={{ cursor: "pointer", userSelect: "none", whiteSpace: "nowrap" }} onClick={() => sort(sortKey)}>{children}{arrow(sortKey)}</th>;
}

interface DirectDataViewerProps {
  data: DirectAcquisition;
  serial: string;
  onClose: () => void;
  onStatus: (msg: string) => void;
}

export function DirectDataViewer({ data, serial, onClose, onStatus }: DirectDataViewerProps) {
  const [tab, setTab] = useState<Tab>("calls");
  const [filter, setFilter] = useState("");
  const [playingPath, setPlayingPath] = useState<string | null>(null);
  const [previewPath, setPreviewPath] = useState<string | null>(null);
  const [previewLocal, setPreviewLocal] = useState<string | null>(null);
  const [previewType, setPreviewType] = useState<string | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  const handlePlay = async (remotePath: string): Promise<string | null> => {
    onStatus("Pulling file from device...");
    try {
      const localPath = await api.pullRecording(serial, remotePath);
      setPlayingPath(remotePath);
      onStatus("File ready to play");
      return localPath;
    } catch (e) {
      onStatus(`Error: ${e}`);
      return null;
    }
  };

  const handlePreview = async (remotePath: string, mediaType: string) => {
    setPreviewLoading(true);
    const localPath = await handlePlay(remotePath);
    setPreviewLoading(false);
    if (localPath) {
      setPreviewPath(remotePath);
      setPreviewLocal(localPath);
      setPreviewType(mediaType);
    }
  };

  const tabs: { key: Tab; icon: string; label: string; count: number }[] = [
    { key: "calls", icon: "📞", label: "Call Logs", count: data.call_logs.length },
    { key: "sms", icon: "💬", label: "SMS/MMS", count: data.sms_messages.length },
    { key: "contacts", icon: "👤", label: "Contacts", count: data.contacts.length },
    { key: "recordings", icon: "🎙️", label: "Recordings", count: data.call_recordings.length },
    { key: "whatsapp", icon: "💬", label: "WhatsApp", count: data.whatsapp_media.length + data.whatsapp_chats.length + data.whatsapp_calls.length },
    { key: "calendar", icon: "📅", label: "Calendar", count: data.calendar_events.length },
  ];

  const footerText = () => {
    const t = tabs.find(t => t.key === tab);
    return `📱 REAL DATA — ${t?.count ?? 0} ${t?.label?.toLowerCase() ?? ""}${filter ? " · filtered" : ""}`;
  };

  return (
    <div className="direct-full-overlay">
      {/* Header with tabs + filter */}
      <div className="direct-full-header">
        <div className="direct-full-tabs">
          {tabs.map(t => (
            <button
              key={t.key}
              className={`direct-tab ${tab === t.key ? "active" : ""}`}
              onClick={() => { setTab(t.key); setFilter(""); }}
            >
              {t.icon} {t.label} <span className="tab-count">{t.count}</span>
            </button>
          ))}
        </div>
        <div className="direct-full-actions">
          <input
            className="direct-filter"
            type="text"
            placeholder={`Filter ${tab}...`}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
          <button className="mini" onClick={onClose} title="Close">✕</button>
        </div>
      </div>

      {/* Tab content */}
      <div className="direct-full-body">
        {tab === "calls" && <CallsTab data={data.call_logs} filter={filter} />}
        {tab === "sms" && <SmsTab data={data.sms_messages} filter={filter} />}
        {tab === "contacts" && <ContactsTab data={data.contacts} filter={filter} />}
        {tab === "recordings" && (
          <RecordingsTab
            data={data.call_recordings}
            filter={filter}
            playingPath={playingPath}
            onPlay={handlePlay}
          />
        )}
        {tab === "whatsapp" && (
          <WhatsAppTab
            data={data}
            filter={filter}
            serial={serial}
            onPlay={handlePlay}
            onPreview={handlePreview}
            onClosePreview={() => { setPreviewPath(null); setPreviewLocal(null); setPreviewType(null); }}
            playingPath={playingPath}
            previewPath={previewPath}
            previewLocal={previewLocal}
            previewType={previewType}
            previewLoading={previewLoading}
          />
        )}
        {tab === "calendar" && <CalendarTab data={data.calendar_events} filter={filter} />}
      </div>

      {/* Footer */}
      <div className="direct-full-footer">
        <span className="hint">{footerText()}</span>
      </div>
    </div>
  );
}

/* ─── Sub-tabs (private) ─── */

function CallsTab({ data, filter }: { data: CallLog[]; filter: string }) {
  const s = useSort<CallLog>("timestamp", "desc");
  const q = filter.toLowerCase();
  const filtered = useMemo(() => s.sort(data.filter(c =>
    !q || c.number.includes(q) || (c.name || "").toLowerCase().includes(q) || c.call_type.includes(q)
  )), [data, filter, s.key, s.dir]);
  const pg = usePagination(filtered);
  if (!filtered.length) return <Empty msg="No call logs found." />;
  return (
    <>
    <table className="direct-table">
      <thead><tr>
        <th>#</th>
        <Th sortKey="number" sort={s.toggle} arrow={s.arrow}>Number</Th>
        <Th sortKey="name" sort={s.toggle} arrow={s.arrow}>Name</Th>
        <Th sortKey="call_type" sort={s.toggle} arrow={s.arrow}>Type</Th>
        <Th sortKey="duration" sort={s.toggle} arrow={s.arrow}>Duration</Th>
        <Th sortKey="timestamp" sort={s.toggle} arrow={s.arrow}>Date</Th>
      </tr></thead>
      <tbody>
        {pg.slice.map((c, idx) => (
          <tr key={c.id}>
            <td className="mono small">{pg.start + idx + 1}</td>
            <td className="mono">{c.number}</td>
            <td><b>{c.name || "Unknown"}</b></td>
            <td><TypeBadge type={c.call_type} /></td>
            <td className="mono small">{c.duration}s</td>
            <td className="small">{c.timestamp > 0 ? new Date(c.timestamp).toLocaleString() : "\u2014"}</td>
          </tr>
        ))}
      </tbody>
    </table>
    <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
    </>
  );
}

function SmsTab({ data, filter }: { data: SmsMessage[]; filter: string }) {
  const s = useSort<SmsMessage>("timestamp", "desc");
  const q = filter.toLowerCase();
  const filtered = useMemo(() => s.sort(data.filter(m =>
    !q || m.address.includes(q) || (m.body || "").toLowerCase().includes(q) || m.msg_type.includes(q)
  )), [data, filter, s.key, s.dir]);
  const pg = usePagination(filtered);
  const [selectedMsg, setSelectedMsg] = useState<SmsMessage | null>(null);
  
  if (!filtered.length) return <Empty msg="No messages found." />;
  return (
    <>
      <table className="direct-table">
        <thead><tr>
          <th>#</th>
          <Th sortKey="address" sort={s.toggle} arrow={s.arrow}>Address</Th>
          <Th sortKey="body" sort={s.toggle} arrow={s.arrow}>Body</Th>
          <Th sortKey="msg_type" sort={s.toggle} arrow={s.arrow}>Type</Th>
          <Th sortKey="timestamp" sort={s.toggle} arrow={s.arrow}>Date</Th>
        </tr></thead>
        <tbody>
          {pg.slice.map((m, idx) => (
            <tr key={m.id} style={{ cursor: "pointer" }} onClick={() => setSelectedMsg(m)}>
              <td className="mono small">{pg.start + idx + 1}</td>
              <td className="mono">{m.address}</td>
              <td className="small" title="Click to view full message">
                {(m.body || "").slice(0, 120)}{m.body && m.body.length > 120 ? "…" : ""}
                {m.body && m.body.length > 120 && <span style={{ marginLeft: 4, color: "var(--accent)", fontSize: 10 }}>🔍</span>}
              </td>
              <td><TypeBadge type={m.msg_type} variant="sms" /></td>
              <td className="small">{m.timestamp > 0 ? new Date(m.timestamp).toLocaleString() : "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
      
      {/* Full Message Modal */}
      {selectedMsg && (
        <div style={{
          position: "fixed", top: 0, left: 0, right: 0, bottom: 0,
          background: "rgba(0,0,0,0.6)", zIndex: 9999,
          display: "flex", alignItems: "center", justifyContent: "center",
          padding: 20
        }} onClick={() => setSelectedMsg(null)}>
          <div style={{
            background: "#ffffff", borderRadius: 12, padding: 24,
            maxWidth: 600, width: "100%", maxHeight: "80vh", overflow: "auto",
            border: "2px solid #f4a300", boxShadow: "0 8px 32px rgba(0,0,0,0.3)"
          }} onClick={e => e.stopPropagation()}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16 }}>
              <h3 style={{ margin: 0, fontSize: 16, color: "#c77700" }}>💬 SMS Message</h3>
              <button className="mini" onClick={() => setSelectedMsg(null)}>✕ Close</button>
            </div>
            <div style={{ marginBottom: 12 }}>
              <div style={{ display: "flex", gap: 16, marginBottom: 8 }}>
                <div>
                  <span style={{ color: "#8a7a60", fontSize: 12 }}>From/To:</span>
                  <div className="mono" style={{ fontWeight: 600, color: "#3a2e1f" }}>{selectedMsg.address}</div>
                </div>
                <div>
                  <span style={{ color: "#8a7a60", fontSize: 12 }}>Type:</span>
                  <div><TypeBadge type={selectedMsg.msg_type} variant="sms" /></div>
                </div>
                <div>
                  <span style={{ color: "#8a7a60", fontSize: 12 }}>Date:</span>
                  <div style={{ fontSize: 13, color: "#3a2e1f" }}>
                    {selectedMsg.timestamp > 0 ? new Date(selectedMsg.timestamp).toLocaleString() : "—"}
                  </div>
                </div>
              </div>
            </div>
            <hr style={{ border: "none", borderTop: "1px solid #f3e2c8", margin: "12px 0" }} />
            <div>
              <span style={{ color: "#8a7a60", fontSize: 12 }}>Message Body:</span>
              <div style={{
                marginTop: 8, padding: 16, background: "#fff8f0",
                borderRadius: 8, fontSize: 14, lineHeight: 1.6,
                whiteSpace: "pre-wrap", wordBreak: "break-word",
                border: "1px solid #f3e2c8", color: "#3a2e1f"
              }}>
                {selectedMsg.body || "(empty message)"}
              </div>
            </div>
            <div style={{ marginTop: 12, fontSize: 11, color: "#8a7a60" }}>
              Message ID: {selectedMsg.id} · {selectedMsg.body?.length ?? 0} characters
            </div>
          </div>
        </div>
      )}
    </>
  );
}

function ContactsTab({ data, filter }: { data: ContactEntry[]; filter: string }) {
  const s = useSort<ContactEntry>("display_name" as keyof ContactEntry, "asc");
  const q = filter.toLowerCase();
  const filtered = useMemo(() => s.sort(data.filter(c =>
    !q || (c.display_name || "").toLowerCase().includes(q) || (c.phone || "").includes(q) || (c.email || "").toLowerCase().includes(q)
  )), [data, filter, s.key, s.dir]);
  const pg = usePagination(filtered);
  if (!filtered.length) return <Empty msg="No contacts found." />;
  return (
    <>
    <table className="direct-table">
      <thead><tr>
        <th>#</th>
        <Th sortKey="display_name" sort={s.toggle} arrow={s.arrow}>Name</Th>
        <Th sortKey="phone" sort={s.toggle} arrow={s.arrow}>Phone</Th>
        <Th sortKey="email" sort={s.toggle} arrow={s.arrow}>Email</Th>
      </tr></thead>
      <tbody>
        {pg.slice.map((c, idx) => (
          <tr key={c.id}>
            <td className="mono small">{pg.start + idx + 1}</td>
            <td><b>{c.display_name || "Unknown"}</b></td>
            <td className="mono">{c.phone || "\u2014"}</td>
            <td className="mono small">{c.email || "\u2014"}</td>
          </tr>
        ))}
      </tbody>
    </table>
    <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
    </>
  );
}

function RecordingsTab({ data, filter, playingPath, onPlay }: {
  data: CallRecording[]; filter: string; playingPath: string | null; onPlay: (path: string) => void;
}) {
  const s = useSort<CallRecording>("filename" as keyof CallRecording, "asc");
  const q = filter.toLowerCase();
  const filtered = useMemo(() => s.sort(data.filter(r =>
    !q || r.filename.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)
  )), [data, filter, s.key, s.dir]);
  const pg = usePagination(filtered);
  if (!filtered.length) return <Empty msg="No call recordings found." />;
  return (
    <>
      <table className="direct-table">
        <thead><tr>
          <th>#</th>
          <Th sortKey="filename" sort={s.toggle} arrow={s.arrow}>Filename</Th>
          <Th sortKey="path" sort={s.toggle} arrow={s.arrow}>Path</Th>
          <Th sortKey="size" sort={s.toggle} arrow={s.arrow}>Size</Th>
          <th>Actions</th>
        </tr></thead>
        <tbody>
          {pg.slice.map((r, idx) => (
            <tr key={r.path} style={playingPath === r.path ? { background: "rgba(255,165,0,0.1)" } : undefined}>
              <td className="mono small">{pg.start + idx + 1}</td>
              <td><b>{r.filename}</b></td>
              <td className="mono small">{r.path}</td>
              <td className="mono">{r.size > 0 ? `${(r.size / 1024 / 1024).toFixed(2)} MB` : "—"}</td>
              <td>
                <button className="mini" onClick={() => onPlay(r.path)}
                  style={playingPath === r.path ? { background: "#ff8c00" } : undefined}>
                  {playingPath === r.path ? "🔄 Reload" : "▶ Play"}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
      {playingPath && (
        <AudioPlayer filename={data.find(r => r.path === playingPath)?.filename || "Recording"} localPath="" />
      )}
    </>
  );
}

function WhatsAppTab({ data, filter, onPlay, onPreview, onClosePreview, playingPath, previewPath, previewLocal, previewType, previewLoading }: { 
  data: DirectAcquisition; filter: string; serial: string;
  onPlay: (path: string) => Promise<string | null>;
  onPreview: (path: string, type: string) => void;
  onClosePreview: () => void;
  playingPath: string | null;
  previewPath: string | null;
  previewLocal: string | null;
  previewType: string | null;
  previewLoading: boolean;
}) {
  const s = useSort<WhatsAppMedia>("timestamp" as keyof WhatsAppMedia, "desc");
  const q = filter.toLowerCase();
  const media = useMemo(() => s.sort(data.whatsapp_media.filter(m =>
    !q || m.filename.toLowerCase().includes(q) || m.media_type.includes(q) || m.source_folder.toLowerCase().includes(q) || m.path.toLowerCase().includes(q) || (m.contact || "").toLowerCase().includes(q)
  )), [data.whatsapp_media, filter, s.key, s.dir]);
  const pg = usePagination(media);

  const highCount = data.whatsapp_media.filter(m => m.confidence === "high").length;
  const medCount = data.whatsapp_media.filter(m => m.confidence === "medium").length;
  const noneCount = data.whatsapp_media.filter(m => m.confidence === "none").length;

  return (
    <>
      <div style={{ marginBottom: 12, display: "flex", gap: 8, flexWrap: "wrap" }}>
        <span className="pill ok">📸 {data.whatsapp_media.filter(m => m.media_type === "image").length} Images</span>
        <span className="pill ok">🎥 {data.whatsapp_media.filter(m => m.media_type === "video").length} Videos</span>
        <span className="pill ok">🎵 {data.whatsapp_media.filter(m => m.media_type === "audio").length} Audio</span>
        <span className="pill ok">📄 {data.whatsapp_media.filter(m => m.media_type === "document").length} Docs</span>
        <span className="pill ok">💬 {data.whatsapp_chats.length} Chats</span>
        {highCount > 0 && <span className="pill ok" style={{ background: "rgba(16,185,129,0.15)", color: "#10b981", borderColor: "rgba(16,185,129,0.3)" }}>✅ {highCount} high</span>}
        {medCount > 0 && <span className="pill ok" style={{ background: "rgba(245,158,11,0.15)", color: "#f59e0b", borderColor: "rgba(245,158,11,0.3)" }}>~ {medCount} medium</span>}
        {noneCount > 0 && <span className="pill" style={{ background: "rgba(107,114,128,0.15)", color: "#6b7280", borderColor: "rgba(107,114,128,0.3)" }}>— {noneCount} unattributed</span>}
      </div>
      {highCount === 0 && medCount === 0 && data.whatsapp_media.length > 0 && (
        <div style={{ marginBottom: 12, padding: "8px 12px", background: "rgba(245,158,11,0.1)", border: "1px solid rgba(245,158,11,0.25)", borderRadius: 6, fontSize: 12, color: "#f59e0b" }}>
          ⚠️ No media-to-contact correlation achieved. WhatsApp database may be encrypted. Use <b>Deep Extraction</b> to decrypt and map contacts.
        </div>
      )}
      {media.length > 0 && (
        <>
        <table className="direct-table">
          <thead><tr>
            <th>#</th>
            <Th sortKey="filename" sort={s.toggle} arrow={s.arrow}>Filename</Th>
            <Th sortKey="media_type" sort={s.toggle} arrow={s.arrow}>Type</Th>
            <Th sortKey="contact" sort={s.toggle} arrow={s.arrow}>Contact</Th>
            <Th sortKey="confidence" sort={s.toggle} arrow={s.arrow}>Confidence</Th>
            <Th sortKey="source_folder" sort={s.toggle} arrow={s.arrow}>Source</Th>
            <Th sortKey="size" sort={s.toggle} arrow={s.arrow}>Size</Th>
            <Th sortKey="timestamp" sort={s.toggle} arrow={s.arrow}>Date</Th>
            <th>Actions</th>
          </tr></thead>
          <tbody>
            {pg.slice.map((m, idx) => {
              const confColor = m.confidence === "high" ? "#10b981" : m.confidence === "medium" ? "#f59e0b" : m.confidence === "low" ? "#f97316" : "#6b7280";
              const confLabel = m.confidence === "high" ? "High" : m.confidence === "medium" ? "Medium" : m.confidence === "low" ? "Low" : "None";
              const evidenceTip = m.evidence.length > 0 ? m.evidence.join("\n") + (m.source_database ? `\nDB: ${m.source_database}` : "") : "No evidence";
              return (
              <tr key={m.path} style={playingPath === m.path || previewPath === m.path ? { background: "rgba(255,165,0,0.1)" } : undefined}>
                <td className="mono small">{pg.start + idx + 1}</td>
                <td><b>{m.filename}</b></td>
                <td><MediaBadge type={m.media_type} /></td>
                <td className="mono small" style={{ color: m.contact ? "#2e7d32" : "#999" }} title={m.contact ? `${m.contact}${m.chat_jid ? `\nJID: ${m.chat_jid}` : ""}${m.message_timestamp ? `\nMsg time: ${new Date(m.message_timestamp).toLocaleString()}` : ""}` : "Unknown contact"}>
                  {m.contact ? (
                    <>
                      <span style={{ marginRight: 4 }}>{"\ud83d\udc64"}</span>
                      {m.contact.includes("(") ? m.contact.split("(")[0].trim() : m.contact.includes("@") ? m.contact.split("@")[0] : m.contact}
                    </>
                  ) : "\u2014"}
                </td>
                <td>
                  <span title={evidenceTip} style={{
                    display: "inline-block", padding: "1px 6px", borderRadius: 4,
                    fontSize: 10, fontWeight: 600, cursor: "help",
                    background: `${confColor}20`, color: confColor, border: `1px solid ${confColor}40`
                  }}>{confLabel}</span>
                </td>
                <td className="mono small" title={m.path}>{m.source_folder}</td>
                <td className="mono">{m.size > 0 ? `${(m.size / 1024 / 1024).toFixed(2)} MB` : "\u2014"}</td>
                <td className="small">{m.timestamp > 0 ? new Date(m.timestamp * 1000).toLocaleString() : "\u2014"}</td>
                <td>
                  {m.media_type === "audio" && (
                    <button 
                      className="mini" 
                      onClick={() => onPlay(m.path)}
                      style={playingPath === m.path ? { background: "#ff8c00" } : undefined}
                    >
                      {playingPath === m.path ? "🔄 Reload" : "▶ Play"}
                    </button>
                  )}
                  {m.media_type === "image" && (
                    <button 
                      className="mini" 
                      onClick={() => onPreview(m.path, "image")}
                      style={previewPath === m.path ? { background: "#ff8c00" } : undefined}
                    >
                      {previewPath === m.path ? "🔄 Reload" : "👁 Preview"}
                    </button>
                  )}
                  {m.media_type === "video" && (
                    <button 
                      className="mini" 
                      onClick={() => onPreview(m.path, "video")}
                      style={previewPath === m.path ? { background: "#ff8c00" } : undefined}
                    >
                      {previewPath === m.path ? "🔄 Reload" : "▶ Play"}
                    </button>
                  )}
                </td>
              </tr>
              );
            })}
          </tbody>
        </table>
        <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
        </>
      )}
      {playingPath && (
        <AudioPlayer 
          filename={data.whatsapp_media.find(m => m.path === playingPath)?.filename || "Audio"} 
          localPath="" 
        />
      )}
      {previewLoading && (
        <div style={{
          position: "fixed", top: 0, left: 0, right: 0, bottom: 0,
          background: "rgba(0,0,0,0.85)", zIndex: 10000,
          display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          padding: 24
        }}>
          <div style={{
            background: "#fff", borderRadius: 8, padding: 32, textAlign: "center"
          }}>
            <div style={{ fontSize: 32, marginBottom: 12 }}>⏳</div>
            <div style={{ fontWeight: "bold" }}>Pulling file from device...</div>
            <div style={{ fontSize: 12, color: "#999", marginTop: 8 }}>Please wait</div>
          </div>
        </div>
      )}
      {previewLocal && (
        <div style={{
          position: "fixed", top: 0, left: 0, right: 0, bottom: 0,
          background: "rgba(0,0,0,0.85)", zIndex: 10000,
          display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center",
          padding: 24
        }}>
          <div style={{
            background: "#fff", borderRadius: 8, padding: 16, maxWidth: "90vw", maxHeight: "90vh",
            display: "flex", flexDirection: "column", alignItems: "center", position: "relative"
          }}>
            <button
              onClick={onClosePreview}
              style={{
                position: "absolute", top: 8, right: 8, background: "#eee", border: "none",
                borderRadius: "50%", width: 32, height: 32, cursor: "pointer", fontSize: 16
              }}
            >✕</button>
            <div style={{ marginBottom: 12, fontWeight: "bold", fontSize: 14 }}>
              {previewType === "image" ? "📸" : "🎥"} {data.whatsapp_media.find(m => m.path === previewPath)?.filename}
            </div>
            {previewType === "image" && (
              <img src={convertFileSrc(previewLocal)} alt="preview" style={{ maxWidth: "80vw", maxHeight: "70vh", objectFit: "contain" }} />
            )}
            {previewType === "video" && (
              <video src={convertFileSrc(previewLocal)} controls autoPlay style={{ maxWidth: "80vw", maxHeight: "70vh" }} />
            )}
            <div style={{ marginTop: 8, fontSize: 11, color: "#999" }}>
              {previewLocal}
            </div>
          </div>
        </div>
      )}
      {data.whatsapp_chats.length > 0 && (
        <div style={{ marginTop: 16 }}>
          <h4 style={{ margin: "8px 0" }}>💬 Chat Information</h4>
          {data.whatsapp_chats.map((c, i) => (
            <div key={i} style={{ padding: 8, background: "#f5f5f5", marginBottom: 4, borderRadius: 4 }}>
              <b>{c.name || c.jid}</b>
              {c.last_message && <div className="small" style={{ color: "#666" }}>{c.last_message}</div>}
            </div>
          ))}
        </div>
      )}
      {data.whatsapp_media.length === 0 && data.whatsapp_chats.length === 0 && (
        <Empty msg="No WhatsApp data found. Make sure WhatsApp is installed on the device." />
      )}
    </>
  );
}

function CalendarTab({ data, filter }: { data: CalendarEvent[]; filter: string }) {
  const s = useSort<CalendarEvent>("start_time" as keyof CalendarEvent, "desc");
  const q = filter.toLowerCase();
  const filtered = useMemo(() => s.sort(data.filter(e =>
    !q || (e.title || "").toLowerCase().includes(q) || (e.location || "").toLowerCase().includes(q) || (e.description || "").toLowerCase().includes(q)
  )), [data, filter, s.key, s.dir]);
  const pg = usePagination(filtered);
  if (!filtered.length) return <Empty msg="No calendar events found." />;
  return (
    <>
    <table className="direct-table">
      <thead><tr>
        <th>#</th>
        <Th sortKey="title" sort={s.toggle} arrow={s.arrow}>Title</Th>
        <Th sortKey="location" sort={s.toggle} arrow={s.arrow}>Location</Th>
        <Th sortKey="start_time" sort={s.toggle} arrow={s.arrow}>Start</Th>
        <Th sortKey="end_time" sort={s.toggle} arrow={s.arrow}>End</Th>
      </tr></thead>
      <tbody>
        {pg.slice.map((e, idx) => (
          <tr key={e.id}>
            <td className="mono small">{pg.start + idx + 1}</td>
            <td><b>{e.title || "No title"}</b>{e.description && <div className="small" style={{ color: "#666", marginTop: 2 }}>{e.description.slice(0, 100)}</div>}</td>
            <td className="small">{e.location || "\u2014"}</td>
            <td className="small">{e.start_time > 0 ? new Date(e.start_time).toLocaleString() : "\u2014"}</td>
            <td className="small">{e.end_time > 0 ? new Date(e.end_time).toLocaleString() : "\u2014"}</td>
          </tr>
        ))}
      </tbody>
    </table>
    <PaginationBar page={pg.page} pages={pg.pages} total={pg.total} start={pg.start} setPage={pg.setPage} />
    </>
  );
}

function Empty({ msg }: { msg: string }) {
  return <div className="empty" style={{ padding: 40 }}><p>{msg}</p></div>;
}
