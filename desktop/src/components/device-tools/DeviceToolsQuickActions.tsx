import { useState } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, miniBtn } from "./shared";

export function DeviceToolsQuickActions({ serial, busy, run }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const [ringing, setRinging] = useState(false);
  const [locked, setLocked] = useState(false);
  const [chargingStopped, setChargingStopped] = useState(false);

  const handleRing = async () => {
    if (!serial) return;
    try {
      if (ringing) {
        await api.stopRingtone(serial);
        setRinging(false);
      } else {
        await api.startRingtoneLoop(serial);
        setRinging(true);
      }
    } catch { /* ignore */ }
  };

  const handleLock = async () => {
    if (!serial) return;
    try {
      if (locked) {
        await api.releaseDevice(serial);
        setLocked(false);
      } else {
        await api.lockdownDevice(serial);
        setLocked(true);
      }
    } catch { /* ignore */ }
  };

  const handleCharge = async () => {
    if (!serial) return;
    try {
      if (chargingStopped) {
        await api.startCharging(serial);
        setChargingStopped(false);
      } else {
        await api.stopCharging(serial);
        setChargingStopped(true);
      }
    } catch { /* ignore */ }
  };

  const handleScreenshot = () => {
    if (!serial) return;
    run("screenshot", async () => { const r = await api.deviceScreenshot(serial); return `Screenshot saved: ${JSON.stringify(r)}`; });
  };

  return (
    <div style={{ padding: "10px 12px" }}>
      <div style={{ fontSize: 11, color: "var(--text-secondary, #888)", marginBottom: 12 }}>
        Quick device controls — ring, lock, charge, and capture from the connected device.
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, marginBottom: 16 }}>
        {/* Ring */}
        <button disabled={!serial} onClick={handleRing}
          style={{ padding: "12px 16px", borderRadius: 8, border: `1px solid ${ringing ? "rgba(239,68,68,0.3)" : "var(--border, #444)"}`, background: ringing ? "rgba(239,68,68,0.12)" : "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: serial ? "pointer" : "default", textAlign: "left" }}>
          <div style={{ fontSize: 20, marginBottom: 4 }}>{ringing ? "⏹" : "🔔"}</div>
          <div style={{ fontSize: 12, fontWeight: 600 }}>{ringing ? "Stop Ringing" : "Ring Phone"}</div>
          <div style={{ fontSize: 9.5, color: "var(--text-secondary, #888)", marginTop: 2 }}>{ringing ? "Tap to stop the ringtone" : "Loop ringtone until stopped"}</div>
        </button>
        {/* Lock */}
        <button disabled={!serial} onClick={handleLock}
          style={{ padding: "12px 16px", borderRadius: 8, border: `1px solid ${locked ? "rgba(239,68,68,0.3)" : "var(--border, #444)"}`, background: locked ? "rgba(239,68,68,0.12)" : "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: serial ? "pointer" : "default", textAlign: "left" }}>
          <div style={{ fontSize: 20, marginBottom: 4 }}>{locked ? "🔓" : "🔒"}</div>
          <div style={{ fontSize: 12, fontWeight: 600 }}>{locked ? "Unlock Device" : "Lock Device"}</div>
          <div style={{ fontSize: 9.5, color: "var(--text-secondary, #888)", marginTop: 2 }}>{locked ? "Navigation + status bars restored" : "Block navigation + status bars"}</div>
        </button>
        {/* Charge */}
        <button disabled={!serial} onClick={handleCharge}
          style={{ padding: "12px 16px", borderRadius: 8, border: `1px solid ${chargingStopped ? "rgba(245,158,11,0.3)" : "var(--border, #444)"}`, background: chargingStopped ? "rgba(245,158,11,0.12)" : "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: serial ? "pointer" : "default", textAlign: "left" }}>
          <div style={{ fontSize: 20, marginBottom: 4 }}>{chargingStopped ? "⚡" : "🔌"}</div>
          <div style={{ fontSize: 12, fontWeight: 600 }}>{chargingStopped ? "Start Charging" : "Stop Charging"}</div>
          <div style={{ fontSize: 9.5, color: "var(--text-secondary, #888)", marginTop: 2 }}>{chargingStopped ? "Resume normal charging" : "Disable charging for preservation"}</div>
        </button>
        {/* Screenshot */}
        <button disabled={!serial || isBusy("screenshot")} onClick={handleScreenshot}
          style={{ padding: "12px 16px", borderRadius: 8, border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: serial ? "pointer" : "default", textAlign: "left" }}>
          <div style={{ fontSize: 20, marginBottom: 4 }}>{isBusy("screenshot") ? "⏳" : "📸"}</div>
          <div style={{ fontSize: 12, fontWeight: 600 }}>Screenshot</div>
          <div style={{ fontSize: 9.5, color: "var(--text-secondary, #888)", marginTop: 2 }}>Capture the device screen</div>
        </button>
      </div>
      {/* Battery status quick read */}
      <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
        <span style={{ fontSize: 10, color: "var(--text-secondary, #888)" }}>Battery:</span>
        <button disabled={!serial || isBusy("batt")} onClick={() => run("batt", () => api.getBatteryStatus(serial!))} style={miniBtn}>
          {isBusy("batt") ? "⏳" : "🔋"} Read Status
        </button>
      </div>
    </div>
  );
}
