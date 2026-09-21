import { useState, useEffect } from "react";
import * as api from "../../api";
import type { DeviceFullInfo } from "../../types";
import type { DeviceToolsTabProps } from "./shared";

function Section({ icon, title, children, defaultOpen = true }: { icon: string; title: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div style={{ marginBottom: 2 }}>
      <button onClick={() => setOpen(!open)} style={{ width: "100%", display: "flex", justifyContent: "space-between", alignItems: "center", padding: "6px 8px", background: "var(--panel-alt, #1a1a2e)", border: "1px solid var(--border, #333)", borderRadius: open ? "6px 6px 0 0" : 6, cursor: "pointer", color: "var(--text, #eee)", fontSize: 11, fontWeight: 600 }}>
        <span>{icon} {title}</span>
        <span style={{ fontSize: 9, color: "var(--text-secondary, #888)" }}>{open ? "▾" : "▸"}</span>
      </button>
      {open && (
        <div style={{ padding: "6px 8px", border: "1px solid var(--border, #333)", borderTop: "none", borderRadius: "0 0 6px 6px", background: "rgba(0,0,0,0.15)" }}>
          {children}
        </div>
      )}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  if (!value) return null;
  return (
    <div style={{ display: "flex", justifyContent: "space-between", padding: "2px 0", borderBottom: "1px solid rgba(255,255,255,0.04)", fontSize: 10.5 }}>
      <span style={{ color: "var(--text-secondary, #888)" }}>{label}</span>
      <span style={{ color: "var(--text, #eee)", fontFamily: "monospace", textAlign: "right" }}>{value}</span>
    </div>
  );
}

export function DeviceToolsInfo({ serial }: DeviceToolsTabProps) {
  const [info, setInfo] = useState<DeviceFullInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  const load = () => {
    if (!serial) return;
    setLoading(true);
    setError("");
    api.deviceFullInfo(serial)
      .then((data) => { setInfo(data); setError(""); })
      .catch((err) => setError(String(err)))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    if (serial) load();
  }, [serial]); // eslint-disable-line react-hooks/exhaustive-deps

  if (!serial) return <div style={{ padding: 20, textAlign: "center", color: "var(--text-secondary, #666)", fontSize: 11 }}>Connect a device to view details</div>;

  return (
    <div style={{ padding: "10px 12px" }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 10 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontSize: 20 }}>📱</span>
          <div>
            <div style={{ fontSize: 13, fontWeight: 600, color: "var(--text, #eee)" }}>
              {info?.manufacturer ? `${info.manufacturer} ${info.model}` : "Device Details"}
            </div>
            {info && (
              <div style={{ fontSize: 10, color: "var(--text-secondary, #888)" }}>
                {info.android_version ? `Android ${info.android_version}` : ""}
                {info.sdk_level ? ` (API ${info.sdk_level})` : ""}
                {info.build_type ? ` · ${info.build_type}` : ""}
              </div>
            )}
          </div>
        </div>
        <button onClick={load} disabled={loading} style={{ padding: "4px 10px", fontSize: 10.5, borderRadius: 4, border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: "pointer" }}>
          {loading ? "⟳ Loading…" : "⟳ Refresh"}
        </button>
      </div>
      {error && <div style={{ padding: "6px 8px", marginBottom: 8, background: "rgba(239,68,68,0.08)", borderRadius: 4, color: "#f87171", fontSize: 10 }}>⚠ {error}</div>}
      {info && (
        <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
          <Section icon="🏷" title="Identity">
            <Row label="Manufacturer" value={info.manufacturer} />
            <Row label="Brand" value={info.brand} />
            <Row label="Model" value={info.model} />
            <Row label="Device Name" value={info.device_name} />
            <Row label="Board" value={info.board} />
            <Row label="Hardware" value={info.hardware} />
            <Row label="Serial Number" value={info.serial_no} />
            <Row label="Bootloader" value={info.bootloader} />
            <Row label="Baseband" value={info.baseband} />
          </Section>
          <Section icon="🤖" title="Operating System">
            <Row label="Android Version" value={info.android_version} />
            <Row label="SDK Level" value={info.sdk_level} />
            <Row label="Security Patch" value={info.security_patch} />
            <Row label="Build ID" value={info.build_id} />
            <Row label="Build Type" value={info.build_type} />
            <Row label="Fingerprint" value={info.build_fingerprint} />
            <Row label="Build Date" value={info.build_date} />
          </Section>
          <Section icon="⚡" title="Processor">
            <Row label="Primary ABI" value={info.cpu_abi} />
            <Row label="Supported ABIs" value={info.cpu_abi2} />
            <Row label="CPU Cores" value={info.cpu_cores} />
            <Row label="Max Frequency" value={info.cpu_max_freq} />
          </Section>
          <Section icon="🖥" title="Display">
            <Row label="Resolution" value={info.resolution} />
            <Row label="Density" value={info.density} />
            <Row label="Refresh Rate" value={info.refresh_rate} />
          </Section>
          <Section icon="💾" title="Memory & Storage">
            <Row label="Total RAM" value={info.total_ram} />
            <Row label="Available RAM" value={info.available_ram} />
            <Row label="Internal Storage" value={info.internal_storage} />
            <Row label="SD Card" value={info.sd_card} />
          </Section>
          <Section icon="🔋" title="Battery">
            <Row label="Level" value={info.battery_level} />
            <Row label="Status" value={info.battery_status} />
            <Row label="Temperature" value={info.battery_temp} />
            <Row label="Voltage" value={info.battery_voltage} />
            <Row label="Health" value={info.battery_health} />
            <Row label="Technology" value={info.battery_technology} />
          </Section>
          <Section icon="📡" title="Network" defaultOpen={false}>
            <Row label="WiFi" value={info.wifi_state} />
            <Row label="IP Address" value={info.ip_address} />
            <Row label="MAC Address" value={info.mac_address} />
          </Section>
          <Section icon="📶" title="SIM Card" defaultOpen={false}>
            <Row label="Operator" value={info.sim_operator} />
            <Row label="Carrier" value={info.sim_carrier} />
            <Row label="Country" value={info.sim_country?.toUpperCase()} />
            <Row label="State" value={info.sim_state} />
          </Section>
          <Section icon="🌍" title="System" defaultOpen={false}>
            <Row label="Language" value={info.language} />
            <Row label="Timezone" value={info.timezone} />
            <Row label="Encryption" value={info.encryption} />
            <Row label="Uptime" value={info.uptime} />
          </Section>
          <Section icon="📦" title="Applications" defaultOpen={false}>
            <Row label="Total Packages" value={info.total_apps} />
            <Row label="User Apps" value={info.user_apps} />
          </Section>
          <Section icon="🔌" title="USB & Debug" defaultOpen={false}>
            <Row label="USB State" value={info.usb_state} />
            <Row label="ADB Version" value={info.adb_version} />
          </Section>
        </div>
      )}
    </div>
  );
}
