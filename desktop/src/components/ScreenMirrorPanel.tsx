import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ScreenCapture } from "../types";
import { playDeviceRingtone } from "../api";
import { CaptureGallery } from "./CaptureGallery";

interface ScreenMirrorPanelProps {
  serial: string | null;
  // Mirror state
  shot: string | null;
  shotTs: number;
  shotDark: boolean;
  mirrorBusy: boolean;
  mirrorErr: string;
  isVideoFrame: boolean;
  // Stream state
  liveStream: boolean;
  streamFps: number;
  streamMode: string;
  autoRefresh: boolean;
  // Capture state
  captures: ScreenCapture[];
  captureBusy: boolean;
  autoCaptureActive: boolean;
  captureInterval: number;
  // Handlers
  onCapture: (s: string) => void;
  onStartStream: (s: string) => void;
  onStopStream: (s: string) => void;
  onTakeScreenshot: () => void;
  onStartAuto: () => void;
  onStopAuto: () => void;
  onIntervalChange: (v: number) => void;
  onRefreshCaptures: () => void;
  onDeleteCapture: (id: number) => void;
  onAutoRefreshChange: (v: boolean) => void;
  onStreamFpsChange: (v: number) => void;
  onStreamModeChange: (v: string) => void;
}

export function ScreenMirrorPanel(props: ScreenMirrorPanelProps) {
  const {
    serial, shot, shotTs, shotDark, mirrorBusy, mirrorErr, isVideoFrame,
    liveStream, streamFps, streamMode, autoRefresh,
    captures, captureBusy, autoCaptureActive, captureInterval,
    onCapture, onStartStream, onStopStream,
    onTakeScreenshot, onStartAuto, onStopAuto,
    onIntervalChange, onRefreshCaptures, onDeleteCapture,
    onAutoRefreshChange, onStreamFpsChange, onStreamModeChange,
  } = props;

  const [ringing, setRinging] = useState(false);
  const [ringtoneMsg, setRingtoneMsg] = useState<string | null>(null);

  const handleRingtone = async () => {
    if (!serial || ringing) return;
    setRinging(true);
    setRingtoneMsg(null);
    try {
      await playDeviceRingtone(serial);
      setRingtoneMsg("🔔 Ringing — check the device sound");
      setTimeout(() => setRingtoneMsg(null), 4000);
    } catch (e) {
      setRingtoneMsg(`⚠ ${e}`);
      setTimeout(() => setRingtoneMsg(null), 5000);
    } finally {
      setRinging(false);
    }
  };

  return (
    <div className="panel" style={{ gridArea: "main" }}>
      <div className="panel-bar">
        📱 {liveStream ? "Live stream" : "Screen mirror"}
        {liveStream && <span className="pill ok" style={{ fontSize: 9, padding: "0 5px" }}>● {streamFps}fps</span>}
        <span className="sep" />
        <span className="hint">
          {captures.length} capture{captures.length === 1 ? "" : "s"}
          {autoCaptureActive ? " · ● auto" : ""}
        </span>
      </div>
      <div className="panel-body mirror-combo">
        {/* Left panel — Phone Monitor */}
        <div className="mirror-preview-col">
          <div className="mirror-frame">
            {shot ? (
              isVideoFrame ? (
                <video
                  src={`${convertFileSrc(shot)}?t=${shotTs}`}
                  autoPlay muted loop
                  style={{ maxWidth: '100%', maxHeight: '100%' }}
                />
              ) : (
                <img src={`${convertFileSrc(shot)}?t=${shotTs}`} alt="phone screen" />
              )
            ) : mirrorBusy ? (
              <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 8, padding: 12 }}>
                <div className="spin" style={{ width: 20, height: 20, borderWidth: 3 }} />
                <span style={{ fontSize: 11, color: "#aaa" }}>Capturing screen…</span>
              </div>
            ) : (
              <div style={{
                width: 120, borderRadius: 14, border: "3px solid #555",
                background: "#111", overflow: "hidden", boxShadow: "0 4px 20px rgba(0,0,0,0.5)"
              }}>
                {/* Status bar */}
                <div style={{
                  display: "flex", justifyContent: "space-between", alignItems: "center",
                  padding: "4px 8px", background: "#1a1a2e", fontSize: 8, color: "#aaa"
                }}>
                  <span>9:41</span>
                  <span>📶 🔋</span>
                </div>
                {/* Simulated app content */}
                <div style={{ padding: 8, background: "#16213e", minHeight: 140 }}>
                  <div style={{ fontSize: 10, color: "#e94560", fontWeight: 700, marginBottom: 6 }}>RecoverKit</div>
                  <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    {[
                      { icon: "", label: "Call Logs", count: "24" },
                      { icon: "💬", label: "Messages", count: "156" },
                      { icon: "👤", label: "Contacts", count: "89" },
                      { icon: "📷", label: "Photos", count: "342" },
                    ].map((item, i) => (
                      <div key={i} style={{
                        display: "flex", alignItems: "center", gap: 6,
                        padding: "4px 6px", background: "rgba(255,255,255,0.06)",
                        borderRadius: 6, fontSize: 9, color: "#ccc"
                      }}>
                        <span>{item.icon}</span>
                        <span style={{ flex: 1 }}>{item.label}</span>
                        <span style={{ color: "#e94560", fontWeight: 600 }}>{item.count}</span>
                      </div>
                    ))}
                  </div>
                </div>
                {/* Nav bar */}
                <div style={{
                  display: "flex", justifyContent: "center", padding: "4px 0",
                  background: "#1a1a2e"
                }}>
                  <div style={{ width: 30, height: 3, borderRadius: 2, background: "#555" }} />
                </div>
              </div>
            )}
          </div>
          {mirrorErr && <p className="hint lock-warn" style={{ padding: "1px 6px", fontSize: 9.5, margin: 0 }}>⚠ {mirrorErr}</p>}
          {shotDark && <p className="hint lock-warn" style={{ padding: "1px 6px", fontSize: 9.5, margin: 0 }}>🔒 Dark frame — locked</p>}
          {ringtoneMsg && <p className="hint" style={{ padding: "1px 6px", fontSize: 9.5, margin: 0, color: ringtoneMsg.startsWith("⚠") ? "var(--err)" : "var(--ok)", fontWeight: 500 }}>{ringtoneMsg}</p>}
          <div className="mirror-toolbar">
            <button disabled={!serial || mirrorBusy || liveStream} onClick={() => serial && onCapture(serial)} title="Take snapshot" style={{ padding: "2px 6px", fontSize: 10.5, whiteSpace: "nowrap", flexShrink: 0 }}>
              📸 Snap
            </button>
            <button disabled={!serial || ringing} onClick={handleRingtone} title="Play ringtone on device to locate it" style={{ padding: "2px 6px", fontSize: 10.5, background: ringing ? "var(--saffron)" : undefined, color: ringing ? "#fff" : undefined, whiteSpace: "nowrap", flexShrink: 0 }}>
              {ringing ? "🔔 Ringing…" : "🔔 Ring"}
            </button>
            <label className="check" style={{ fontSize: 10, whiteSpace: "nowrap", flexShrink: 0 }}>
              <input type="checkbox" checked={autoRefresh} disabled={liveStream} onChange={(e) => onAutoRefreshChange(e.target.checked)} />
              5s auto
            </label>
            <span className="mirror-sep" />
            {liveStream ? (
              <button disabled={!liveStream} onClick={() => serial && onStopStream(serial)} title="Stop stream" style={{ padding: "2px 6px", fontSize: 10.5, whiteSpace: "nowrap", flexShrink: 0 }}>⏹ Stop</button>
            ) : (
              <button className="primary" disabled={!serial} onClick={() => serial && onStartStream(serial)} title="Start live stream" style={{ padding: "2px 6px", fontSize: 10.5, whiteSpace: "nowrap", flexShrink: 0 }}>▶ Stream</button>
            )}
            <div style={{ display: "flex", gap: 3, alignItems: "center", flexShrink: 0 }}>
              <select value={streamFps} onChange={(e) => onStreamFpsChange(Number(e.target.value))} disabled={liveStream} title="FPS" style={{ width: 38, padding: "1px 2px", fontSize: 10 }}>
                <option value={5}>5</option><option value={10}>10</option><option value={15}>15</option>
                <option value={20}>20</option><option value={30}>30</option><option value={60}>60</option>
              </select>
              <select value={streamMode} onChange={(e) => onStreamModeChange(e.target.value)} disabled={liveStream} title="Mode" style={{ width: 52, padding: "1px 2px", fontSize: 10 }}>
                <option value="normal">Stable</option><option value="fast">Fast</option>
              </select>
            </div>
          </div>
        </div>

        {/* Right panel — Capture gallery */}
        <CaptureGallery
          serial={serial}
          captures={captures}
          captureBusy={captureBusy}
          autoCaptureActive={autoCaptureActive}
          captureInterval={captureInterval}
          onCapture={onTakeScreenshot}
          onStartAuto={onStartAuto}
          onStopAuto={onStopAuto}
          onIntervalChange={onIntervalChange}
          onRefresh={onRefreshCaptures}
          onDelete={onDeleteCapture}
        />
      </div>
    </div>
  );
}
