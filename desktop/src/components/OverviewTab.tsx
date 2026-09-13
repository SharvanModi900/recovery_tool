import { useState, useEffect } from "react";
import * as api from "../api";
import type { DeviceFullInfo, DeviceMirror, DirectAcquisition } from "../types";

interface OverviewTabProps {
  serial: string;
  mirror: DeviceMirror | null;
  directData: DirectAcquisition | null;
}

export function OverviewTab({ serial, mirror, directData }: OverviewTabProps) {
  const [fullInfo, setFullInfo] = useState<DeviceFullInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let mounted = true;
    setLoading(true);
    api.deviceFullInfo(serial).then((info) => {
      if (mounted) { setFullInfo(info); setLoading(false); }
    }).catch(() => { if (mounted) setLoading(false); });
    return () => { mounted = false; };
  }, [serial]);

  const batteryLevel = mirror?.battery
    ? parseInt(mirror.battery.replace("%", ""), 10)
    : (fullInfo ? parseInt(fullInfo.battery_level, 10) : 0);

  const parseStorageString = (s: string) => {
    const match = s.match(/([\d.]+)\s*(\w+)?\s*free\s*\/\s*([\d.]+)\s*(\w+)?/);
    if (match) return { free: `${parseFloat(match[1])} ${match[2] || ""}`.trim(), total: `${parseFloat(match[3])} ${match[4] || ""}`.trim() };
    return null;
  };

  const mirrorStorage = mirror?.storage ? parseStorageString(mirror.storage) : null;
  const storageFree = mirrorStorage?.free || (fullInfo?.internal_storage ? fullInfo.internal_storage.split(" free")[0] : "—");
  const storageTotal = mirrorStorage?.total || (fullInfo?.internal_storage ? fullInfo.internal_storage.split(" / ")[1] || "—" : "—");
  const ramFree = fullInfo?.available_ram || "—";
  const ramTotal = fullInfo?.total_ram || "—";
  const batteryTemp = fullInfo?.battery_temp || "—";
  const batteryStatus = fullInfo?.battery_status || "Unknown";
  const batteryHealth = fullInfo?.battery_health || "Unknown";
  const cpuMaxFreq = fullInfo?.cpu_max_freq || "—";
  const refreshRate = fullInfo?.refresh_rate || "—";

  const clean = (v: string | undefined) => {
    if (!v || v.includes("Permission denied") || v.includes("error") || v === "NaN") return "—";
    return v;
  };

  const batteryPct = isNaN(batteryLevel) ? 0 : batteryLevel;
  const batteryClass = batteryPct > 50 ? "ok" : batteryPct > 20 ? "warn" : "hot";

  if (loading) {
    return (
      <div className="ov-loading">
        <span className="spin" />
        <span>Loading device information…</span>
      </div>
    );
  }

  return (
    <div className="ov-scroll">
      <div className="ov-grid">

        {/* ── Device Identity Card ── */}
        <div className="ov-identity">
          <div className="ov-identity-left">
            <h2 className="ov-model">{clean(fullInfo?.manufacturer) !== "—" ? fullInfo?.manufacturer : "Unknown"} {clean(fullInfo?.model) !== "—" ? fullInfo?.model : "Device"}</h2>
            <div className="ov-brand">{clean(fullInfo?.brand)} · {clean(fullInfo?.device_name)}</div>
            <div className="ov-tags">
              <span className="ov-tag"><em>Android</em> {clean(fullInfo?.android_version)}</span>
              <span className="ov-tag"><em>SDK</em> {clean(fullInfo?.sdk_level)}</span>
              <span className="ov-tag"><em>Security</em> {clean(fullInfo?.security_patch)}</span>
            </div>
          </div>
          <div className="ov-serial">
            <small>Serial</small>
            <code>{serial}</code>
          </div>
        </div>

        {/* ── Live Status Cards ── */}
        <div className="ov-stats">
          <div className="ov-stat-card">
            <div className="ov-stat-head"><span>Battery</span><strong className={batteryClass}>{batteryPct}%</strong></div>
            <div className="ov-stat-body">
              <div>{clean(batteryStatus)}</div>
              <div className="muted">{clean(batteryTemp)} · {clean(batteryHealth)}</div>
            </div>
          </div>
          <div className="ov-stat-card">
            <div className="ov-stat-head"><span>Storage</span></div>
            <div className="ov-stat-body accent-blue">
              <div>{storageFree} free</div>
              <div className="muted">of {storageTotal}</div>
            </div>
          </div>
          <div className="ov-stat-card">
            <div className="ov-stat-head"><span>Memory</span></div>
            <div className="ov-stat-body accent-purple">
              <div>{ramFree} free</div>
              <div className="muted">of {ramTotal}</div>
            </div>
          </div>
          <div className="ov-stat-card">
            <div className="ov-stat-head"><span>Network</span></div>
            <div className="ov-stat-body accent-cyan">
              <div>{clean(fullInfo?.wifi_state)}</div>
              <div className="muted">{clean(fullInfo?.wifi_ssid) || "No WiFi"}</div>
            </div>
          </div>
        </div>

        {/* ── Detail Sections ── */}
        <div className="ov-details">
          <div className="ov-section">
            <h3>Hardware</h3>
            <div className="ov-rows">
              <OvRow label="CPU" value={`${clean(fullInfo?.cpu_abi)} · ${clean(fullInfo?.cpu_cores)} cores`} />
              <OvRow label="Max Freq" value={clean(cpuMaxFreq)} />
              <OvRow label="Display" value={clean(fullInfo?.resolution)} />
              <OvRow label="Density" value={`${clean(fullInfo?.density)} dpi`} />
              <OvRow label="Refresh" value={clean(refreshRate)} />
              <OvRow label="Board" value={clean(fullInfo?.board)} />
              <OvRow label="Hardware" value={clean(fullInfo?.hardware)} />
            </div>
          </div>

          <div className="ov-section">
            <h3>System</h3>
            <div className="ov-rows">
              <OvRow label="Build ID" value={clean(fullInfo?.build_id)} />
              <OvRow label="Build Type" value={clean(fullInfo?.build_type)} />
              <OvRow label="Bootloader" value={clean(fullInfo?.bootloader)} />
              <OvRow label="Baseband" value={clean(fullInfo?.baseband)} />
              <OvRow label="Language" value={clean(fullInfo?.language)} />
              <OvRow label="Timezone" value={clean(fullInfo?.timezone)} />
              <OvRow label="Encryption" value={clean(fullInfo?.encryption)} />
              <OvRow label="Uptime" value={clean(fullInfo?.uptime)} />
            </div>
          </div>

          <div className="ov-section">
            <h3>SIM & Network</h3>
            <div className="ov-rows">
              <OvRow label="Operator" value={clean(fullInfo?.sim_operator)} />
              <OvRow label="Carrier" value={clean(fullInfo?.sim_carrier)} />
              <OvRow label="Country" value={clean(fullInfo?.sim_country)} />
              <OvRow label="SIM State" value={clean(fullInfo?.sim_state)} />
              <OvRow label="IP Address" value={clean(fullInfo?.ip_address)} />
              <OvRow label="MAC" value={clean(fullInfo?.mac_address)} />
            </div>
          </div>

          <div className="ov-section">
            <h3>USB & Debug</h3>
            <div className="ov-rows">
              <OvRow label="USB State" value={clean(fullInfo?.usb_state)} />
              <OvRow label="ADB Version" value={clean(fullInfo?.adb_version)} />
              <OvRow label="Apps" value={`${clean(fullInfo?.user_apps)} user / ${clean(fullInfo?.total_apps)} total`} />
            </div>
          </div>
        </div>

        {/* ── Acquisition Summary ── */}
        {directData && (
          <div className="ov-section ov-full">
            <h3>Acquisition Summary</h3>
            <div className="ov-acq-grid">
              <AcqStat label="Call Logs" value={directData.call_logs?.length ?? 0} />
              <AcqStat label="SMS" value={directData.sms_messages?.length ?? 0} />
              <AcqStat label="Contacts" value={directData.contacts?.length ?? 0} />
              <AcqStat label="Recordings" value={directData.call_recordings?.length ?? 0} />
              <AcqStat label="WA Media" value={directData.whatsapp_media?.length ?? 0} />
              <AcqStat label="WA Chats" value={directData.whatsapp_chats?.length ?? 0} />
              <AcqStat label="WA Calls" value={directData.whatsapp_calls?.length ?? 0} />
              <AcqStat label="Calendar" value={directData.calendar_events?.length ?? 0} />
            </div>
          </div>
        )}

      </div>
    </div>
  );
}

function OvRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="ov-row">
      <span className="ov-row-label">{label}</span>
      <span className="ov-row-value">{value}</span>
    </div>
  );
}

function AcqStat({ label, value }: { label: string; value: number }) {
  return (
    <div className="ov-acq-stat">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}
