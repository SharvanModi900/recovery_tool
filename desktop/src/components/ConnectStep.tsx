import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type {
  Device, DeviceMirror, DirectAcquisition, WhatsAppDeepResult, RootCheckResult,
  CallEvent, CallRecording, SimSnapshot, SimSwapVerdict, HistoricalSim, ScreenCapture,
} from "../types";
import { fmtSize } from "../types";
import { ScreenMirrorPanel } from "./ScreenMirrorPanel";
import { DesktopMicStream } from "./DesktopMicStream";
import { OverviewTab } from "./OverviewTab";
import { playDeviceRingtone as _playRingtone, startRingtoneLoop, stopRingtone, lockdownDevice, releaseDevice } from "../api";

interface ConnectStepProps {
  serial: string;
  busy: boolean;
  devices: Device[];
  connectTab: "overview" | "data" | "whatsapp" | "calls" | "sim";
  setConnectTab: (t: "overview" | "data" | "whatsapp" | "calls" | "sim") => void;
  // System data
  directData: DirectAcquisition | null;
  directBusy: boolean;
  onAcquire: () => void;
  onViewFull: () => void;
  // Camera
  cameraBusy: boolean;
  cameraStatus: string;
  onCapturePhoto: () => void;
  // Microphone
  micBusy: boolean;
  micStatus: string;
  micDuration: number;
  onMicDurationChange: (v: number) => void;
  onCaptureMic: () => void;
  // WhatsApp
  waDeepResult: WhatsAppDeepResult | null;
  waDeepBusy: boolean;
  rootCheck: RootCheckResult | null;
  rootBusy: boolean;
  onWhatsappDeep: () => void;
  onCheckRoot: () => void;
  onWhatsappDowngrade: () => void;
  onImportTxt: () => void;
  onProvideKey: () => void;
  // Call monitor
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
  // SIM
  simSnapshot: SimSnapshot | null;
  simBaseline: SimSnapshot | null;
  simHistorical: HistoricalSim[];
  simVerdict: SimSwapVerdict | null;
  simHistory: SimSnapshot[];
  simBusy: boolean;
  onCollectSim: () => void;
  onSimBaseline: (id: number) => void;
  onSimSwapCheck: () => void;
  onDeleteSimSnap: (id: number) => void;
  // Screen mirror
  mirror: DeviceMirror | null;
  shot: string;
  shotTs: number;
  shotDark: boolean;
  mirrorBusy: boolean;
  mirrorErr: string;
  isVideoFrame: boolean;
  liveStream: boolean;
  streamFps: number;
  streamMode: string;
  autoRefresh: boolean;
  captures: ScreenCapture[];
  captureBusy: boolean;
  autoCaptureActive: boolean;
  captureInterval: number;
  onCapture: (s: string) => void;
  onStartStream: (s: string) => void;
  onStopStream: (s: string) => void;
  onTakeScreenshot: () => void;
  onStartAuto: () => void;
  onStopAuto: () => void;
  onIntervalChange: (n: number) => void;
  onRefreshCaptures: () => void;
  onDeleteCapture: (id: number) => void;
  onAutoRefreshChange: (v: boolean) => void;
  onStreamFpsChange: (v: number) => void;
  onStreamModeChange: (v: string) => void;
  controlMode: boolean;
  onControlModeChange: (v: boolean) => void;
  headlessActive: boolean;
  headlessBusy: boolean;
  onToggleHeadless: (v: boolean) => void;
  paused: boolean;
  onPausedChange: (v: boolean) => void;
  onRingDevice: () => void;
  onLockDevice: () => void;
  onToggleCharging: () => void;
  chargingStopped: boolean;
  // Navigation
  onGoTo: (step: "scan") => void;
}

export function ConnectStep(props: ConnectStepProps) {
  const {
    serial, busy, devices, connectTab, setConnectTab,
    mirror, directData, directBusy, onAcquire, onViewFull,
    cameraBusy, cameraStatus, onCapturePhoto,
    waDeepResult, waDeepBusy, rootCheck, rootBusy, onWhatsappDeep, onCheckRoot, onWhatsappDowngrade, onImportTxt, onProvideKey,
    callMonitorActive, callEvents, currentCallNotification, callRecordings, playingRec, timeline,
    onStartMonitor, onStopMonitor, onLoadRecordings, onPlayRecording, onStopPlaying,
    simSnapshot, simBaseline, simHistorical, simVerdict, simHistory, simBusy,
    onCollectSim, onSimBaseline, onSimSwapCheck, onDeleteSimSnap,
    shot, shotTs, shotDark, mirrorBusy, mirrorErr, isVideoFrame, liveStream, streamFps, streamMode,
    autoRefresh, captures, captureBusy, autoCaptureActive, captureInterval,
    onCapture, onStartStream, onStopStream, onTakeScreenshot, onStartAuto, onStopAuto,
    onIntervalChange, onRefreshCaptures, onDeleteCapture, onAutoRefreshChange, onStreamFpsChange, onStreamModeChange,
    controlMode, onControlModeChange,
    headlessActive, headlessBusy, onToggleHeadless,
    paused, onPausedChange,
    onRingDevice, onLockDevice, onToggleCharging, chargingStopped,
    onGoTo,
  } = props;

  const [locked, setLocked] = useState(false);
  const [ringing, setRinging] = useState(false);

  const handleLockdown = async () => {
    if (!serial) return;
    try {
      if (locked) {
        await releaseDevice(serial);
        setLocked(false);
      } else {
        await lockdownDevice(serial);
        setLocked(true);
      }
    } catch {}
  };

  return (
    <section className="workbench wb-connect">
      {/* TOP — status strip */}
      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {devices.length === 0
              ? "No device detected — enable USB debugging, plug in via USB-C, tap Allow, then hit ⟳ in the header."
              : `${devices.length} device${devices.length === 1 ? "" : "s"} · ${serial || "none selected"} · read-only`}
          </span>
          {devices.some((d) => d.serial === serial && d.state === "device") && (
            <>
              <button className="primary next" onClick={() => onGoTo("scan")}>
                Continue to Scan →
              </button>
              <div className="quick-actions-inline">
                <button className="qa-btn" onClick={onViewFull} title="View Full Details">
                   Full Details
                </button>
                <button className="qa-btn primary" onClick={onAcquire} disabled={directBusy} title="Acquire Data">
                   Acquire Data
                </button>
                <button className="qa-btn" onClick={onLockDevice} title="Lock Device">
                   Lock
                </button>
                <button className="qa-btn" onClick={onRingDevice} title="Ring Device">
                   Ring
                </button>
                <button className={`qa-btn ${chargingStopped ? "warning" : ""}`} onClick={onToggleCharging} title={chargingStopped ? "Resume Charging" : "Stop Charging"}>
                  {chargingStopped ? "" : ""} {chargingStopped ? "Start Charge" : "Stop Charge"}
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* TAB BAR */}
      <div className="wb-connect-tabs">
        <button className={connectTab === "overview" ? "active" : ""} onClick={() => setConnectTab("overview")}>
          📋 Overview
        </button>
        <button className={connectTab === "data" ? "active" : ""} onClick={() => setConnectTab("data")}>
          📞 System Data
          {directData && <span className="tab-badge">{directData.call_logs.length + directData.sms_messages.length + directData.contacts.length}</span>}
        </button>
        <button className={connectTab === "whatsapp" ? "active" : ""} onClick={() => setConnectTab("whatsapp")}>
          🔐 WhatsApp
          {waDeepResult && <span className="tab-badge">{waDeepResult.total_messages || "✓"}</span>}
        </button>
        <button className={connectTab === "calls" ? "active" : ""} onClick={() => setConnectTab("calls")}>
          📞 Calls
          {callMonitorActive && <span className="tab-badge" style={{ background: "var(--ok)", color: "#fff" }}>●</span>}
        </button>
        <button className={connectTab === "sim" ? "active" : ""} onClick={() => setConnectTab("sim")}>
          📶 SIM
          {simSnapshot && <span className="tab-badge">✓</span>}
        </button>
      </div>

      {/* Overview Tab - Full Width */}
      {connectTab === "overview" && (
        <div style={{ gridArea: "main", gridColumn: "1 / -1" }}>
          <OverviewTab
            serial={serial}
            mirror={mirror}
            directData={directData}
            screenMirror={
              <ScreenMirrorPanel
                serial={serial}
                shot={shot} shotTs={shotTs} shotDark={shotDark}
                mirrorBusy={mirrorBusy} mirrorErr={mirrorErr} isVideoFrame={isVideoFrame}
                liveStream={liveStream} streamFps={streamFps} streamMode={streamMode}
                autoRefresh={autoRefresh}
                mirror={mirror}
                captures={captures} captureBusy={captureBusy}
                autoCaptureActive={autoCaptureActive} captureInterval={captureInterval}
                onCapture={onCapture}
                onStartStream={onStartStream}
                onStopStream={onStopStream}
                onTakeScreenshot={onTakeScreenshot}
                onStartAuto={onStartAuto}
                onStopAuto={onStopAuto}
                onIntervalChange={onIntervalChange}
                onRefreshCaptures={onRefreshCaptures}
                onDeleteCapture={onDeleteCapture}
                onAutoRefreshChange={onAutoRefreshChange}
                onStreamFpsChange={onStreamFpsChange}
                onStreamModeChange={onStreamModeChange}
                controlMode={controlMode}
                onControlModeChange={onControlModeChange}
                headlessActive={headlessActive}
                headlessBusy={headlessBusy}
                onToggleHeadless={onToggleHeadless}
                paused={paused}
                onPausedChange={onPausedChange}
              />
            }
          />
        </div>
      )}

      {/* System data acquisition */}
      {connectTab === "data" && (
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
                disabled={!serial || busy}
                onClick={async () => {
                  try {
                    if (ringing) {
                      await stopRingtone(serial);
                      setRinging(false);
                    } else {
                      await startRingtoneLoop(serial);
                      setRinging(true);
                    }
                  } catch {}
                }}
                style={{ background: ringing ? "rgba(239,68,68,0.15)" : "rgba(249,115,22,0.1)", borderColor: ringing ? "rgba(239,68,68,0.3)" : "rgba(249,115,22,0.25)" }}
              >
                <span className="qa-icon">{ringing ? "\u23F9" : "\uD83D\uDD14"}</span>
                <span className="qa-label">{ringing ? "Stop Ringing" : "Ring Phone"}</span>
                <span className="qa-hint">{ringing ? "Tap to stop" : "Loop until stopped"}</span>
              </button>
              <button
                className="quick-action"
                disabled={!serial || busy}
                onClick={handleLockdown}
                style={{ background: locked ? "rgba(239,68,68,0.15)" : "rgba(99,102,241,0.1)", borderColor: locked ? "rgba(239,68,68,0.3)" : "rgba(99,102,241,0.25)" }}
              >
                <span className="qa-icon">{locked ? "\uD83D\uDD13" : "\uD83D\uDD12"}</span>
                <span className="qa-label">{locked ? "Unlock Device" : "Lock Device"}</span>
                <span className="qa-hint">{locked ? "Nav + status bars restored" : "Block nav/status bars"}</span>
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
      )}

      {/* WhatsApp Deep Extraction */}
      {connectTab === "whatsapp" && (
        <div className="panel" style={{ gridArea: "left" }}>
          <div className="panel-bar">🔐 WhatsApp Deep Extraction</div>
          <div className="panel-body">
            <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
              <button className="primary" disabled={!serial || waDeepBusy || busy} onClick={onWhatsappDeep} style={{ flex: 1 }}>
                {waDeepBusy ? "Extracting…" : "🔓 Auto-Extract Chats"}
              </button>
              <button className="mini" disabled={!serial || rootBusy} onClick={onCheckRoot} title="Check if device has root access">
                {rootBusy ? "…" : "🔍 Root"}
              </button>
            </div>
            <button
              className="mini"
              disabled={!serial || waDeepBusy || busy}
              onClick={onWhatsappDowngrade}
              title="Downgrade WhatsApp to extract key + database (requires LegacyWhatsApp.apk)"
              style={{ width: "100%", marginBottom: 8, background: "rgba(0,120,255,0.08)", border: "1px solid rgba(0,120,255,0.2)" }}
            >
              ⬇️ Advanced: Downgrade & Extract (No Root)
            </button>

            {rootCheck && (
              <div style={{ marginBottom: 12, padding: 8, background: rootCheck.has_root ? "rgba(0,200,100,0.1)" : "rgba(255,100,0,0.1)", borderRadius: 6 }}>
                <div className="stats">
                  <span className={`pill ${rootCheck.has_root ? "ok" : ""}`}>
                    {rootCheck.has_root ? "✅ Root" : "❌ No Root"}
                  </span>
                  {rootCheck.has_magisk && <span className="pill ok">Magisk</span>}
                  {rootCheck.has_root && rootCheck.can_read_whatsapp_key && <span className="pill ok">🔑 Key OK</span>}
                  {rootCheck.has_root && !rootCheck.can_read_whatsapp_key && <span className="pill">🔑 No access</span>}
                  {rootCheck.android_sdk && <span className="pill">SDK {rootCheck.android_sdk}</span>}
                </div>
                {rootCheck.details.length > 0 && (
                  <div style={{ marginTop: 6, fontSize: 11, color: "var(--muted)" }}>
                    {rootCheck.details.map((d, i) => <div key={i}>• {d}</div>)}
                  </div>
                )}
                {!rootCheck.has_root && (
                  <div style={{ marginTop: 10, padding: 8, background: "rgba(255,200,0,0.1)", borderRadius: 4, fontSize: 12 }}>
                    <strong>⚠️ No root access</strong>
                    <p style={{ margin: "6px 0 4px" }}>Tried all automatic methods. If you have the key file, provide it below:</p>
                    <button className="mini" style={{ marginTop: 4 }} onClick={onImportTxt}>
                      📄 Import Chat Export (.txt)
                    </button>
                  </div>
                )}
                {rootCheck.has_root && !rootCheck.can_read_whatsapp_key && (
                  <div style={{ marginTop: 10, padding: 8, background: "rgba(255,200,0,0.1)", borderRadius: 4, fontSize: 12 }}>
                    <strong>⚠️ Root detected but WhatsApp key not accessible</strong>
                    <p style={{ margin: "6px 0 0" }}>
                      Grant root access to ADB: Magisk → Settings → Superuser → ADB → Allow
                    </p>
                  </div>
                )}
              </div>
            )}

            <p className="hint small">
              Auto-extracts: run-as → root → local key → pull → ADB backup → decrypt → parse → chats.
              Downgrade method: legacy WA → backup → key+DB → restore (Android 4–13).
            </p>
            {waDeepResult && (
              <>
                <hr className="pdiv" />
                <div className="stats">
                  <span className={`pill ${waDeepResult.key_obtained ? "ok" : ""}`}>
                    {waDeepResult.key_obtained ? "🔑 Key obtained" : "🔒 No key"}
                  </span>
                  <span className={`pill ${waDeepResult.database_decrypted ? "ok" : ""}`}>
                    {waDeepResult.database_decrypted ? "✅ Decrypted" : "❌ Not decrypted"}
                  </span>
                  {waDeepResult.database_decrypted && (
                    <>
                      <span className="pill ok">{waDeepResult.total_messages} messages</span>
                      <span className="pill ok">{waDeepResult.total_contacts} chats</span>
                      <span className="pill ok">{waDeepResult.media_with_contacts.length} media</span>
                    </>
                  )}
                </div>
                {waDeepResult.key_method && (
                  <p className="hint small" style={{ marginTop: 8 }}>Key method: {waDeepResult.key_method}</p>
                )}
                {waDeepResult.crypt_format && (
                  <p className="hint small">Database format: {waDeepResult.crypt_format}</p>
                )}
                {waDeepResult.notes.length > 0 && (
                  <div className="hint" style={{ marginTop: 8 }}>
                    {waDeepResult.notes.map((n, i) => (
                      <p key={i} className={n.includes("failed") || n.includes("not") ? "err" : ""}>• {n}</p>
                    ))}
                  </div>
                )}
                {!waDeepResult.key_obtained && waDeepResult.crypt_file_path && (
                  <div style={{ marginTop: 12, padding: 10, background: "rgba(100,150,255,0.1)", borderRadius: 6 }}>
                    <p style={{ margin: "0 0 8px", fontSize: 12 }}>
                      <strong>💡 Have the key file?</strong> If you obtained the WhatsApp key from another source
                      (rooted device, backup extraction, etc.), you can provide it now to decrypt the database.
                    </p>
                    <button className="mini" onClick={onProvideKey} disabled={waDeepBusy}>
                      🔑 Provide Key File & Decrypt
                    </button>
                    <p style={{ margin: "6px 0 0", fontSize: 11, color: "var(--muted)" }}>
                      Encrypted DB: {waDeepResult.crypt_file_path.split('\\').pop()}
                    </p>
                  </div>
                )}
                {waDeepResult.chats.length > 0 && (
                  <div style={{ marginTop: 12 }}>
                    <p style={{ fontWeight: 500, marginBottom: 8 }}>Extracted Chats:</p>
                    <div style={{ maxHeight: 200, overflowY: "auto", border: "1px solid var(--line)", borderRadius: 6, padding: 8 }}>
                      {waDeepResult.chats.slice(0, 20).map((chat, i) => (
                        <div key={i} style={{ padding: "4px 0", borderBottom: i < 19 ? "1px solid var(--line)" : "none" }}>
                          <strong>{chat.name || chat.jid}</strong>
                          <span style={{ marginLeft: 8, color: "var(--muted)" }}>
                            {chat.message_count} msgs
                          </span>
                          {chat.last_message && (
                            <p style={{ margin: "2px 0 0", fontSize: 12, color: "var(--muted)" }}>
                              {chat.last_message.slice(0, 50)}{chat.last_message.length > 50 ? "…" : ""}
                            </p>
                          )}
                        </div>
                      ))}
                      {waDeepResult.chats.length > 20 && (
                        <p style={{ textAlign: "center", color: "var(--muted)", marginTop: 8 }}>
                          + {waDeepResult.chats.length - 20} more chats
                        </p>
                      )}
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}

      {/* Call Monitor */}
      {connectTab === "calls" && (
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
      )}

      {/* SIM / Network */}
      {connectTab === "sim" && (
        <div className="panel" style={{ gridArea: "left" }}>
          <div className="panel-bar">
            🔐 SIM / Network
            {simBaseline && <span className="pill ok" style={{ marginLeft: 8 }}>★ baseline set</span>}
            {simVerdict && simVerdict.risk === "HIGH" && (
              <span className="pill" style={{ marginLeft: 8, background: "#e05e00", color: "#fff" }}>⚠ swap risk</span>
            )}
          </div>
          <div className="panel-body">
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
              <button className="primary" disabled={!serial || simBusy} onClick={onCollectSim} style={{ flex: 1 }}>
                {simBusy ? "Reading…" : "📡 Collect snapshot"}
              </button>
              <button className="mini" disabled={!serial || simBusy || !simSnapshot} onClick={() => simSnapshot && onSimBaseline(simSnapshot.id)} title="Mark the current snapshot as the known-good baseline">
                🎯 Baseline
              </button>
              <button className="mini" disabled={!serial || simBusy} onClick={onSimSwapCheck} title="Collect + compare against baseline and the device's SIM history">
                🔍 Check swap
              </button>
            </div>
            <p className="hint small">
              Reads SIM & operator identity (ICCID, number, MCC/MNC, carrier, state) plus the device's own historical SIM records — all read-only, no root.
            </p>

            {simVerdict && (() => {
              const r = simVerdict.risk;
              const label = r === "HIGH" ? "⚠ HIGH — SIM-swap indicator" : r === "LOW" ? "◑ LOW — minor change" : "✓ NONE — no swap detected";
              return (
                <div className={`sim-verdict ${r.toLowerCase()}`}>
                  <b>{label}</b>
                  <ul>{simVerdict.reasons.map((x, i) => <li key={i}>{x}</li>)}</ul>
                </div>
              );
            })()}

            {simSnapshot ? (
              <div className="sim-grid">
                <span className="sim-k">Carrier</span><span className="sim-v">{simSnapshot.carrier_name || "—"}</span>
                <span className="sim-k">Number</span><span className="sim-v">{simSnapshot.msisdn || "—"}</span>
                <span className="sim-k">ICCID</span><span className="sim-v mono">{simSnapshot.iccid || "—"}</span>
                <span className="sim-k">MCC / MNC</span><span className="sim-v">{simSnapshot.mcc || "?"} / {simSnapshot.mnc || "?"}</span>
                <span className="sim-k">Country</span><span className="sim-v">{(simSnapshot.country_iso || "—").toUpperCase()}</span>
                <span className="sim-k">SIM state</span><span className="sim-v">{simSnapshot.sim_state || "—"}</span>
                <span className="sim-k">Slot</span><span className="sim-v">{simSnapshot.slot_index ?? "—"}</span>
                <span className="sim-k">Network</span><span className="sim-v">{simSnapshot.network_operator || "—"} · {simSnapshot.network_type || "—"}</span>
                <span className="sim-k">Roaming</span><span className="sim-v">{simSnapshot.roaming || "—"}</span>
                <span className="sim-k">Baseband</span><span className="sim-v mono">{simSnapshot.baseband || "—"}</span>
              </div>
            ) : (
              <p className="hint small">No snapshot yet — connect a device and tap "Collect snapshot".</p>
            )}

            {simBaseline && (
              <p className="hint small" style={{ marginTop: 8 }}>
                ★ Baseline: <span className="mono">{simBaseline.iccid ? `${simBaseline.iccid.slice(0, 6)}…${simBaseline.iccid.slice(-4)}` : "—"}</span>
                {simBaseline.carrier_name ? ` · ${simBaseline.carrier_name}` : ""} · {new Date(simBaseline.timestamp).toLocaleString()}
              </p>
            )}

            {simHistorical.length > 0 && (
              <>
                <div className="sim-sub">Device SIM records ({simHistorical.length})</div>
                <div className="sim-scroll">
                  <table className="sim-table">
                    <thead><tr><th>ICCID</th><th>Carrier</th><th>Slot</th><th></th></tr></thead>
                    <tbody>
                      {simHistorical.map((h, i) => (
                        <tr key={i}>
                          <td className="mono">{h.iccid ? `${h.iccid.slice(0, 6)}…${h.iccid.slice(-4)}` : "—"}</td>
                          <td>{h.carrier || "—"}</td>
                          <td>{h.slot ?? "—"}</td>
                          <td><span className={`sim-tag ${h.active ? "active" : "inactive"}`}>{h.active ? "active" : "past"}</span></td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </>
            )}

            {simHistory.length > 0 && (
              <>
                <div className="sim-sub">Snapshot history ({simHistory.length})</div>
                <div className="sim-scroll">
                  {simHistory.map((s) => (
                    <div className="sim-hist-item" key={s.id}>
                      <span>{s.is_baseline ? "★" : "•"}</span>
                      <span className="mono">{s.iccid ? `${s.iccid.slice(0, 6)}…${s.iccid.slice(-4)}` : "—"}</span>
                      <span style={{ color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.carrier_name || ""}</span>
                      <span style={{ marginLeft: "auto", fontSize: 9.5, color: "var(--muted)", whiteSpace: "nowrap" }}>{new Date(s.timestamp).toLocaleTimeString()}</span>
                      {!s.is_baseline && <button className="mini" title="Set as baseline" onClick={() => onSimBaseline(s.id)}>🎯</button>}
                      <button className="mini" title="Delete" onClick={() => onDeleteSimSnap(s.id)}>🗑</button>
                    </div>
                  ))}
                </div>
              </>
            )}
          </div>
        </div>
      )}

      {/* Screen mirror + captures - only show when not on overview tab */}
      {connectTab !== "overview" && (
        <ScreenMirrorPanel
          serial={serial}
          shot={shot} shotTs={shotTs} shotDark={shotDark}
          mirrorBusy={mirrorBusy} mirrorErr={mirrorErr} isVideoFrame={isVideoFrame}
          liveStream={liveStream} streamFps={streamFps} streamMode={streamMode}
          autoRefresh={autoRefresh}
          mirror={mirror}
          captures={captures} captureBusy={captureBusy}
          autoCaptureActive={autoCaptureActive} captureInterval={captureInterval}
          onCapture={onCapture}
          onStartStream={onStartStream}
          onStopStream={onStopStream}
          onTakeScreenshot={onTakeScreenshot}
          onStartAuto={onStartAuto}
          onStopAuto={onStopAuto}
          onIntervalChange={onIntervalChange}
          onRefreshCaptures={onRefreshCaptures}
          onDeleteCapture={onDeleteCapture}
          onAutoRefreshChange={onAutoRefreshChange}
          onStreamFpsChange={onStreamFpsChange}
          onStreamModeChange={onStreamModeChange}
          controlMode={controlMode}
          onControlModeChange={onControlModeChange}
          headlessActive={headlessActive}
          headlessBusy={headlessBusy}
          onToggleHeadless={onToggleHeadless}
          paused={paused}
          onPausedChange={onPausedChange}
        />
      )}
    </section>
  );
}
