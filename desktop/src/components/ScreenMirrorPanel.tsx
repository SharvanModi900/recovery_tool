import { useState, useRef, useCallback, useEffect } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { ScreenCapture, DeviceMirror } from "../types";
import { playDeviceRingtone, screenTap, screenSwipe, screenText, screenKey, deviceRotate, deviceAutoRotate } from "../api";
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
  // Device info for coordinate mapping
  mirror: DeviceMirror | null;
  // Control mode
  controlMode: boolean;
  onControlModeChange: (v: boolean) => void;
  // Headless mode
  headlessActive: boolean;
  headlessBusy: boolean;
  onToggleHeadless: (v: boolean) => void;
  // Pause
  paused: boolean;
  onPausedChange: (v: boolean) => void;
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
    liveStream, streamFps, streamMode, autoRefresh, mirror,
    controlMode, onControlModeChange,
    headlessActive, headlessBusy, onToggleHeadless,
    paused, onPausedChange,
    captures, captureBusy, autoCaptureActive, captureInterval,
    onCapture, onStartStream, onStopStream,
    onTakeScreenshot, onStartAuto, onStopAuto,
    onIntervalChange, onRefreshCaptures, onDeleteCapture,
    onAutoRefreshChange, onStreamFpsChange, onStreamModeChange,
  } = props;

  const [ringing, setRinging] = useState(false);
  const [ringtoneMsg, setRingtoneMsg] = useState<string | null>(null);

  // ── Remote Control State ──
  const [typeText, setTypeText] = useState("");
  const [tapFeedback, setTapFeedback] = useState<{ x: number; y: number } | null>(null);
  const imgRef = useRef<HTMLDivElement>(null);
  const dragStart = useRef<{ x: number; y: number; time: number } | null>(null);

  // ── Image Preload — prevents flash/blank between frames ──
  const [displaySrc, setDisplaySrc] = useState<string | null>(null);
  const [displayReady, setDisplayReady] = useState(false);
  const preloadImgRef = useRef<HTMLImageElement | null>(null);

  useEffect(() => {
    if (!shot) { setDisplaySrc(null); return; }
    const src = `${convertFileSrc(shot)}?t=${shotTs}`;
    // Reuse a single Image object for preloading
    if (!preloadImgRef.current) {
      preloadImgRef.current = new Image();
    }
    const img = preloadImgRef.current;
    img.onload = () => {
      setDisplaySrc(src);
      setDisplayReady(true);
    };
    img.src = src;
  }, [shot, shotTs]);

  // ── Device Rotation — controls the real phone display orientation ──
  const [rotation, setRotation] = useState(0); // degrees: 0, 90, 180, 270
  const [rotBusy, setRotBusy] = useState(false);
  const [rotMsg, setRotMsg] = useState<string | null>(null);
  const spinIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [spinning, setSpinning] = useState(false);
  const [spinSpeed, setSpinSpeed] = useState(600); // ms between rotations

  // Speed presets
  const speedPresets = [
    { label: "🐢 Slow", ms: 1200 },
    { label: "🚶 Med", ms: 600 },
    { label: "🏃 Fast", ms: 300 },
    { label: "⚡ Turbo", ms: 150 },
  ];

  // Map degrees → Android rotation value (0-3)
  const degToAndroid = (deg: number) => Math.round(deg / 90) % 4;

  // Helper to (re)start the spin interval at the current speed
  const restartSpin = useCallback((serial: string, startDeg: number, speed: number) => {
    if (spinIntervalRef.current) clearInterval(spinIntervalRef.current);
    let current = startDeg;
    spinIntervalRef.current = setInterval(async () => {
      current = (current + 90) % 360;
      setRotation(current);
      try { await deviceRotate(serial, degToAndroid(current)); } catch { /* ignore */ }
    }, speed);
  }, []);

  const rotateTo = useCallback(async (deg: number) => {
    if (!serial || rotBusy) return;
    // Stop any spinning
    if (spinIntervalRef.current) { clearInterval(spinIntervalRef.current); spinIntervalRef.current = null; setSpinning(false); }
    setRotBusy(true);
    setRotMsg(null);
    setRotation(deg);
    try {
      const result = await deviceRotate(serial, degToAndroid(deg));
      setRotMsg(result);
      setTimeout(() => setRotMsg(null), 3000);
    } catch (e) {
      setRotMsg(`⚠ ${e}`);
      setTimeout(() => setRotMsg(null), 4000);
    }
    finally { setRotBusy(false); }
  }, [serial, rotBusy]);

  // Spin: cycle through all 4 rotations on the real device
  const startSpin = useCallback(() => {
    if (!serial) return;
    if (spinning) {
      // Stop
      if (spinIntervalRef.current) { clearInterval(spinIntervalRef.current); spinIntervalRef.current = null; }
      setSpinning(false);
      return;
    }
    setSpinning(true);
    restartSpin(serial, rotation, spinSpeed);
  }, [serial, spinning, rotation, spinSpeed, restartSpin]);

  // Change speed while spinning — restart interval at new speed
  const changeSpinSpeed = useCallback((ms: number) => {
    setSpinSpeed(ms);
    if (spinning && serial) {
      restartSpin(serial, rotation, ms);
    }
  }, [spinning, serial, rotation, restartSpin]);

  // Restore auto-rotation (accelerometer)
  const resetRotation = useCallback(async () => {
    if (!serial) return;
    if (spinIntervalRef.current) { clearInterval(spinIntervalRef.current); spinIntervalRef.current = null; setSpinning(false); }
    setRotation(0);
    try { await deviceAutoRotate(serial); } catch { /* ignore */ }
  }, [serial]);

  // Cleanup spin interval on unmount
  useEffect(() => {
    return () => {
      if (spinIntervalRef.current) clearInterval(spinIntervalRef.current);
    };
  }, []);

  // Parse device resolution for coordinate mapping
  const deviceRes = (() => {
    if (!mirror?.resolution) return { w: 1080, h: 2400 };
    const m = mirror.resolution.match(/(\d+)\s*x\s*(\d+)/);
    return m ? { w: parseInt(m[1]), h: parseInt(m[2]) } : { w: 1080, h: 2400 };
  })();

  // Map click position on the displayed image to device coordinates
  const mapTo = useCallback((clientX: number, clientY: number) => {
    const el = imgRef.current;
    if (!el) return null;
    const rect = el.getBoundingClientRect();
    const relX = (clientX - rect.left) / rect.width;
    const relY = (clientY - rect.top) / rect.height;
    if (relX < 0 || relX > 1 || relY < 0 || relY > 1) return null;
    return { x: Math.round(relX * deviceRes.w), y: Math.round(relY * deviceRes.h) };
  }, [deviceRes]);

  const handlePointerDown = useCallback((e: React.PointerEvent) => {
    if (!controlMode || !serial) return;
    const pt = mapTo(e.clientX, e.clientY);
    if (pt) dragStart.current = { ...pt, time: Date.now() };
  }, [controlMode, serial, mapTo]);

  const handlePointerUp = useCallback(async (e: React.PointerEvent) => {
    if (!controlMode || !serial || !dragStart.current) return;
    const end = mapTo(e.clientX, e.clientY);
    const start = dragStart.current;
    dragStart.current = null;
    if (!end) return;

    const dx = Math.abs(end.x - start.x);
    const dy = Math.abs(end.y - start.y);
    const dt = Date.now() - start.time;

    if (dx < 20 && dy < 20 && dt < 500) {
      // Tap
      setTapFeedback({ x: end.x, y: end.y });
      setTimeout(() => setTapFeedback(null), 300);
      await screenTap(serial, start.x, start.y).catch(() => {});
    } else if (dx > 30 || dy > 30) {
      // Swipe
      await screenSwipe(serial, start.x, start.y, end.x, end.y, Math.max(100, dt)).catch(() => {});
    }
  }, [controlMode, serial, mapTo]);

  const handleSendText = useCallback(async () => {
    if (!serial || !typeText.trim()) return;
    await screenText(serial, typeText).catch(() => {});
    setTypeText("");
  }, [serial, typeText]);

  const handleKey = useCallback(async (keycode: number) => {
    if (!serial) return;
    await screenKey(serial, keycode).catch(() => {});
  }, [serial]);

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
        📱 {headlessActive ? "Headless mode" : paused ? "Paused" : liveStream ? "Live stream" : "Screen mirror"}
        {liveStream && !paused && <span className="pill ok" style={{ fontSize: 9, padding: "0 5px" }}>● {streamFps}fps</span>}
        {paused && <span className="pill" style={{ fontSize: 9, padding: "0 5px", background: "#f59e0b", color: "#000" }}>⏸ paused</span>}
        {headlessActive && <span className="pill" style={{ fontSize: 9, padding: "0 5px", background: "#7c3aed", color: "#fff" }}>🕶 screen OFF</span>}
        <span className="sep" />
        <span className="hint">
          {captures.length} capture{captures.length === 1 ? "" : "s"}
          {autoCaptureActive ? " · ● auto" : ""}
        </span>
        <span style={{ flex: 1 }} />
        {/* Pause / Resume button */}
        {liveStream && (
          <button
            onClick={() => onPausedChange(!paused)}
            title={paused ? "Resume live stream" : "Pause — freeze current frame"}
            style={{
              padding: "3px 10px", fontSize: 11, fontWeight: 600, whiteSpace: "nowrap",
              borderRadius: 6, border: paused ? "1px solid #f59e0b" : "1px solid #555",
              background: paused ? "#f59e0b" : "rgba(245,158,11,0.12)",
              color: paused ? "#000" : "#f59e0b",
              cursor: "pointer", transition: "all 0.2s ease",
            }}
          >
            {paused ? "▶ Resume" : "⏸ Pause"}
          </button>
        )}
        {/* Prominent headless mode button in the top bar */}
        <button
          disabled={!serial || headlessBusy}
          onClick={() => onToggleHeadless(!headlessActive)}
          title={headlessActive ? "Exit headless — wake phone screen" : "Headless — turn phone screen off, control from desktop"}
          style={{
            padding: "3px 10px", fontSize: 11, fontWeight: 600, whiteSpace: "nowrap",
            borderRadius: 6, border: headlessActive ? "1px solid #a78bfa" : "1px solid #555",
            background: headlessActive ? "#7c3aed" : "rgba(124,58,237,0.15)",
            color: headlessActive ? "#fff" : "#a78bfa",
            cursor: headlessBusy ? "wait" : "pointer",
            animation: headlessActive ? "headless-pulse 2s ease-in-out infinite" : undefined,
            transition: "all 0.2s ease",
          }}
        >
          {headlessBusy ? "⏳ Switching…" : headlessActive ? "🕶 Exit Headless" : "🕶 Headless Mode"}
        </button>
      </div>
      <div className="panel-body mirror-combo">
        {/* Left panel — Phone Monitor */}
        <div className="mirror-preview-col">
          <div
            className="mirror-frame"
            ref={imgRef}
            style={controlMode && shot ? { cursor: "crosshair", userSelect: "none" } : undefined}
            onPointerDown={handlePointerDown}
            onPointerUp={handlePointerUp}
          >
            {shot ? (
              isVideoFrame ? (
                <video
                  key={displaySrc || undefined}
                  src={displaySrc || undefined}
                  autoPlay muted loop
                  style={{ maxWidth: '100%', maxHeight: '100%', pointerEvents: 'none', transition: 'opacity 0.15s ease', opacity: displayReady ? 1 : 0.6 }}
                />
              ) : (
                <img
                  src={displaySrc || undefined}
                  alt="phone screen"
                  style={{ pointerEvents: 'none', transition: 'opacity 0.15s ease', opacity: displayReady ? 1 : 0.6 }}
                  draggable={false}
                />
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
          {/* Tap feedback indicator */}
          {tapFeedback && (
            <div style={{ position: "absolute", top: 8, left: "50%", transform: "translateX(-50%)", background: "rgba(59,130,246,0.85)", color: "#fff", padding: "2px 10px", borderRadius: 10, fontSize: 10, fontWeight: 600, zIndex: 5, pointerEvents: "none" }}>
              tap ({tapFeedback.x}, {tapFeedback.y})
            </div>
          )}
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
            {/* Remote control toggle */}
            <button
              disabled={!serial || !shot}
              onClick={() => onControlModeChange(!controlMode)}
              title={controlMode ? "Disable touch control" : "Enable click/swipe on screen"}
              style={{ padding: "2px 6px", fontSize: 10.5, whiteSpace: "nowrap", flexShrink: 0, background: controlMode ? "var(--ok, #22c55e)" : undefined, color: controlMode ? "#fff" : undefined }}
            >
              {controlMode ? "🎮 Control ON" : "🎮 Control"}
            </button>
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
          {/* Remote control bar — nav keys + text input */}
          {controlMode && (
            <div style={{ display: "flex", gap: 4, alignItems: "center", padding: "4px 6px", background: "var(--panel-alt, #1a1a2e)", borderRadius: "0 0 6px 6px", flexWrap: "wrap" }}>
              <button onClick={() => handleKey(4)} title="Back (KEYCODE_BACK)" style={{ padding: "2px 8px", fontSize: 11, flexShrink: 0 }}>◀ Back</button>
              <button onClick={() => handleKey(3)} title="Home (KEYCODE_HOME)" style={{ padding: "2px 8px", fontSize: 11, flexShrink: 0 }}>⬤ Home</button>
              <button onClick={() => handleKey(187)} title="Recents (KEYCODE_APP_SWITCH)" style={{ padding: "2px 8px", fontSize: 11, flexShrink: 0 }}>▢ Recent</button>
              <button onClick={() => handleKey(26)} title="Power (KEYCODE_POWER)" style={{ padding: "2px 8px", fontSize: 11, flexShrink: 0 }}>⏻ Power</button>
              <span style={{ flex: 1 }} />
              <input
                className="mono"
                value={typeText}
                onChange={(e) => setTypeText(e.target.value)}
                onKeyDown={(e) => { if (e.key === "Enter") handleSendText(); }}
                placeholder="Type text → send to device"
                style={{ width: 160, padding: "2px 6px", fontSize: 10.5, flexShrink: 1, minWidth: 80 }}
              />
              <button onClick={handleSendText} disabled={!typeText.trim()} title="Send text to focused field on device" style={{ padding: "2px 6px", fontSize: 10.5, flexShrink: 0 }}>⌨ Send</button>
            </div>
          )}
          {/* Device rotation bar — controls the real phone display */}
          <div style={{
            display: "flex", gap: 4, alignItems: "center", padding: "3px 6px",
            background: "linear-gradient(90deg, rgba(124,58,237,0.08), rgba(236,72,153,0.08))",
            borderRadius: "0 0 6px 6px", borderTop: "1px solid rgba(124,58,237,0.15)",
            overflowX: "auto", flexWrap: "nowrap",
          }}>
            <span style={{ fontSize: 10, color: "#a78bfa", fontWeight: 600, flexShrink: 0 }}>📱 Device Rotate</span>
            {[0, 90, 180, 270].map((deg) => (
              <button
                key={deg}
                disabled={!serial || rotBusy}
                onClick={() => rotateTo(deg)}
                title={`Rotate device to ${deg}°`}
                style={{
                  padding: "1px 6px", fontSize: 10, flexShrink: 0, borderRadius: 4,
                  border: rotation === deg && !spinning ? "1px solid #a78bfa" : "1px solid transparent",
                  background: rotation === deg && !spinning ? "rgba(124,58,237,0.25)" : "rgba(255,255,255,0.04)",
                  color: rotation === deg && !spinning ? "#c4b5fd" : "#888",
                  fontWeight: rotation === deg && !spinning ? 600 : 400,
                  cursor: "pointer", transition: "all 0.15s ease",
                }}
              >
                {deg}°
              </button>
            ))}
            <span style={{ flex: 1 }} />
            {/* Auto-rotate restore button */}
            <button
              disabled={!serial}
              onClick={resetRotation}
              title="Restore auto-rotation (accelerometer)"
              style={{
                padding: "1px 6px", fontSize: 10, flexShrink: 0, borderRadius: 4,
                background: rotation === 0 && !spinning ? "rgba(34,197,94,0.15)" : "rgba(255,255,255,0.04)",
                color: rotation === 0 && !spinning ? "#86efac" : "#888",
                border: rotation === 0 && !spinning ? "1px solid rgba(34,197,94,0.3)" : "1px solid transparent",
                cursor: "pointer",
              }}
            >
              🔄 Auto
            </button>
            {/* Speed selector — visible when spinning or always as quick access */}
            {spinning && (
              <div style={{ display: "flex", gap: 2, alignItems: "center", flexShrink: 0 }}>
                {speedPresets.map((p) => (
                  <button
                    key={p.ms}
                    onClick={() => changeSpinSpeed(p.ms)}
                    title={`Set spin speed: ${p.ms}ms per rotation`}
                    style={{
                      padding: "1px 5px", fontSize: 9, flexShrink: 0, borderRadius: 3,
                      border: spinSpeed === p.ms ? "1px solid #ec4899" : "1px solid transparent",
                      background: spinSpeed === p.ms ? "rgba(236,72,153,0.2)" : "rgba(255,255,255,0.04)",
                      color: spinSpeed === p.ms ? "#f472b6" : "#666",
                      cursor: "pointer", transition: "all 0.15s ease",
                    }}
                  >
                    {p.label}
                  </button>
                ))}
              </div>
            )}
            {/* Spin button — cycles device through all rotations */}
            <button
              disabled={!serial}
              onClick={startSpin}
              title={spinning ? "Stop cycling device rotation" : "Cycle device through all 4 rotations!"}
              style={{
                padding: "2px 10px", fontSize: 10.5, fontWeight: 700, flexShrink: 0,
                borderRadius: 12, border: "1px solid",
                borderColor: spinning ? "#ec4899" : "#a78bfa",
                background: spinning ? "linear-gradient(135deg, #7c3aed, #ec4899)" : "rgba(124,58,237,0.12)",
                color: spinning ? "#fff" : "#c4b5fd",
                cursor: "pointer",
                transition: "all 0.2s ease",
              }}
            >
              {spinning ? "⏹ Stop" : "🌀 Spin!"}
            </button>
          </div>
          {rotMsg && (
            <div style={{
              padding: "2px 8px", fontSize: 10, fontWeight: 500,
              color: rotMsg.startsWith("⚠") ? "var(--err, #ef4444)" : "var(--ok, #22c55e)",
              background: rotMsg.startsWith("⚠") ? "rgba(239,68,68,0.08)" : "rgba(34,197,94,0.08)",
              borderRadius: 4, margin: "2px 6px",
            }}>
              {rotMsg}
            </div>
          )}
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
