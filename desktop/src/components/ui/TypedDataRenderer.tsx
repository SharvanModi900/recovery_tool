import type {
  WifiNetwork, BluetoothDevice, PowerEvent, NotificationEntry,
  DictionaryWord, BrowserForensicsResult, LocationForensicsResult,
  NetworkForensicsResult, DeletedSmsResult, ThumbnailRecoveryResult, SmsRecord,
} from "../../types";
import { fmtTime, fmtSize } from "../../types";

const th: React.CSSProperties = { padding: "4px 8px", textAlign: "left", fontSize: 10, color: "#999", borderBottom: "1px solid #333", fontWeight: 600, position: "sticky", top: 0, background: "var(--bg, #0f0f1a)" };
const td: React.CSSProperties = { padding: "3px 8px", fontSize: 10.5, color: "#ccc", borderBottom: "1px solid #1a1a2e", whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis", maxWidth: 200 };
const table: React.CSSProperties = { width: "100%", borderCollapse: "collapse", margin: 0 };

interface Props {
  tabId: string;
  data: unknown;
}

export function TypedDataRenderer({ tabId, data }: Props) {
  if (!data || typeof data !== "object") {
    return <pre style={{ margin: 0, fontSize: 11, color: "#888" }}>{JSON.stringify(data, null, 2)}</pre>;
  }

  switch (tabId) {
    case "wifi": return <WifiTable data={data as WifiNetwork[]} />;
    case "bluetooth": return <BluetoothTable data={data as BluetoothDevice[]} />;
    case "browser": return <BrowserTable data={data as BrowserForensicsResult} />;
    case "gps": return <LocationTable data={data as LocationForensicsResult} />;
    case "power": return <PowerTable data={data as PowerEvent[]} />;
    case "notifications": return <NotificationTable data={data as NotificationEntry[]} />;
    case "keyboard": return <DictionaryTable data={data as DictionaryWord[]} />;
    case "sms": return <DeletedSmsTable data={data as DeletedSmsResult} />;
    case "photos": return <ThumbnailsTable data={data as ThumbnailRecoveryResult} />;
    case "nettraffic": return <NetworkTable data={data as NetworkForensicsResult} />;
    default:
      return <pre style={{ margin: 0, fontSize: 11, color: "#888" }}>{JSON.stringify(data, null, 2)}</pre>;
  }
}

function WifiTable({ data }: { data: WifiNetwork[] }) {
  if (!data?.length) return <Empty msg="No WiFi networks found" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>SSID</th><th style={th}>BSSID</th><th style={th}>Security</th><th style={th}>Freq</th><th style={th}>Saved</th><th style={th}>Last Connected</th></tr></thead>
        <tbody>{data.map((w, i) => (
          <tr key={i}><td style={td}>{w.ssid || "—"}</td><td style={{ ...td, fontFamily: "monospace", fontSize: 9.5 }}>{w.bssid}</td><td style={td}>{w.security}</td><td style={td}>{w.freq_mhz ? `${w.freq_mhz} MHz` : "—"}</td><td style={td}>{w.is_saved ? "✓" : "✗"}</td><td style={td}>{w.last_connected ? fmtTime(w.last_connected) : "—"}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function BluetoothTable({ data }: { data: BluetoothDevice[] }) {
  if (!data?.length) return <Empty msg="No Bluetooth devices found" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>Name</th><th style={th}>MAC</th><th style={th}>Type</th><th style={th}>Bond</th><th style={th}>Alias</th></tr></thead>
        <tbody>{data.map((b, i) => (
          <tr key={i}><td style={td}>{b.name || "—"}</td><td style={{ ...td, fontFamily: "monospace", fontSize: 9.5 }}>{b.mac_address}</td><td style={td}>{b.device_type}</td><td style={td}>{b.bond_state}</td><td style={td}>{b.alias || "—"}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function BrowserTable({ data }: { data: BrowserForensicsResult }) {
  if (!data) return <Empty msg="No browser data" />;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      {data.notes?.length > 0 && <div style={{ padding: 8, background: "rgba(255,200,0,0.08)", borderRadius: 6, fontSize: 10, color: "#f0c040" }}>{data.notes.map((n, i) => <div key={i}>⚠ {n}</div>)}</div>}
      <Section title={`🌐 History (${data.history?.length || 0})`}>
        {data.history?.length ? (
          <div style={{ maxHeight: 200, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>URL</th><th style={th}>Title</th><th style={th}>Visits</th><th style={th}>Last Visit</th></tr></thead>
              <tbody>{data.history.slice(0, 100).map((h, i) => (
                <tr key={i}><td style={{ ...td, maxWidth: 300 }}>{h.url}</td><td style={td}>{h.title}</td><td style={{ ...td, textAlign: "center" }}>{h.visit_count}</td><td style={td}>{fmtTime(h.last_visit)}</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No history" />}
      </Section>
      <Section title={`🔖 Bookmarks (${data.bookmarks?.length || 0})`}>
        {data.bookmarks?.length ? (
          <div style={{ maxHeight: 160, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>Title</th><th style={th}>URL</th><th style={th}>Created</th></tr></thead>
              <tbody>{data.bookmarks.map((b, i) => (
                <tr key={i}><td style={td}>{b.title}</td><td style={{ ...td, maxWidth: 300 }}>{b.url}</td><td style={td}>{fmtTime(b.created)}</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No bookmarks" />}
      </Section>
      <Section title={`📥 Downloads (${data.downloads?.length || 0})`}>
        {data.downloads?.length ? (
          <div style={{ maxHeight: 160, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>Filename</th><th style={th}>Size</th><th style={th}>Progress</th></tr></thead>
              <tbody>{data.downloads.map((d, i) => (
                <tr key={i}><td style={td}>{d.filename || d.url}</td><td style={td}>{fmtSize(d.total_bytes)}</td><td style={td}>{d.total_bytes > 0 ? `${Math.round(d.received_bytes / d.total_bytes * 100)}%` : "—"}</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No downloads" />}
      </Section>
      <Section title={`🍪 Cookies (${data.cookies?.length || 0})`}>
        {data.cookies?.length ? (
          <div style={{ maxHeight: 160, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>Host</th><th style={th}>Name</th><th style={th}>Expires</th></tr></thead>
              <tbody>{data.cookies.slice(0, 50).map((c, i) => (
                <tr key={i}><td style={td}>{c.host_key}</td><td style={td}>{c.name}</td><td style={td}>{c.expires_utc && c.expires_utc > 0 ? fmtTime(c.expires_utc) : "Session"}</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No cookies" />}
      </Section>
    </div>
  );
}

function LocationTable({ data }: { data: LocationForensicsResult }) {
  if (!data?.points?.length) return <Empty msg="No location data" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>Latitude</th><th style={th}>Longitude</th><th style={th}>Accuracy</th><th style={th}>Source</th><th style={th}>Timestamp</th></tr></thead>
        <tbody>{data.points.map((p, i) => (
          <tr key={i}><td style={{ ...td, fontFamily: "monospace" }}>{p.latitude}</td><td style={{ ...td, fontFamily: "monospace" }}>{p.longitude}</td><td style={td}>{p.accuracy_m != null ? `${p.accuracy_m}m` : "—"}</td><td style={td}>{p.source}</td><td style={td}>{p.timestamp ? fmtTime(p.timestamp) : "—"}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function PowerTable({ data }: { data: PowerEvent[] }) {
  if (!data?.length) return <Empty msg="No power events" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>Event</th><th style={th}>Battery</th><th style={th}>Detail</th><th style={th}>Time</th></tr></thead>
        <tbody>{data.map((p, i) => (
          <tr key={i}><td style={td}>{p.event_type}</td><td style={td}>{p.battery_level}%</td><td style={{ ...td, maxWidth: 250 }}>{p.detail || "—"}</td><td style={td}>{fmtTime(p.timestamp_ms)}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function NotificationTable({ data }: { data: NotificationEntry[] }) {
  if (!data?.length) return <Empty msg="No notifications" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>Package</th><th style={th}>Title</th><th style={th}>Text</th><th style={th}>Channel</th><th style={th}>Time</th></tr></thead>
        <tbody>{data.map((n, i) => (
          <tr key={i}><td style={{ ...td, fontFamily: "monospace", fontSize: 9 }}>{n.package}</td><td style={td}>{n.title}</td><td style={{ ...td, maxWidth: 200 }}>{n.text}</td><td style={td}>{n.channel || "—"}</td><td style={td}>{n.timestamp ? fmtTime(n.timestamp) : "—"}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function DictionaryTable({ data }: { data: DictionaryWord[] }) {
  if (!data?.length) return <Empty msg="No dictionary words" />;
  return (
    <div style={{ maxHeight: 360, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
      <table style={table}>
        <thead><tr><th style={th}>Word</th><th style={th}>Frequency</th><th style={th}>App</th></tr></thead>
        <tbody>{data.map((w, i) => (
          <tr key={i}><td style={td}>{w.word}</td><td style={{ ...td, textAlign: "center" }}>{w.frequency}</td><td style={td}>{w.app_id || w.locale || "—"}</td></tr>
        ))}</tbody>
      </table>
    </div>
  );
}

function DeletedSmsTable({ data }: { data: DeletedSmsResult }) {
  if (!data?.records?.length) return <Empty msg={`No deleted SMS found. ${data?.notes?.join(" ") || ""}`} />;
  return (
    <div>
      <div style={{ marginBottom: 8, display: "flex", gap: 8 }}>
        <span className="pill ok">{data.deleted_count} deleted</span>
        <span className="pill">{data.live_count} live</span>
      </div>
      <div style={{ maxHeight: 300, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
        <table style={table}>
          <thead><tr><th style={th}>Address</th><th style={th}>Body</th><th style={th}>Date</th><th style={th}>Source</th></tr></thead>
          <tbody>{data.records.map((m: SmsRecord, i: number) => (
            <tr key={i}><td style={td}>{m.address || "—"}</td><td style={{ ...td, maxWidth: 300, whiteSpace: "normal" }}>{m.body}</td><td style={td}>{m.timestamp ? fmtTime(m.timestamp) : "—"}</td><td style={td}>{m.source}</td></tr>
          ))}</tbody>
        </table>
      </div>
    </div>
  );
}

function ThumbnailsTable({ data }: { data: ThumbnailRecoveryResult }) {
  if (!data?.thumbnails?.length) return <Empty msg={`No thumbnails found. ${data?.notes?.join(" ") || ""}`} />;
  return (
    <div>
      <div style={{ marginBottom: 8, display: "flex", gap: 8 }}>
        <span className="pill ok">{data.total_count} thumbnails</span>
      </div>
      <div style={{ maxHeight: 300, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
        <table style={table}>
          <thead><tr><th style={th}>File</th><th style={th}>Size</th><th style={th}>Dimensions</th></tr></thead>
          <tbody>{data.thumbnails.map((t, i) => (
            <tr key={i}><td style={{ ...td, maxWidth: 300 }}>{t.path}</td><td style={td}>{fmtSize(t.file_size)}</td><td style={td}>{t.width != null && t.height != null ? `${t.width}x${t.height}` : "—"}</td></tr>
          ))}</tbody>
        </table>
      </div>
    </div>
  );
}

function NetworkTable({ data }: { data: NetworkForensicsResult }) {
  const tcpLen = data?.tcp_connections?.length || 0;
  const arpLen = data?.arp_entries?.length || 0;
  if (!tcpLen && !arpLen) return <Empty msg="No network artifacts" />;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
      <Section title={`🔌 TCP Connections (${tcpLen})`}>
        {tcpLen > 0 ? (
          <div style={{ maxHeight: 180, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>Local</th><th style={th}>Remote</th><th style={th}>State</th><th style={th}>UID/PID</th></tr></thead>
              <tbody>{data.tcp_connections.slice(0, 80).map((c, i) => (
                <tr key={i}><td style={{ ...td, fontFamily: "monospace", fontSize: 9 }}>{c.local_addr}:{c.local_port}</td><td style={{ ...td, fontFamily: "monospace", fontSize: 9 }}>{c.remote_addr}:{c.remote_port}</td><td style={td}>{c.state}</td><td style={td}>—</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No TCP connections" />}
      </Section>
      <Section title={`📋 ARP Entries (${arpLen})`}>
        {arpLen > 0 ? (
          <div style={{ maxHeight: 160, overflow: "auto", border: "1px solid #222", borderRadius: 6 }}>
            <table style={table}>
              <thead><tr><th style={th}>IP</th><th style={th}>MAC</th><th style={th}>Device</th><th style={th}>State</th></tr></thead>
              <tbody>{data.arp_entries.map((a, i) => (
                <tr key={i}><td style={td}>{a.ip}</td><td style={{ ...td, fontFamily: "monospace", fontSize: 9 }}>{a.mac}</td><td style={td}>{a.device}</td><td style={td}>{a.state}</td></tr>
              ))}</tbody>
            </table>
          </div>
        ) : <Empty msg="No ARP entries" />}
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <div style={{ fontSize: 11, fontWeight: 600, color: "#aaa", marginBottom: 4 }}>{title}</div>
      {children}
    </div>
  );
}

function Empty({ msg }: { msg: string }) {
  return <div style={{ padding: 20, textAlign: "center", color: "#555", fontSize: 11 }}>{msg}</div>;
}
