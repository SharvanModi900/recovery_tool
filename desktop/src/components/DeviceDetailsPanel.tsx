import { useState, useEffect } from "react";
import * as api from "../api";
import type { DeviceFullInfo } from "../types";

interface DeviceDetailsPanelProps {
  serial: string;
}

interface SectionProps {
  icon: string;
  title: string;
  children: React.ReactNode;
  defaultOpen?: boolean;
}

function Section({ icon, title, children, defaultOpen = true }: SectionProps) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="dfi-section">
      <button className="dfi-section-header" onClick={() => setOpen(!open)}>
        <span>{icon}</span>
        <span>{title}</span>
        <span className="dfi-chevron">{open ? "▾" : "▸"}</span>
      </button>
      {open && <div className="dfi-section-body">{children}</div>}
    </div>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  if (!value) return null;
  return (
    <div className="dfi-row">
      <span className="dfi-label">{label}</span>
      <span className="dfi-value">{value}</span>
    </div>
  );
}

export function DeviceDetailsPanel({ serial }: DeviceDetailsPanelProps) {
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

  if (!serial) return <p className="hint" style={{ textAlign: "center", padding: 20 }}>Connect a device to view details</p>;

  return (
    <div className="dfi-panel">
      {/* Header */}
      <div className="dfi-header">
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 24 }}>📱</span>
            <div>
              <h2 style={{ margin: 0, fontSize: 14, color: "var(--ink)" }}>
                {info?.manufacturer ? `${info.manufacturer} ${info.model}` : "Device Details"}
              </h2>
              {info && (
                <p className="hint" style={{ margin: 0 }}>
                  {info.android_version ? `Android ${info.android_version}` : ""}
                  {info.sdk_level ? ` (API ${info.sdk_level})` : ""}
                  {info.build_type ? ` · ${info.build_type}` : ""}
                </p>
              )}
            </div>
          </div>
        </div>
        <button className="mini" onClick={load} disabled={loading} title="Refresh">
          {loading ? "⟳ Loading…" : "⟳ Refresh"}
        </button>
      </div>

      {error && <p className="hint" style={{ color: "#e74c3c", padding: "8px 12px" }}>⚠ {error}</p>}

      {info && (
        <div className="dfi-body">
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
