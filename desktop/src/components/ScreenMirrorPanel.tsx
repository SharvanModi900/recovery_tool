import { convertFileSrc } from "@tauri-apps/api/core";
import type { ScreenCapture } from "../types";
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
            ) : (
              <div className="empty" style={{ padding: 8, fontSize: 11 }}>{mirrorBusy ? "capturing…" : serial ? "no snapshot yet" : "select a device"}</div>
            )}
          </div>
          {mirrorErr && <p className="hint lock-warn" style={{ padding: "1px 6px", fontSize: 9.5, margin: 0 }}>⚠ {mirrorErr}</p>}
          {shotDark && <p className="hint lock-warn" style={{ padding: "1px 6px", fontSize: 9.5, margin: 0 }}>🔒 Dark frame — locked</p>}
          <div className="mirror-toolbar">
            <button disabled={!serial || mirrorBusy || liveStream} onClick={() => serial && onCapture(serial)} title="Take snapshot" style={{ padding: "2px 6px", fontSize: 10.5 }}>
              📸 Snap
            </button>
            <label className="check" style={{ fontSize: 10 }}>
              <input type="checkbox" checked={autoRefresh} disabled={liveStream} onChange={(e) => onAutoRefreshChange(e.target.checked)} />
              5s auto
            </label>
            <span className="mirror-sep" />
            {liveStream ? (
              <button disabled={!liveStream} onClick={() => serial && onStopStream(serial)} title="Stop stream" style={{ padding: "2px 6px", fontSize: 10.5 }}>⏹ Stop</button>
            ) : (
              <button className="primary" disabled={!serial} onClick={() => serial && onStartStream(serial)} title="Start live stream" style={{ padding: "2px 6px", fontSize: 10.5 }}>▶ Stream</button>
            )}
            <div style={{ display: "flex", gap: 3, alignItems: "center" }}>
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
