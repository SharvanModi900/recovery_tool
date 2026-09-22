import { useState } from "react";
import type { UiRecovery, WhatsAppDeepResult, DirectAcquisition, Step } from "../types";
import { fmtTime } from "../types";
import { DataStepLayout, EmptyState, ConnectPrompt } from "./DataStepLayout";

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

  const tabs = [
    { key: "chats" as const, label: "WhatsApp Chats", icon: "💬", count: waChats.length },
    { key: "wa_calls" as const, label: "WhatsApp Calls", icon: "📞", count: waCalls.length },
    { key: "recovered" as const, label: "Recovered Messages", icon: "📝", count: messages.length },
  ];

  const hint = `${waChats.length} chats · ${messages.length} messages${waCalls.length > 0 ? ` · ${waCalls.length} WA calls` : ""}`;

  return (
    <DataStepLayout
      title="💬 Messages"
      hint={hint}
      filter={filter}
      onFilterChange={setFilter}
      filterPlaceholder="Filter messages…"
      tabs={tabs}
      activeTab={tab}
      onTabChange={(k) => setTab(k as Tab)}
      nav={<>
        <button className="mini" onClick={() => onGoTo("media")}>← Media</button>
        <button className="mini" onClick={() => onGoTo("forensics")}>Forensics →</button>
      </>}
    >
      {!serial ? (
        <ConnectPrompt onGoTo={onGoTo} />
      ) : tab === "chats" ? (
        waChats.length === 0 ? (
          <EmptyState icon="💬" text="No WhatsApp chats found — run WhatsApp deep extraction from Overview to populate this view" cta={{ label: "Go to Overview", target: "connect", onGoTo }} />
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
          <EmptyState icon="📞" text="No WhatsApp call records — run direct acquisition from Overview to populate" cta={{ label: "Go to Overview", target: "connect", onGoTo }} />
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
          <EmptyState icon="📝" text="No recovered messages — run database recovery from the Recovery tab" />
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
    </DataStepLayout>
  );
}
