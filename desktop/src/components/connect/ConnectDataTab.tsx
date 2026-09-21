import type { DirectAcquisition } from "../../types";
import { DesktopMicStream } from "../DesktopMicStream";

interface ConnectDataTabProps {
  serial: string;
  busy: boolean;
  directData: DirectAcquisition | null;
  directBusy: boolean;
  onAcquire: () => void;
  onViewFull: () => void;
  cameraBusy: boolean;
  cameraStatus: string;
  onCapturePhoto: () => void;
}

export function ConnectDataTab({
  serial, busy, directData, directBusy, onAcquire, onViewFull,
  cameraBusy, cameraStatus, onCapturePhoto,
}: ConnectDataTabProps) {
  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">📞 System Data</div>
      <div className="panel-body">
        {/* Quick Actions Grid */}
        <div className="quick-actions-grid">
          <button
            className="quick-action primary"
            disabled={!serial || directBusy || busy}
            onClick={onAcquire}
          >
            <span className="qa-icon">📥</span>
            <span className="qa-label">{directBusy ? "Acquiring…" : "Acquire Data"}</span>
            <span className="qa-hint">Call logs, SMS, contacts</span>
          </button>
          <button
            className="quick-action"
            disabled={!serial || busy || cameraBusy}
            onClick={onCapturePhoto}
            style={{ background: cameraBusy ? "rgba(239,68,68,0.15)" : "rgba(249,115,22,0.1)", borderColor: cameraBusy ? "rgba(239,68,68,0.3)" : "rgba(249,115,22,0.25)" }}
          >
            <span className="qa-icon">📸</span>
            <span className="qa-label">{cameraBusy ? "Capturing…" : "Camera"}</span>
            <span className="qa-hint">Take photo via device</span>
          </button>
          <button
            className="quick-action"
            disabled={!directData}
            onClick={onViewFull}
            style={{ background: "rgba(16,185,129,0.1)", borderColor: "rgba(16,185,129,0.25)" }}
          >
            <span className="qa-icon">🔍</span>
            <span className="qa-label">View All Data</span>
            <span className="qa-hint">{directData ? `${directData.call_logs.length + directData.sms_messages.length + directData.contacts.length} items` : "Acquire first"}</span>
          </button>
        </div>

        {/* Status feedback */}
        {cameraStatus && (
          <p className="hint small" style={{ marginTop: 6, textAlign: "center", color: cameraStatus.startsWith("📸") ? "#2ecc71" : cameraStatus.startsWith("❌") ? "#e74c3c" : "#888" }}>
            {cameraStatus}
          </p>
        )}

        {/* Acquired data summary */}
        {directData && (
          <>
            <hr className="pdiv" />
            <div className="stats">
              <span className="pill ok">{directData.call_logs.length} calls</span>
              <span className="pill ok">{directData.sms_messages.length} SMS</span>
              <span className="pill ok">{directData.contacts.length} contacts</span>
              <span className="pill ok">{directData.call_recordings.length} rec</span>
              <span className="pill ok">{directData.whatsapp_media.length} WA</span>
            </div>
            {directData.notes.length > 0 && (
              <div className="hint" style={{ marginTop: 4 }}>
                {directData.notes.map((n, i) => (
                  <p key={i} className="err">⚠ {n}</p>
                ))}
              </div>
            )}
          </>
        )}

        {/* Desktop Mic — compact */}
        <hr className="pdiv" />
        <DesktopMicStream onStatus={() => {}} />
      </div>
    </div>
  );
}
