import { convertFileSrc } from "@tauri-apps/api/core";
import type { CallEvent, CallRecording } from "../../types";
import { fmtSize } from "../../types";

interface ConnectCallsTabProps {
  serial: string;
  callMonitorActive: boolean;
  callEvents: CallEvent[];
  currentCallNotification: string | null;
  callRecordings: CallRecording[];
  playingRec: { path: string; localPath: string; filename: string } | null;
  timeline: { kind: "event" | "recording"; ts: number; event?: CallEvent; rec?: CallRecording }[];
  onStartMonitor: () => void;
  onStopMonitor: () => void;
  onLoadRecordings: () => void;
  onPlayRecording: (rec: CallRecording) => void;
  onStopPlaying: () => void;
}

export function ConnectCallsTab({
  serial, callMonitorActive, callEvents, currentCallNotification, callRecordings,
  playingRec, timeline, onStartMonitor, onStopMonitor, onLoadRecordings,
  onPlayRecording, onStopPlaying,
}: ConnectCallsTabProps) {
  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">
        📞 Call Monitor
        {callMonitorActive && <span className="pill ok" style={{ marginLeft: 8 }}>● Auto-Monitoring</span>}
      </div>
      <div className="panel-body">
        <div style={{ display: "flex", gap: 8, marginBottom: 8, alignItems: "center" }}>
          {callMonitorActive ? (
            <>
              <span style={{ color: "#2f9e44", fontSize: 12, fontWeight: 500 }}>✓ Monitoring active</span>
              <button className="mini" style={{ marginLeft: "auto" }} onClick={onStopMonitor} title="Pause monitoring">
                ⏸ Pause
              </button>
            </>
          ) : (
            <button className="primary" disabled={!serial} onClick={onStartMonitor} style={{ flex: 1 }}>
              ▶ Resume Monitoring
            </button>
          )}
        </div>
        <p className="hint small">
          {callMonitorActive
            ? "Automatically detects calls when they occur. Notifications appear for incoming/outgoing calls."
            : "Call monitoring paused. Click resume to start automatic detection."}
        </p>

        {currentCallNotification && (
          <div style={{
            marginTop: 12, padding: 12, background: "rgba(0,200,100,0.15)",
            borderRadius: 8, border: "2px solid rgba(0,200,100,0.5)", animation: "pulse 1s infinite"
          }}>
            <p style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>{currentCallNotification}</p>
          </div>
        )}

        <div style={{ marginTop: 12 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8, flexWrap: "wrap" }}>
            <span style={{ fontWeight: 500, fontSize: 12 }}>📊 Activity Timeline</span>
            <span className="pill">{callEvents.length} events</span>
            <span className="pill">{callRecordings.length} rec</span>
            <button className="mini" style={{ marginLeft: "auto" }} onClick={onLoadRecordings} disabled={!serial} title="Rescan device for call recordings">⟳ Recordings</button>
          </div>

          {playingRec && (
            <div style={{ marginBottom: 8, padding: 8, background: "var(--cream)", border: "1px solid var(--line)", borderRadius: 6 }}>
              <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 4, gap: 8 }}>
                <span style={{ fontSize: 11, fontWeight: 600, color: "var(--saffron-deep)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>🎵 {playingRec.filename}</span>
                <button className="mini" onClick={onStopPlaying}>✕</button>
              </div>
              <audio controls autoPlay src={convertFileSrc(playingRec.localPath)} style={{ width: "100%", height: 32 }} />
            </div>
          )}

          {timeline.length > 0 ? (
            <div style={{ maxHeight: 260, overflowY: "auto", border: "1px solid var(--line)", borderRadius: 6, padding: 8 }}>
              {timeline.slice(0, 60).map((item, i) => (
                item.kind === "event" && item.event ? (
                  <div key={`e-${i}`} style={{ display: "flex", alignItems: "center", gap: 6, padding: "4px 0", borderBottom: "1px solid var(--line)", fontSize: 12 }}>
                    <span>{item.event.event_type === "Incoming" ? "📞" : item.event.event_type === "Outgoing" ? "📱" : item.event.event_type === "Active" ? "🟢" : item.event.event_type === "Missed" ? "❌" : item.event.event_type === "Ended" ? "📴" : "•"}</span>
                    <strong>{item.event.event_type}</strong>
                    {(item.event.contact_name || item.event.phone_number) && (
                      <span style={{ color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{item.event.contact_name || item.event.phone_number}</span>
                    )}
                    <span style={{ marginLeft: "auto", fontSize: 10, color: "var(--muted)", whiteSpace: "nowrap" }}>{new Date(item.ts).toLocaleTimeString()}</span>
                  </div>
                ) : item.rec ? (
                  <div key={`r-${i}`} style={{ display: "flex", alignItems: "center", gap: 6, padding: "4px 0", borderBottom: "1px solid var(--line)", fontSize: 12 }}>
                    <span>🎙️</span>
                    <strong style={{ color: "var(--saffron-deep)" }}>Rec</strong>
                    <span style={{ color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", maxWidth: 130 }} title={item.rec.path}>
                      {item.rec.matched_number ? `${item.rec.matched_name ? item.rec.matched_name + " " : ""}${item.rec.matched_number}` : item.rec.filename}
                    </span>
                    <span style={{ fontSize: 10, color: "var(--muted)", whiteSpace: "nowrap" }}>{fmtSize(item.rec.size)}</span>
                    <button className="mini" style={{ marginLeft: "auto" }} onClick={() => onPlayRecording(item.rec!)} title="Pull & play">{playingRec?.path === item.rec.path ? "🔄" : "▶"}</button>
                    <span style={{ fontSize: 10, color: "var(--muted)", whiteSpace: "nowrap" }}>{new Date(item.ts).toLocaleString()}</span>
                  </div>
                ) : null
              ))}
            </div>
          ) : (
            <p className="hint small" style={{ textAlign: "center" }}>
              {callMonitorActive
                ? "Waiting for calls… make or receive a call on the device."
                : "Connect a device to start automatic call detection."}
            </p>
          )}
        </div>
      </div>
    </div>
  );
}
