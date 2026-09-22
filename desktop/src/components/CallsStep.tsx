import { useState } from "react";
import type { CallEvent, CallRecording, CallLog, WhatsAppCall, Step } from "../types";
import { fmtTime } from "../types";
import { DataStepLayout, EmptyState, ConnectPrompt } from "./DataStepLayout";

interface CallsStepProps {
  serial: string;
  callEvents: CallEvent[];
  callRecordings: CallRecording[];
  directData: { call_logs: CallLog[]; whatsapp_calls: WhatsAppCall[] } | null;
  callMonitorActive: boolean;
  onGoTo: (step: Step) => void;
}

type Tab = "logs" | "recordings" | "monitor" | "wa_calls";

export function CallsStep({ serial, callEvents, callRecordings, directData, callMonitorActive, onGoTo }: CallsStepProps) {
  const [tab, setTab] = useState<Tab>("logs");
  const [filter, setFilter] = useState("");

  const callLogs = directData?.call_logs ?? [];
  const waCalls = directData?.whatsapp_calls ?? [];

  const lf = filter.toLowerCase();
  const filteredLogs = callLogs.filter(c =>
    (c.number || "").includes(lf) || (c.name || "").toLowerCase().includes(lf)
  );
  const filteredRecs = callRecordings.filter(r =>
    (r.filename || "").toLowerCase().includes(lf) ||
    (r.matched_name || "").toLowerCase().includes(lf) ||
    (r.matched_number || "").includes(lf)
  );
  const filteredEvents = callEvents.filter(e =>
    (e.phone_number || "").includes(lf) || (e.contact_name || "").toLowerCase().includes(lf)
  );
  const filteredWaCalls = waCalls.filter(c =>
    (c.jid || "").toLowerCase().includes(lf) || (c.name || "").toLowerCase().includes(lf)
  );

  const tabs = [
    { key: "logs" as const, label: "Call Logs", icon: "📋", count: callLogs.length },
    { key: "recordings" as const, label: "Recordings", icon: "🎙", count: callRecordings.length },
    { key: "monitor" as const, label: "Live Monitor", icon: "📡", count: callEvents.length,
      badge: tab === "monitor" && callMonitorActive ? <span className="live-dot" style={{ marginLeft: 4 }}>●</span> : undefined },
    { key: "wa_calls" as const, label: "WhatsApp Calls", icon: "📞", count: waCalls.length },
  ];

  const hint = `${callLogs.length} logs · ${callRecordings.length} recordings · ${callEvents.length} monitored${waCalls.length > 0 ? ` · ${waCalls.length} WA` : ""}`;

  const callTypeIcon = (t: string) => {
    switch (t.toLowerCase()) {
      case "incoming": return "📥";
      case "outgoing": return "📤";
      case "missed": return "❌";
      case "rejected": return "✋";
      default: return "📞";
    }
  };

  return (
    <DataStepLayout
      title="📞 Calls"
      hint={hint}
      filter={filter}
      onFilterChange={setFilter}
      filterPlaceholder="Filter calls…"
      tabs={tabs}
      activeTab={tab}
      onTabChange={(k) => setTab(k as Tab)}
      nav={<>
        <button className="mini" onClick={() => onGoTo("messages")}>← Messages</button>
        <button className="mini" onClick={() => onGoTo("contacts")}>Contacts →</button>
      </>}
    >
      {!serial ? (
        <ConnectPrompt onGoTo={onGoTo} />
      ) : tab === "logs" ? (
        callLogs.length === 0 ? (
          <EmptyState icon="📋" text="No call logs — run direct acquisition from Overview to populate" cta={{ label: "Go to Overview", target: "connect", onGoTo }} />
        ) : (
          <div className="fx-scroll">
            <table className="fx-table">
              <thead><tr><th></th><th>Number</th><th>Name</th><th>Type</th><th>Duration</th><th>Time</th></tr></thead>
              <tbody>
                {filteredLogs.map((c) => (
                  <tr key={c.id}>
                    <td>{callTypeIcon(c.call_type)}</td>
                    <td className="mono">{c.number}</td>
                    <td style={{ fontWeight: 600 }}>{c.name || "—"}</td>
                    <td><span className={`fx-row-label ${c.call_type === "Incoming" ? "fx-cat-call" : c.call_type === "Missed" ? "fx-cat-notification" : "fx-cat-power"}`}>{c.call_type}</span></td>
                    <td>{c.duration > 0 ? `${c.duration}s` : "—"}</td>
                    <td style={{ whiteSpace: "nowrap" }}>{fmtTime(c.timestamp)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      ) : tab === "recordings" ? (
        callRecordings.length === 0 ? (
          <EmptyState icon="🎙" text="No call recordings found — start call monitor to capture recordings" />
        ) : (
          <div className="fx-scroll">
            <table className="fx-table">
              <thead><tr><th>Filename</th><th>Matched Contact</th><th>Matched Number</th><th>Type</th><th>Size</th><th>Time</th></tr></thead>
              <tbody>
                {filteredRecs.map((r, i) => (
                  <tr key={i}>
                    <td className="mono" style={{ fontSize: 10.5 }}>{r.filename}</td>
                    <td style={{ fontWeight: 600 }}>{r.matched_name || "—"}</td>
                    <td className="mono">{r.matched_number || "—"}</td>
                    <td>{r.matched_call_type || "—"}</td>
                    <td>{r.size > 0 ? `${(r.size / 1024).toFixed(0)} KB` : "—"}</td>
                    <td style={{ whiteSpace: "nowrap" }}>{fmtTime(r.timestamp)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      ) : tab === "monitor" ? (
        callEvents.length === 0 ? (
          <EmptyState
            icon="📡"
            text={callMonitorActive ? "Monitoring active — waiting for call events…" : "Call monitor not active — start it from Overview → Calls tab"}
            cta={!callMonitorActive ? { label: "Go to Overview", target: "connect", onGoTo } : undefined}
          />
        ) : (
          <div className="fx-scroll fx-list">
            {filteredEvents.map((e, i) => (
              <div key={i} className="fx-row">
                <span className="fx-row-icon">{callTypeIcon(e.event_type)}</span>
                <span className={`fx-row-label ${e.event_type === "Incoming" || e.event_type === "Outgoing" ? "fx-cat-call" : e.event_type === "Missed" ? "fx-cat-notification" : "fx-cat-power"}`}>
                  {e.event_type}
                </span>
                <div className="fx-row-content">
                  <span className="fx-row-title">{e.contact_name || e.phone_number || "Unknown"}</span>
                  {e.duration != null && e.duration > 0 && <span className="fx-row-detail"> — {e.duration}s</span>}
                </div>
                <span className="fx-row-time">{fmtTime(e.timestamp)}</span>
              </div>
            ))}
          </div>
        )
      ) : (
        waCalls.length === 0 ? (
          <EmptyState icon="📞" text="No WhatsApp call records — run direct acquisition from Overview" cta={{ label: "Go to Overview", target: "connect", onGoTo }} />
        ) : (
          <div className="fx-scroll">
            <table className="fx-table">
              <thead><tr><th>Contact</th><th>Type</th><th>Duration</th><th>Time</th></tr></thead>
              <tbody>
                {filteredWaCalls.map((c, i) => (
                  <tr key={i}>
                    <td style={{ fontWeight: 600 }}>{c.name || c.jid}</td>
                    <td><span className={`fx-row-label ${c.call_type === "incoming" ? "fx-cat-call" : "fx-cat-power"}`}>{c.call_type}</span></td>
                    <td>{c.duration > 0 ? `${c.duration}s` : "—"}</td>
                    <td style={{ whiteSpace: "nowrap" }}>{fmtTime(c.timestamp)}</td>
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
