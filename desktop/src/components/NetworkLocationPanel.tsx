import { useState } from "react";
import type { LocationForensicsResult, NetworkForensicsResult, ApkAnalysisResult } from "../types";
import { fmtTime } from "../types";
import { collectLocationData, collectNetworkArtifacts, analyzeApks } from "../api";

interface NetworkLocationPanelProps {
  serial: string;
}

type SubTab = "location" | "network" | "apk";

export function NetworkLocationPanel({ serial }: NetworkLocationPanelProps) {
  const [tab, setTab] = useState<SubTab>("location");
  const [location, setLocation] = useState<LocationForensicsResult | null>(null);
  const [network, setNetwork] = useState<NetworkForensicsResult | null>(null);
  const [apks, setApks] = useState<ApkAnalysisResult | null>(null);
  const [busy, setBusy] = useState(false);

  const loadLocation = async () => {
    setBusy(true);
    try { setLocation(await collectLocationData(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const loadNetwork = async () => {
    setBusy(true);
    try { setNetwork(await collectNetworkArtifacts(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const loadApks = async () => {
    setBusy(true);
    try { setApks(await analyzeApks(serial)); } catch { /* ignore */ }
    setBusy(false);
  };

  const tabs: { key: SubTab; label: string; icon: string; count?: number }[] = [
    { key: "location", label: "Location", icon: "📍", count: location?.points.length },
    { key: "network", label: "Network", icon: "🌐", count: (network?.tcp_connections.length || 0) + (network?.arp_entries.length || 0) },
    { key: "apk", label: "APK Analysis", icon: "📦", count: apks?.apps.length },
  ];

  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">📍 Location & Network</div>
      <div className="fx-tabs">
        {tabs.map((t) => (
          <button key={t.key} className={`fx-tab${tab === t.key ? " active" : ""}`} onClick={() => setTab(t.key)}>
            {t.icon} {t.label}
            {t.count != null && t.count > 0 && <span className="fx-tab-count">{t.count}</span>}
          </button>
        ))}
      </div>
      <div className="panel-body" style={{ overflow: "auto" }}>
        {tab === "location" && <LocationSection data={location} busy={busy} serial={serial} onLoad={loadLocation} />}
        {tab === "network" && <NetworkSection data={network} busy={busy} serial={serial} onLoad={loadNetwork} />}
        {tab === "apk" && <ApkSection data={apks} busy={busy} serial={serial} onLoad={loadApks} />}
      </div>
    </div>
  );
}

function LocationSection({ data, busy, serial, onLoad }: { data: LocationForensicsResult | null; busy: boolean; serial: string; onLoad: () => void }) {
  if (!data) {
    return (
      <div className="fx-empty">
        <span className="fx-empty-icon">📍</span>
        <span className="fx-empty-text">Collect GPS, cell tower, and WiFi location data from the device.</span>
        <button className="primary" disabled={!serial || busy} onClick={onLoad}>
          {busy ? "Collecting…" : "📍 Collect Location Data"}
        </button>
      </div>
    );
  }
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.points.length} points</span>
        <span className="fx-stat info">{data.points.filter(p => p.source === "gps").length} GPS</span>
        <span className="fx-stat neutral">{data.points.filter(p => p.source === "cell").length} cell</span>
        <button className="mini" onClick={onLoad} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.notes.length > 0 && (
        <div className="fx-notes">{data.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
      )}
      <div className="fx-scroll fx-list">
        {data.points.map((p, i) => (
          <div key={i} className="fx-row">
            <span className="fx-row-icon">📍</span>
            <span className={`fx-row-label ${p.source === "gps" ? "fx-cat-location" : p.source === "cell" ? "fx-cat-call" : "fx-cat-wifi"}`}>{p.source}</span>
            <div className="fx-row-content">
              <span className="fx-row-title mono" style={{ fontSize: 10.5 }}>{p.latitude.toFixed(6)}, {p.longitude.toFixed(6)}</span>
              {p.accuracy_m && <span className="fx-row-detail"> ±{p.accuracy_m.toFixed(0)}m</span>}
            </div>
            {p.timestamp && <span className="fx-row-time">{fmtTime(p.timestamp)}</span>}
          </div>
        ))}
      </div>
    </>
  );
}

function NetworkSection({ data, busy, serial, onLoad }: { data: NetworkForensicsResult | null; busy: boolean; serial: string; onLoad: () => void }) {
  if (!data) {
    return (
      <div className="fx-empty">
        <span className="fx-empty-icon">🌐</span>
        <span className="fx-empty-text">Snapshot TCP connections, ARP table, and DNS cache from the device.</span>
        <button className="primary" disabled={!serial || busy} onClick={onLoad}>
          {busy ? "Collecting…" : "🌐 Collect Network Artifacts"}
        </button>
      </div>
    );
  }
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.tcp_connections.length} TCP</span>
        <span className="fx-stat info">{data.arp_entries.length} ARP</span>
        <span className="fx-stat neutral">{data.dns_cache.length} DNS</span>
        <button className="mini" onClick={onLoad} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.notes.length > 0 && (
        <div className="fx-notes">{data.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
      )}

      <div className="fx-section-head">TCP Connections</div>
      <div className="fx-scroll">
        <table className="fx-table">
          <thead><tr><th>Local</th><th>Remote</th><th>State</th></tr></thead>
          <tbody>
            {data.tcp_connections.slice(0, 100).map((c, i) => (
              <tr key={i}>
                <td className="mono">{c.local_addr}:{c.local_port}</td>
                <td className="mono">{c.remote_addr}:{c.remote_port}</td>
                <td><span className={`fx-row-label ${c.state === "ESTABLISHED" ? "fx-cat-call" : "fx-cat-system"}`}>{c.state}</span></td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {data.arp_entries.length > 0 && (
        <>
          <div className="fx-section-head" style={{ marginTop: 8 }}>ARP Table</div>
          <table className="fx-table">
            <thead><tr><th>IP</th><th>MAC</th><th>Device</th></tr></thead>
            <tbody>
              {data.arp_entries.map((a, i) => (
                <tr key={i}>
                  <td className="mono">{a.ip}</td>
                  <td className="mono">{a.mac}</td>
                  <td>{a.device}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </>
  );
}

function ApkSection({ data, busy, serial, onLoad }: { data: ApkAnalysisResult | null; busy: boolean; serial: string; onLoad: () => void }) {
  if (!data) {
    return (
      <div className="fx-empty">
        <span className="fx-empty-icon">📦</span>
        <span className="fx-empty-text">Hash installed APKs (SHA-256) and check for forensic flags.</span>
        <button className="primary" disabled={!serial || busy} onClick={onLoad}>
          {busy ? "Analyzing…" : "📦 Analyze Installed APKs"}
        </button>
      </div>
    );
  }
  return (
    <>
      <div className="fx-stats">
        <span className="fx-stat">{data.apps.length} apps analyzed</span>
        <button className="mini" onClick={onLoad} disabled={busy} style={{ marginLeft: "auto" }}>⟳ Refresh</button>
      </div>
      {data.notes.length > 0 && (
        <div className="fx-notes">{data.notes.map((n, i) => <p key={i}>• {n}</p>)}</div>
      )}
      <div className="fx-scroll">
        <table className="fx-table">
          <thead><tr><th>Package</th><th>SHA-256</th><th>Flags</th><th>Installed</th></tr></thead>
          <tbody>
            {data.apps.slice(0, 100).map((a, i) => (
              <tr key={i}>
                <td style={{ fontWeight: 600 }} title={a.package}>{a.package}</td>
                <td className="mono" style={{ fontSize: 9 }}>{a.sha256.slice(0, 16)}…</td>
                <td>
                  {a.forensic_flags.map((f, j) => (
                    <span key={j} className={`fx-row-label ${f === "unusual_path" ? "fx-cat-deleted_sms" : "fx-cat-system"}`} style={{ marginRight: 2 }}>{f}</span>
                  ))}
                </td>
                <td style={{ fontSize: 9.5 }}>{a.install_time ? fmtTime(a.install_time) : "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
