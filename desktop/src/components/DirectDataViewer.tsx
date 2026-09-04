import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { DirectAcquisition, CallLog, SmsMessage, ContactEntry, CallRecording, CalendarEvent } from "../types";
import { AudioPlayer, MediaBadge, TypeBadge } from "./index";
import * as api from "../api";

type Tab = "calls" | "sms" | "contacts" | "recordings" | "whatsapp" | "calendar";

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
  const q = filter.toLowerCase();
  const filtered = data.filter(c =>
    !q || c.number.includes(q) || (c.name || "").toLowerCase().includes(q) || c.call_type.includes(q)
  );
  if (!filtered.length) return <Empty msg="No call logs found." />;
  return (
    <table className="direct-table">
      <thead><tr><th>#</th><th>Number</th><th>Name</th><th>Type</th><th>Duration</th><th>Date</th></tr></thead>
      <tbody>
        {filtered.map((c, i) => (
          <tr key={c.id}>
            <td className="mono small">{i + 1}</td>
            <td className="mono">{c.number}</td>
            <td><b>{c.name || "Unknown"}</b></td>
            <td><TypeBadge type={c.call_type} /></td>
            <td className="mono small">{c.duration}s</td>
            <td className="small">{c.timestamp > 0 ? new Date(c.timestamp).toLocaleString() : "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function SmsTab({ data, filter }: { data: SmsMessage[]; filter: string }) {
  const q = filter.toLowerCase();
  const filtered = data.filter(m =>
    !q || m.address.includes(q) || (m.body || "").toLowerCase().includes(q) || m.msg_type.includes(q)
  );
  const [selectedMsg, setSelectedMsg] = useState<SmsMessage | null>(null);
  
  if (!filtered.length) return <Empty msg="No messages found." />;
  return (
    <>
      <table className="direct-table">
        <thead><tr><th>#</th><th>Address</th><th>Body</th><th>Type</th><th>Date</th></tr></thead>
        <tbody>
          {filtered.map((m, i) => (
            <tr key={m.id} style={{ cursor: "pointer" }} onClick={() => setSelectedMsg(m)}>
              <td className="mono small">{i + 1}</td>
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
  const q = filter.toLowerCase();
  const filtered = data.filter(c =>
    !q || (c.display_name || "").toLowerCase().includes(q) || (c.phone || "").includes(q) || (c.email || "").toLowerCase().includes(q)
  );
  if (!filtered.length) return <Empty msg="No contacts found." />;
  return (
    <table className="direct-table">
      <thead><tr><th>#</th><th>Name</th><th>Phone</th><th>Email</th></tr></thead>
      <tbody>
        {filtered.map((c, i) => (
          <tr key={c.id}>
            <td className="mono small">{i + 1}</td>
            <td><b>{c.display_name || "Unknown"}</b></td>
            <td className="mono">{c.phone || "—"}</td>
            <td className="mono small">{c.email || "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function RecordingsTab({ data, filter, playingPath, onPlay }: {
  data: CallRecording[]; filter: string; playingPath: string | null; onPlay: (path: string) => void;
}) {
  const q = filter.toLowerCase();
  const filtered = data.filter(r =>
    !q || r.filename.toLowerCase().includes(q) || r.path.toLowerCase().includes(q)
  );
  if (!filtered.length) return <Empty msg="No call recordings found." />;
  return (
    <>
      <table className="direct-table">
        <thead><tr><th>#</th><th>Filename</th><th>Path</th><th>Size</th><th>Actions</th></tr></thead>
        <tbody>
          {filtered.map((r, i) => (
            <tr key={r.path} style={playingPath === r.path ? { background: "rgba(255,165,0,0.1)" } : undefined}>
              <td className="mono small">{i + 1}</td>
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
  const q = filter.toLowerCase();
  const media = data.whatsapp_media.filter(m =>
    !q || m.filename.toLowerCase().includes(q) || m.media_type.includes(q) || m.source_folder.toLowerCase().includes(q) || m.path.toLowerCase().includes(q)
  );

  return (
    <>
      <div style={{ marginBottom: 12, display: "flex", gap: 8 }}>
        <span className="pill ok">📸 {data.whatsapp_media.filter(m => m.media_type === "image").length} Images</span>
        <span className="pill ok">🎥 {data.whatsapp_media.filter(m => m.media_type === "video").length} Videos</span>
        <span className="pill ok">🎵 {data.whatsapp_media.filter(m => m.media_type === "audio").length} Audio</span>
        <span className="pill ok">📄 {data.whatsapp_media.filter(m => m.media_type === "document").length} Docs</span>
        <span className="pill ok">💬 {data.whatsapp_chats.length} Chats</span>
      </div>
      {media.length > 0 && (
        <table className="direct-table">
          <thead><tr><th>#</th><th>Filename</th><th>Type</th><th>Contact</th><th>Source</th><th>Size</th><th>Actions</th></tr></thead>
          <tbody>
            {media.map((m, i) => (
              <tr key={m.path} style={playingPath === m.path || previewPath === m.path ? { background: "rgba(255,165,0,0.1)" } : undefined}>
                <td className="mono small">{i + 1}</td>
                <td><b>{m.filename}</b></td>
                <td><MediaBadge type={m.media_type} /></td>
                <td className="mono small" style={{ color: m.contact ? "#2e7d32" : "#999" }}>{m.contact || "—"}</td>
                <td className="mono small" title={m.path}>{m.source_folder}</td>
                <td className="mono">{m.size > 0 ? `${(m.size / 1024 / 1024).toFixed(2)} MB` : "—"}</td>
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
            ))}
          </tbody>
        </table>
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
  const q = filter.toLowerCase();
  const filtered = data.filter(e =>
    !q || (e.title || "").toLowerCase().includes(q) || (e.location || "").toLowerCase().includes(q) || (e.description || "").toLowerCase().includes(q)
  );
  if (!filtered.length) return <Empty msg="No calendar events found." />;
  return (
    <table className="direct-table">
      <thead><tr><th>#</th><th>Title</th><th>Location</th><th>Start</th><th>End</th></tr></thead>
      <tbody>
        {filtered.map((e, i) => (
          <tr key={e.id}>
            <td className="mono small">{i + 1}</td>
            <td><b>{e.title || "No title"}</b>{e.description && <div className="small" style={{ color: "#666", marginTop: 2 }}>{e.description.slice(0, 100)}</div>}</td>
            <td className="small">{e.location || "—"}</td>
            <td className="small">{e.start_time > 0 ? new Date(e.start_time).toLocaleString() : "—"}</td>
            <td className="small">{e.end_time > 0 ? new Date(e.end_time).toLocaleString() : "—"}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Empty({ msg }: { msg: string }) {
  return <div className="empty" style={{ padding: 40 }}><p>{msg}</p></div>;
}
