import { useState } from "react";
import type { UiRecovery, WhatsAppDeepResult, DirectAcquisition } from "../types";
import { fmtTime } from "../types";

type Step = "connect" | "apps" | "system" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "artifacts" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

interface MessagesStepProps {
  serial: string;
  waDeepResult: WhatsAppDeepResult | null;
  recovery: UiRecovery | null;
  directData: DirectAcquisition | null;
  onGoTo: (step: Step) => void;
}

type Tab = "chats" | "wa_calls" | "recovered";

export function MessagesStep({ serial, waDeepResult, recovery, directData, onGoTo }: MessagesStepProps) {
  const [tab, setTab] = useState<Tab>("chats");
  const [filter, setFilter] = useState("");

  const waChats = waDeepResult?.chats ?? directData?.whatsapp_chats ?? [];
  const waCalls = waDeepResult ? [] : (directData?.whatsapp_calls ?? []);
  const messages = recovery?.messages ?? [];

  const lf = filter.toLowerCase();
  const filteredChats = waChats.filter(c =>
    (c.jid || "").toLowerCase().includes(lf) ||
    (c.name || "").toLowerCase().includes(lf) ||
    (c.last_message || "").toLowerCase().includes(lf)
  );
  const filteredCalls = waCalls.filter(c =>
    (c.jid || "").toLowerCase().includes(lf) ||
    (c.name || "").toLowerCase().includes(lf)
  );
  const filteredMsgs = filter
    ? messages.filter(m =>
        (m.text || "").toLowerCase().includes(lf) ||
        (m.chat_jid || "").toLowerCase().includes(lf) ||
        (m.sender || "").toLowerCase().includes(lf)
      )
    : messages;

  const tabs: { key: Tab; label: string; icon: string; count: number }[] = [
    { key: "chats", label: "WhatsApp Chats", icon: "💬", count: waChats.length },
    { key: "wa_calls", label: "WhatsApp Calls", icon: "📞", count: waCalls.length },
    { key: "recovered", label: "Recovered Messages", icon: "📝", count: messages.length },
  ];

  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">
          💬 Messages
          <span className="sep" />
          <span className="hint">
            {waChats.length} chats · {messages.length} messages{waCalls.length > 0 ? ` · ${waCalls.length} WA calls` : ""}
          </span>
          <input
            type="text"
            placeholder="Filter messages…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            style={{
              marginLeft: "auto", padding: "4px 10px", fontSize: 11,
              background: "var(--bg-primary)", border: "1px solid var(--border)",
              borderRadius: 4, color: "var(--text-primary)", width: 200,
            }}
          />
        </div>

        <div className="fx-tabs">
          {tabs.map((t) => (
            <button key={t.key} className={`fx-tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
              {t.icon} {t.label}
              {t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
            </button>
          ))}
        </div>

        <div className="panel-body" style={{ overflow: "auto" }}>
          {!serial ? (
            <div className="fx-empty">
              <span className="fx-empty-icon">📱</span>
              <span className="fx-empty-text">Connect a device to view messages</span>
            </div>
          ) : tab === "chats" ? (
            waChats.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">💬</span>
                <span className="fx-empty-text">No WhatsApp chats found — run WhatsApp deep extraction from Overview to populate this view</span>
              </div>
            ) : (
              <div className="fx-scroll">
                <table className="fx-table">
                  <thead><tr><th>Chat JID</th><th>Name</th><th>Messages</th><th>Last Message</th><th>Last Active</th></tr></thead>
                  <tbody>
                    {filteredChats.map((c, i) => (
                      <tr key={i}>
                        <td className="mono" style={{ fontSize: 10.5 }}>{c.jid}</td>
                        <td style={{ fontWeight: 600 }}>{c.name || "—"}</td>
                        <td>{c.message_count}</td>
                        <td style={{ maxWidth: 300, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{c.last_message || "—"}</td>
                        <td style={{ whiteSpace: "nowrap" }}>{fmtTime(c.last_timestamp)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          ) : tab === "wa_calls" ? (
            waCalls.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">📞</span>
                <span className="fx-empty-text">No WhatsApp call records — run direct acquisition from Overview to populate</span>
              </div>
            ) : (
              <div className="fx-scroll">
                <table className="fx-table">
                  <thead><tr><th>Contact</th><th>Type</th><th>Duration</th><th>Time</th></tr></thead>
                  <tbody>
                    {filteredCalls.map((c, i) => (
                      <tr key={i}>
                        <td style={{ fontWeight: 600 }}>{c.name || c.jid}</td>
                        <td><span className={`fx-row-label ${c.call_type === "incoming" ? "fx-cat-call" : "fx-cat-power"}`}>{c.call_type}</span></td>
                        <td>{c.duration > 0 ? `${c.duration}s` : "—"}</td>
                        <td>{fmtTime(c.timestamp)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          ) : (
            messages.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">📝</span>
                <span className="fx-empty-text">No recovered messages — run database recovery from the Recovery tab</span>
              </div>
            ) : (
              <div className="fx-scroll">
                <table className="fx-table">
                  <thead><tr><th>Time</th><th>Chat</th><th>Direction</th><th>Message</th><th>Source</th></tr></thead>
                  <tbody>
                    {filteredMsgs.slice(0, 500).map((m, i) => (
                      <tr key={i}>
                        <td style={{ whiteSpace: "nowrap" }}>{fmtTime(m.timestamp)}</td>
                        <td className="mono" style={{ fontSize: 10.5 }}>{m.chat_jid || m.sender || "—"}</td>
                        <td>{m.from_me ? "→ Sent" : "← Received"}</td>
                        <td style={{ maxWidth: 400, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.text || "—"}</td>
                        <td><span className={`fx-row-label ${m.source === "wal" ? "fx-cat-notification" : m.source === "freelist" ? "fx-cat-call" : "fx-cat-power"}`}>{m.source}</span></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          )}
        </div>

        <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
          <button className="mini" onClick={() => onGoTo("media")}>← Media</button>
          <button className="mini" onClick={() => onGoTo("deleted")}>Deleted Data →</button>
        </div>
      </div>
    </section>
  );
}
