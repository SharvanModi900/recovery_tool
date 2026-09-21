import { useState } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, miniBtn } from "./shared";

export function DeviceToolsCoolingTab({ serial, results, busy, run }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const [brightness, setBrightness] = useState(100);
  const tempResult = results.find(r => r.id === "temp" && r.ok);

  return (
    <div>
      {/* One-click cooling */}
      <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, color: "var(--text-secondary, #888)", marginBottom: 8 }}>
          ❄ Device Cooling Controls — reduce temperature by lowering power draw
        </div>
        <button disabled={!serial || isBusy("cool")} onClick={() => run("cool", () => api.applyCoolingMode(serial!))}
          style={{ padding: "6px 16px", fontSize: 12, borderRadius: 6, border: "1px solid var(--border, #444)", background: "linear-gradient(135deg, #3b82f6, #06b6d4)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600, marginBottom: 10 }}>
          {isBusy("cool") ? "⏳ Applying..." : "❄ Apply Full Cooling Mode"}
        </button>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", alignItems: "center" }}>
          <button disabled={!serial || isBusy("saver_on")} onClick={() => run("saver_on", () => api.setBatterySaver(serial!, true))} style={miniBtn}>
            {isBusy("saver_on") ? "⏳" : "🔋"} Battery Saver ON
          </button>
          <button disabled={!serial || isBusy("saver_off")} onClick={() => run("saver_off", () => api.setBatterySaver(serial!, false))} style={miniBtn}>
            {isBusy("saver_off") ? "⏳" : "🔋"} Battery Saver OFF
          </button>
          <button disabled={!serial || isBusy("sync_off")} onClick={() => run("sync_off", () => api.setAutoSync(serial!, false))} style={miniBtn}>
            {isBusy("sync_off") ? "⏳" : "🔄"} Auto-Sync OFF
          </button>
          <button disabled={!serial || isBusy("sync_on")} onClick={() => run("sync_on", () => api.setAutoSync(serial!, true))} style={miniBtn}>
            {isBusy("sync_on") ? "⏳" : "🔄"} Auto-Sync ON
          </button>
          <button disabled={!serial || isBusy("kill")} onClick={() => run("kill", () => api.killHeavyProcesses(serial!))} style={miniBtn}>
            {isBusy("kill") ? "⏳" : "💀"} Kill Heavy Apps
          </button>
          <button disabled={!serial || isBusy("gov")} onClick={() => run("gov", () => api.setCpuGovernor(serial!, "powersave"))} style={miniBtn}>
            {isBusy("gov") ? "⏳" : "⚡"} CPU Powersave
          </button>
        </div>
      </div>
      {/* Brightness slider */}
      <div style={{ display: "flex", gap: 8, padding: "8px 12px", alignItems: "center", borderBottom: "1px solid var(--border, #222)" }}>
        <span style={{ fontSize: 11, color: "var(--text-secondary, #888)" }}>☀ Brightness:</span>
        <input type="range" min={0} max={255} value={brightness} onChange={e => setBrightness(Number(e.target.value))} style={{ flex: 1, accentColor: "#f59e0b" }} />
        <span style={{ fontSize: 11, fontFamily: "monospace", color: "var(--text, #eee)", width: 30 }}>{brightness}</span>
        <button disabled={!serial || isBusy("bright")} onClick={() => run("bright", () => api.setScreenBrightness(serial!, brightness))} style={miniBtn}>
          {isBusy("bright") ? "⏳" : "Set"}
        </button>
      </div>
      {/* Temperature readout */}
      <div style={{ padding: "6px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 11, color: "var(--text-secondary, #888)" }}>🌡 Current Temperature</span>
          <button disabled={!serial || isBusy("temp")} onClick={() => run("temp", () => api.getDeviceTemperatures(serial!))} style={miniBtn}>
            {isBusy("temp") ? "⏳" : "🔄"} Read
          </button>
        </div>
        {tempResult && (
          <pre style={{ margin: "4px 0 0", fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap" }}>
            {tempResult.text}
          </pre>
        )}
      </div>
    </div>
  );
}
