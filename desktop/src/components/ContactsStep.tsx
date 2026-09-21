import { useState } from "react";
import type { UiRecovery, DirectAcquisition } from "../types";

type Step = "connect" | "apps" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "forensics" | "device_tools" | "recover" | "export" | "case" | "timeline";

interface ContactsStepProps {
  serial: string;
  recovery: UiRecovery | null;
  directData: DirectAcquisition | null;
  onGoTo: (step: Step) => void;
}

type Tab = "recovery" | "device" | "whatsapp";

export function ContactsStep({ serial, recovery, directData, onGoTo }: ContactsStepProps) {
  const [tab, setTab] = useState<Tab>("recovery");
  const [filter, setFilter] = useState("");

  const recoveryContacts = recovery?.contacts_list ?? [];
  const deviceContacts = directData?.contacts ?? [];
  const waContacts = directData?.whatsapp_chats ?? [];

  const lf = filter.toLowerCase();

  const filteredRecovery = recoveryContacts.filter(([name, number]) =>
    (name || "").toLowerCase().includes(lf) || (number || "").includes(lf)
  );
  const filteredDevice = deviceContacts.filter(c =>
    (c.display_name || "").toLowerCase().includes(lf) ||
    (c.phone || "").includes(lf) ||
    (c.email || "").toLowerCase().includes(lf)
  );
  const filteredWa = waContacts.filter(c =>
    (c.jid || "").toLowerCase().includes(lf) || (c.name || "").toLowerCase().includes(lf)
  );

  const tabs: { key: Tab; label: string; icon: string; count: number }[] = [
    { key: "recovery", label: "Recovered Contacts", icon: "📇", count: recoveryContacts.length },
    { key: "device", label: "Device Contacts", icon: "📱", count: deviceContacts.length },
    { key: "whatsapp", label: "WhatsApp Contacts", icon: "💬", count: waContacts.length },
  ];

  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">
          👥 Contacts
          <span className="sep" />
          <span className="hint">
            {recoveryContacts.length + deviceContacts.length + waContacts.length} total contacts across sources
          </span>
          <input
            type="text"
            placeholder="Filter contacts…"
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
              <span className="fx-empty-text">Connect a device to view contacts</span>
            </div>
          ) : tab === "recovery" ? (
            recoveryContacts.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">📇</span>
                <span className="fx-empty-text">No recovered contacts — run database recovery from the Recovery tab</span>
              </div>
            ) : (
              <div className="fx-scroll">
                <div className="fx-stats">
                  <span className="fx-stat">{filteredRecovery.length} contact{filteredRecovery.length !== 1 ? "s" : ""}</span>
                </div>
                <table className="fx-table">
                  <thead><tr><th>#</th><th>Name</th><th>Phone Number</th></tr></thead>
                  <tbody>
                    {filteredRecovery.map(([name, number], i) => (
                      <tr key={i}>
                        <td style={{ color: "var(--text-muted)", fontSize: 10 }}>{i + 1}</td>
                        <td style={{ fontWeight: 600 }}>{name || "(unknown)"}</td>
                        <td className="mono">{number || "—"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          ) : tab === "device" ? (
            deviceContacts.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">📱</span>
                <span className="fx-empty-text">No device contacts — run direct acquisition from Overview → System Data</span>
                <button className="mini" onClick={() => onGoTo("connect")} style={{ marginTop: 8, border: "1px solid var(--line)", color: "var(--muted)" }}>Go to Overview</button>
              </div>
            ) : (
              <div className="fx-scroll">
                <div className="fx-stats">
                  <span className="fx-stat">{filteredDevice.length} contact{filteredDevice.length !== 1 ? "s" : ""}</span>
                  <span className="fx-stat info">{filteredDevice.filter(c => c.email).length} with email</span>
                </div>
                <table className="fx-table">
                  <thead><tr><th>Name</th><th>Phone</th><th>Email</th><th>Source</th></tr></thead>
                  <tbody>
                    {filteredDevice.map((c) => (
                      <tr key={c.id}>
                        <td style={{ fontWeight: 600 }}>{c.display_name || "(no name)"}</td>
                        <td className="mono">{c.phone || "—"}</td>
                        <td style={{ color: "var(--text-secondary)" }}>{c.email || "—"}</td>
                        <td><span className="fx-row-label fx-cat-power">{c.source}</span></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          ) : (
            waContacts.length === 0 ? (
              <div className="fx-empty">
                <span className="fx-empty-icon">💬</span>
                <span className="fx-empty-text">No WhatsApp contacts — run direct acquisition or deep extraction from Overview</span>
                <button className="mini" onClick={() => onGoTo("connect")} style={{ marginTop: 8, border: "1px solid var(--line)", color: "var(--muted)" }}>Go to Overview</button>
              </div>
            ) : (
              <div className="fx-scroll">
                <div className="fx-stats">
                  <span className="fx-stat">{filteredWa.length} WhatsApp contact{filteredWa.length !== 1 ? "s" : ""}</span>
                </div>
                <table className="fx-table">
                  <thead><tr><th>JID</th><th>Display Name</th><th>Messages</th><th>Last Active</th></tr></thead>
                  <tbody>
                    {filteredWa.map((c, i) => (
                      <tr key={i}>
                        <td className="mono" style={{ fontSize: 10.5 }}>{c.jid}</td>
                        <td style={{ fontWeight: 600 }}>{c.name || "—"}</td>
                        <td>{c.message_count}</td>
                        <td style={{ whiteSpace: "nowrap" }}>{c.last_timestamp ? new Date(c.last_timestamp).toLocaleDateString() : "—"}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )
          )}
        </div>

        <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
          <button className="mini" onClick={() => onGoTo("calls")}>← Calls</button>
          <button className="mini" onClick={() => onGoTo("sim_forensics")}>SIM Forensics →</button>
        </div>
      </div>
    </section>
  );
}
