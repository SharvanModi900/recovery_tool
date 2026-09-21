import { useState } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, miniBtn } from "./shared";

export function DeviceToolsDisplayTab({ serial, busy, run, addResult }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const [zoomLevel, setZoomLevel] = useState(0);
  const [baseDensity, setBaseDensity] = useState(0);
  const [zoomMsg, setZoomMsg] = useState<string | null>(null);
  const [activeAnim, setActiveAnim] = useState<string | null>(null);

  // Load base density on first render if not set
  if (!baseDensity && serial) {
    api.deviceGetDensity(serial).then(info => {
      const m = info.match(/(\d{2,3})/);
      if (m) setBaseDensity(parseInt(m[1]));
    }).catch(() => {});
  }

  return (
    <div>
      {/* Device Zoom Controls */}
      <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 6 }}>
          🔍 Device Zoom <span style={{ fontWeight: 400, color: "var(--text-secondary, #888)" }}>— changes the real phone display density (DPI)</span>
        </div>
        {baseDensity > 0 && (
          <div style={{ fontSize: 10, color: "var(--text-secondary, #888)", marginBottom: 6 }}>
            Device default: <span style={{ color: "var(--ok, #22c55e)", fontWeight: 600 }}>{baseDensity} DPI</span>
          </div>
        )}
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
          {[
            { label: "🔍−− Much Smaller", level: -2 },
            { label: "🔍− Smaller", level: -1 },
            { label: "⚖ Default", level: 0 },
            { label: "🔍+ Larger", level: 1 },
            { label: "🔍++ Much Larger", level: 2 },
          ].map(p => {
            const base = baseDensity > 0 ? baseDensity : 420;
            const dpi = base + (p.level * 80);
            const isActive = zoomLevel === p.level;
            return (
              <button key={p.level} disabled={!serial || isBusy("zoom")}
                onClick={() => {
                  setZoomLevel(p.level);
                  setZoomMsg(null);
                  const fn = p.level === 0
                    ? () => api.deviceZoomReset(serial!)
                    : () => api.deviceZoom(serial!, dpi);
                  run("zoom", fn).then(() => {
                    setZoomMsg(p.level === 0 ? "Reset to default" : `Set to ${dpi} DPI`);
                    setTimeout(() => setZoomMsg(null), 3000);
                  });
                }}
                style={{
                  padding: "5px 12px", fontSize: 11, borderRadius: 6,
                  border: isActive ? "1px solid #60a5fa" : "1px solid var(--border, #444)",
                  background: isActive ? "rgba(96,165,250,0.2)" : "var(--panel-alt, #1a1a2e)",
                  color: isActive ? "#93c5fd" : "var(--text, #eee)",
                  cursor: serial ? "pointer" : "default", fontWeight: isActive ? 600 : 400,
                }}>
                {isBusy("zoom") ? "⏳" : p.label}
              </button>
            );
          })}
        </div>
        {zoomMsg && (
          <div style={{ marginTop: 6, fontSize: 10, color: "var(--ok, #22c55e)", fontWeight: 500 }}>
            ✅ {zoomMsg}
          </div>
        )}
      </div>

      {/* Device Rotation Controls */}
      <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 6 }}>
          📱 Device Rotation <span style={{ fontWeight: 400, color: "var(--text-secondary, #888)" }}>— controls the real phone orientation</span>
        </div>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
          {[
            { label: "0° Portrait", rot: 0 },
            { label: "90° Landscape", rot: 1 },
            { label: "180° Upside-down", rot: 2 },
            { label: "270° Landscape", rot: 3 },
          ].map(p => (
            <button key={p.rot} disabled={!serial || isBusy("rotate")}
              onClick={() => run("rotate", () => api.deviceRotate(serial!, p.rot))}
              style={miniBtn}>
              {isBusy("rotate") ? "⏳" : p.label}
            </button>
          ))}
          <button disabled={!serial || isBusy("auto")}
            onClick={() => run("auto", () => api.deviceAutoRotate(serial!))}
            style={{ ...miniBtn, borderColor: "rgba(34,197,94,0.3)", color: "#86efac" }}>
            {isBusy("auto") ? "⏳" : "🔄 Auto (Accelerometer)"}
          </button>
        </div>
      </div>

      {/* Fun Animation Effects */}
      <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 4 }}>
          ✨ Fun Animations <span style={{ fontWeight: 400, color: "var(--text-secondary, #888)" }}>— visual effects on the mirror preview</span>
        </div>
        <div style={{ fontSize: 9, color: "var(--text-secondary, #666)", marginBottom: 8 }}>
          Click to trigger — effects apply to the screen mirror preview
        </div>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          {[
            { id: "scatter", label: "💥 Scatter" },
            { id: "slide", label: "🎿 Slide" },
            { id: "bounce", label: "🏀 Bounce" },
            { id: "glitch", label: "📺 Glitch" },
            { id: "flip", label: "🃏 Flip" },
            { id: "shake", label: "🫨 Shake" },
            { id: "blur", label: "🌫 Blur" },
            { id: "melt", label: "🫠 Melt" },
            { id: "swing", label: "🎪 Swing" },
            { id: "zoom-pulse", label: "💓 Pulse" },
          ].map(fx => (
            <button key={fx.id}
              onClick={() => {
                setActiveAnim(fx.id);
                setTimeout(() => setActiveAnim(null), 800);
                addResult("anim", true, `✨ ${fx.label} animation triggered`);
              }}
              style={{
                padding: "4px 10px", fontSize: 11, borderRadius: 6,
                border: activeAnim === fx.id ? "1px solid #f472b6" : "1px solid var(--border, #444)",
                background: activeAnim === fx.id ? "rgba(244,114,182,0.2)" : "var(--panel-alt, #1a1a2e)",
                color: activeAnim === fx.id ? "#f9a8d4" : "var(--text, #eee)",
                cursor: "pointer", fontWeight: activeAnim === fx.id ? 600 : 400,
                transition: "all 0.15s ease",
              }}>
              {fx.label}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
