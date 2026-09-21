import { useState, useEffect, useCallback } from "react";
import type { RemoteAgentState, ReceivedPackage } from "../../types";
import { remoteAgentStart, remoteAgentStop, remoteAgentStatus, remoteAgentExport } from "../../api";
import { fmtTime } from "../../types";

interface ConnectRemoteTabProps {
  serial: string;
}

export function ConnectRemoteTab({ serial: _serial }: ConnectRemoteTabProps) {
  const [serverState, setServerState] = useState<RemoteAgentState | null>(null);
  const [port, setPort] = useState(7799);
  const [busy, setBusy] = useState(false);
  const [exportingId, setExportingId] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const isRunning = serverState?.status === "running";

  const refreshStatus = useCallback(async () => {
    try {
      const state = await remoteAgentStatus();
      setServerState(state);
    } catch {
      // Server not started yet
    }
  }, []);

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 3000);
    return () => clearInterval(interval);
  }, [refreshStatus]);

  const handleStart = async () => {
    setBusy(true);
    setError(null);
    try {
      const state = await remoteAgentStart(port);
      setServerState(state);
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  const handleStop = async () => {
    setBusy(true);
    try {
      const state = await remoteAgentStop();
      setServerState(state);
    } catch (e) {
      setError(String(e));
    }
    setBusy(false);
  };

  const handleExport = async (pkg: ReceivedPackage) => {
    setExportingId(pkg.id);
    setExportResult(null);
    try {
      const dir = await remoteAgentExport(pkg.local_path);
      setExportResult(`Exported to: ${dir}`);
    } catch (e) {
      setExportResult(`Error: ${e}`);
    }
    setExportingId(null);
  };

  return (
    <div style={{ gridArea: "main", overflow: "auto", padding: "8px" }}>
      {/* Server Controls */}
      <div style={{ marginBottom: 12 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginBottom: 8 }}>
          <span style={{ fontWeight: 600, fontSize: 12 }}>Remote Agent Receiver</span>
          <span style={{
            fontSize: 9,
            padding: "2px 6px",
            borderRadius: 8,
            background: isRunning ? "var(--ok)" : "var(--text-muted)",
            color: "#fff",
            fontWeight: 600,
          }}>
            {isRunning ? "LISTENING" : "STOPPED"}
          </span>
        </div>

        <p style={{ fontSize: 10, color: "var(--text-secondary)", margin: "0 0 8px" }}>
          Start the receiver to accept forensic data packages from the RecoverKit Remote Agent app
          running on a remote Android device (any distance — WiFi, 4G, or internet).
        </p>

        <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8 }}>
          <label style={{ fontSize: 10 }}>Port:</label>
          <input
            type="number"
            value={port}
            onChange={(e) => setPort(Number(e.target.value))}
            disabled={isRunning}
            style={{ width: 60, fontSize: 10, padding: "2px 4px" }}
          />
          {!isRunning ? (
            <button className="primary" onClick={handleStart} disabled={busy} style={{ fontSize: 10 }}>
              {busy ? "Starting..." : "▶ Start Server"}
            </button>
          ) : (
            <button onClick={handleStop} disabled={busy} style={{ fontSize: 10 }}>
              {busy ? "Stopping..." : "■ Stop Server"}
            </button>
          )}
        </div>

        {isRunning && (
          <div style={{ fontSize: 10, color: "var(--text-secondary)", fontFamily: "monospace", background: "var(--bg-input)", padding: "4px 8px", borderRadius: 4 }}>
            Listening on <strong>0.0.0.0:{port}</strong> — agent uploads to <strong>POST http://YOUR_IP:{port}/upload</strong>
          </div>
        )}

        {error && (
          <div style={{ fontSize: 10, color: "var(--err)", marginTop: 4 }}>{error}</div>
        )}
      </div>

      {/* Agent App Info */}
      <div style={{ marginBottom: 12, background: "var(--bg-input)", borderRadius: 6, padding: "8px 10px" }}>
        <div style={{ fontWeight: 600, fontSize: 11, marginBottom: 4 }}>📱 Remote Agent App</div>
        <p style={{ fontSize: 10, color: "var(--text-secondary)", margin: 0 }}>
          Install the RecoverKit Remote Agent APK on the target device. The app collects
          contacts, call logs, SMS, browser bookmarks, WiFi networks, Bluetooth devices,
          installed apps, and a media manifest — then uploads an encrypted JSON package
          to this server. No root required. No USB needed.
        </p>
        <div style={{ marginTop: 6, fontSize: 10, color: "var(--text-secondary)" }}>
          <strong>Endpoints:</strong>
          <ul style={{ margin: "2px 0 0 16px", padding: 0 }}>
            <li><code>POST /upload</code> — receive forensic package</li>
            <li><code>GET /status</code> — server status check</li>
            <li><code>GET /health</code> — health check</li>
          </ul>
        </div>
      </div>

      {/* Received Packages */}
      <div>
        <div style={{ fontWeight: 600, fontSize: 11, marginBottom: 6 }}>
          📦 Received Packages ({serverState?.packages.length || 0})
        </div>

        {!serverState?.packages.length ? (
          <div style={{ fontSize: 10, color: "var(--text-muted)", textAlign: "center", padding: 20 }}>
            No packages received yet. Start the server and upload from the agent app.
          </div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
            {serverState?.packages.map((pkg) => (
              <PackageCard
                key={pkg.id}
                pkg={pkg}
                onExport={() => handleExport(pkg)}
                exporting={exportingId === pkg.id}
                exportResult={exportingId === pkg.id ? exportResult : null}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function PackageCard({
  pkg,
  onExport,
  exporting,
  exportResult,
}: {
  pkg: ReceivedPackage;
  onExport: () => void;
  exporting: boolean;
  exportResult: string | null;
}) {
  const totalRecords = pkg.contacts_count + pkg.calls_count + pkg.sms_count + pkg.bookmarks_count;

  return (
    <div style={{
      background: "var(--bg-card)",
      border: "1px solid var(--border)",
      borderRadius: 6,
      padding: "8px 10px",
    }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4 }}>
        <div>
          <span style={{ fontWeight: 600, fontSize: 11 }}>{pkg.device_model || pkg.device_id}</span>
          {pkg.android_version && (
            <span style={{ fontSize: 9, color: "var(--text-muted)", marginLeft: 6 }}>
              Android {pkg.android_version}
            </span>
          )}
        </div>
        <span style={{ fontSize: 9, color: "var(--text-muted)" }}>
          {fmtTime(pkg.received_at)}
        </span>
      </div>

      <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 6 }}>
        <Badge label="Contacts" count={pkg.contacts_count} />
        <Badge label="Calls" count={pkg.calls_count} />
        <Badge label="SMS" count={pkg.sms_count} />
        <Badge label="Bookmarks" count={pkg.bookmarks_count} />
        <Badge label="WiFi" count={pkg.wifi_count} />
        <Badge label="BT" count={pkg.bluetooth_count} />
        <Badge label="Apps" count={pkg.apps_count} />
        <Badge label="Media" count={pkg.media_count} />
      </div>

      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ fontSize: 9, color: "var(--text-muted)" }}>
          {totalRecords} total records · ID: {pkg.id.slice(0, 20)}...
        </span>
        <button
          className="primary"
          onClick={onExport}
          disabled={exporting}
          style={{ fontSize: 9, padding: "2px 8px" }}
        >
          {exporting ? "Exporting..." : "📂 Export to Workspace"}
        </button>
      </div>

      {exportResult && (
        <div style={{ fontSize: 9, marginTop: 4, color: exportResult.startsWith("Error") ? "var(--err)" : "var(--ok)" }}>
          {exportResult}
        </div>
      )}

      {pkg.notes.length > 0 && (
        <div style={{ fontSize: 9, color: "var(--text-secondary)", marginTop: 4 }}>
          {pkg.notes.join(" · ")}
        </div>
      )}
    </div>
  );
}

function Badge({ label, count }: { label: string; count: number }) {
  if (count === 0) return null;
  return (
    <span style={{
      fontSize: 9,
      padding: "1px 5px",
      borderRadius: 8,
      background: "var(--accent)",
      color: "#fff",
      fontWeight: 500,
    }}>
      {label}: {count}
    </span>
  );
}
