import { useState, useEffect } from "react";
import type { WifiNetwork, BluetoothDevice, DeviceAccount, PowerEvent, NotificationEntry, DictionaryWord } from "../types";
import { fmtTime } from "../types";
import { collectWifi, collectBluetooth, collectAccounts, collectPowerEvents, collectNotifications, collectDictionary } from "../api";

interface DeepForensicsPanelProps {
  serial: string;
}

type SubTab = "wifi" | "bluetooth" | "accounts" | "power" | "notifications" | "dictionary";

export function DeepForensicsPanel({ serial }: DeepForensicsPanelProps) {
  const [tab, setTab] = useState<SubTab>("wifi");
  const [wifi, setWifi] = useState<WifiNetwork[] | null>(null);
  const [bt, setBt] = useState<BluetoothDevice[] | null>(null);
  const [accounts, setAccounts] = useState<DeviceAccount[] | null>(null);
  const [power, setPower] = useState<PowerEvent[] | null>(null);
  const [notifications, setNotifications] = useState<NotificationEntry[] | null>(null);
  const [dictionary, setDictionary] = useState<DictionaryWord[] | null>(null);
  const [busy, setBusy] = useState(false);

  const load = async (t: SubTab) => {
    setBusy(true);
    try {
      switch (t) {
        case "wifi": if (!wifi) setWifi(await collectWifi(serial)); break;
        case "bluetooth": if (!bt) setBt(await collectBluetooth(serial)); break;
        case "accounts": if (!accounts) setAccounts(await collectAccounts(serial)); break;
        case "power": if (!power) setPower(await collectPowerEvents(serial)); break;
        case "notifications": if (!notifications) setNotifications(await collectNotifications(serial)); break;
        case "dictionary": if (!dictionary) setDictionary(await collectDictionary(serial)); break;
      }
    } catch { /* ignore */ }
    setBusy(false);
  };

  useEffect(() => { load(tab); }, [tab, serial]);

  const tabs: { key: SubTab; label: string; icon: string; count?: number }[] = [
    { key: "wifi", label: "WiFi", icon: "📶", count: wifi?.length },
    { key: "bluetooth", label: "Bluetooth", icon: "🔵", count: bt?.length },
    { key: "accounts", label: "Accounts", icon: "👤", count: accounts?.length },
    { key: "power", label: "Power", icon: "🔋", count: power?.length },
    { key: "notifications", label: "Notifications", icon: "🔔", count: notifications?.length },
    { key: "dictionary", label: "Dictionary", icon: "⌨️", count: dictionary?.length },
  ];

  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">🔍 Deep Forensics</div>
      <div className="fx-tabs">
        {tabs.map((t) => (
          <button key={t.key} className={`fx-tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.icon} {t.label}
            {t.count != null && t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
          </button>
        ))}
      </div>
      <div className="panel-body" style={{ overflow: "auto" }}>
        {tab === "wifi" && wifi && <WifiTable data={wifi} onRefresh={() => { setWifi(null); load("wifi"); }} />}
        {tab === "bluetooth" && bt && <BluetoothTable data={bt} onRefresh={() => { setBt(null); load("bluetooth"); }} />}
        {tab === "accounts" && accounts && <AccountsTable data={accounts} onRefresh={() => { setAccounts(null); load("accounts"); }} />}
        {tab === "power" && power && <PowerTable data={power} onRefresh={() => { setPower(null); load("power"); }} />}
        {tab === "notifications" && notifications && <NotificationsTable data={notifications} onRefresh={() => { setNotifications(null); load("notifications"); }} />}
        {tab === "dictionary" && dictionary && <DictionaryTable data={dictionary} onRefresh={() => { setDictionary(null); load("dictionary"); }} />}
        {!busy && (tab === "wifi" && !wifi) || (tab === "bluetooth" && !bt) || (tab === "accounts" && !accounts) || (tab === "power" && !power) || (tab === "notifications" && !notifications) || (tab === "dictionary" && !dictionary) ? (
          <div className="fx-empty"><span className="spin" /><span className="fx-empty-text">Collecting data from device…</span></div>
        ) : null}
      </div>
    </div>
  );
}

function WifiTable({ data, onRefresh }: { data: WifiNetwork[]; onRefresh: () => void }) {
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} network{data.length !== 1 ? "s" : ""}</span>
        <span className="fx-stat info">{data.filter(n => n.security && n.security !== "open").length} secured</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">📶</span><span className="fx-empty-text">No saved WiFi networks found on this device</span></div>
      ) : (
        <div className="fx-scroll">
          <table className="fx-table">
            <thead><tr><th>SSID</th><th>BSSID</th><th>Security</th><th>Freq</th></tr></thead>
            <tbody>{data.map((n, i) => (
              <tr key={i}>
                <td style={{ fontWeight: 600 }}>{n.ssid || "(hidden)"}</td>
                <td className="mono">{n.bssid || "—"}</td>
                <td><span className={`fx-row-label ${n.security && n.security !== "open" ? "fx-cat-call" : "fx-cat-power"}`}>{n.security || "open"}</span></td>
                <td>{n.freq_mhz ? `${n.freq_mhz} MHz` : "—"}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
    </>
  );
}

function BluetoothTable({ data, onRefresh }: { data: BluetoothDevice[]; onRefresh: () => void }) {
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} paired device{data.length !== 1 ? "s" : ""}</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">🔵</span><span className="fx-empty-text">No paired Bluetooth devices found</span></div>
      ) : (
        <div className="fx-scroll">
          <table className="fx-table">
            <thead><tr><th>Name</th><th>MAC Address</th><th>Type</th><th>State</th></tr></thead>
            <tbody>{data.map((d, i) => (
              <tr key={i}>
                <td style={{ fontWeight: 600 }}>{d.name || d.alias || "(unknown)"}</td>
                <td className="mono">{d.mac_address}</td>
                <td>{d.device_type || "—"}</td>
                <td><span className={`fx-row-label fx-cat-call`}>{d.bond_state}</span></td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
    </>
  );
}

function AccountsTable({ data, onRefresh }: { data: DeviceAccount[]; onRefresh: () => void }) {
  const grouped = data.reduce((acc, a) => {
    const t = a.account_type;
    if (!acc[t]) acc[t] = [];
    acc[t].push(a);
    return acc;
  }, {} as Record<string, DeviceAccount[]>);

  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} account{data.length !== 1 ? "s" : ""}</span>
        <span className="fx-stat info">{Object.keys(grouped).length} types</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">👤</span><span className="fx-empty-text">No accounts found on this device</span></div>
      ) : (
        <div className="fx-scroll">
          {Object.entries(grouped).map(([type, accs]) => (
            <div key={type}>
              <div className="fx-section-head">{type} ({accs.length})</div>
              <table className="fx-table">
                <tbody>{accs.map((a, i) => (
                  <tr key={i}>
                    <td className="mono" style={{ fontSize: 10.5 }}>{a.account_name}</td>
                    <td style={{ textAlign: "right", color: "var(--muted)", fontSize: 10 }}>
                      {a.authority_count > 0 ? `${a.authority_count} sync${a.authority_count !== 1 ? "s" : ""}` : "—"}
                    </td>
                  </tr>
                ))}</tbody>
              </table>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function PowerTable({ data, onRefresh }: { data: PowerEvent[]; onRefresh: () => void }) {
  const icons: Record<string, string> = { plug: "🔌", unplug: "🔋", charge_start: "⚡", charge_full: "✅", shutdown: "⏻", reboot: "🔄", current_state: "📊", history_start: "📜" };
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} event{data.length !== 1 ? "s" : ""}</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">🔋</span><span className="fx-empty-text">No power events recorded</span></div>
      ) : (
        <div className="fx-scroll fx-list">
          {data.map((e, i) => (
            <div key={i} className="fx-row">
              <span className="fx-row-icon">{icons[e.event_type] || "•"}</span>
              <span className={`fx-row-label fx-cat-power`}>{e.event_type.replace("_", " ")}</span>
              <div className="fx-row-content">
                <span className="fx-row-title">{e.battery_level}% battery</span>
                {e.detail && <span className="fx-row-detail"> — {e.detail}</span>}
              </div>
              <span className="fx-row-time">+{(e.timestamp_ms / 1000).toFixed(1)}s</span>
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function NotificationsTable({ data, onRefresh }: { data: NotificationEntry[]; onRefresh: () => void }) {
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} notification{data.length !== 1 ? "s" : ""}</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">🔔</span><span className="fx-empty-text">No notifications in log</span></div>
      ) : (
        <div className="fx-scroll fx-list">
          {data.map((n, i) => (
            <div key={i} className="fx-row">
              <span className="fx-row-icon">🔔</span>
              <span className={`fx-row-label fx-cat-notification`}>{n.package.split(".").pop()}</span>
              <div className="fx-row-content">
                {n.title && <span className="fx-row-title">{n.title}</span>}
                {n.text && <div className="fx-row-detail">{n.text}</div>}
              </div>
              {n.timestamp && <span className="fx-row-time">{fmtTime(n.timestamp)}</span>}
            </div>
          ))}
        </div>
      )}
    </>
  );
}

function DictionaryTable({ data, onRefresh }: { data: DictionaryWord[]; onRefresh: () => void }) {
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.length} word{data.length !== 1 ? "s" : ""}</span>
        <button className="mini" onClick={onRefresh} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.length === 0 ? (
        <div className="fx-empty"><span className="fx-empty-icon">⌨️</span><span className="fx-empty-text">No user dictionary words found</span></div>
      ) : (
        <div className="fx-scroll">
          <table className="fx-table">
            <thead><tr><th>Word</th><th>Frequency</th><th>Locale</th></tr></thead>
            <tbody>{data.slice(0, 200).map((w, i) => (
              <tr key={i}>
                <td style={{ fontWeight: 600 }}>{w.word}</td>
                <td>{w.frequency}</td>
                <td style={{ color: "var(--muted)" }}>{w.locale || "—"}</td>
              </tr>
            ))}</tbody>
          </table>
        </div>
      )}
    </>
  );
}
