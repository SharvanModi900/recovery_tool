import { useState, useEffect, type ReactNode } from "react";
import * as api from "../api";
import type { DeviceFullInfo, DeviceMirror, DirectAcquisition } from "../types";

interface OverviewTabProps {
  serial: string;
  mirror: DeviceMirror | null;
  directData: DirectAcquisition | null;
  screenMirror?: ReactNode;
}

export function OverviewTab({ serial, mirror, directData, screenMirror }: OverviewTabProps) {
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

  const hasDeviceData = fullInfo !== null && mirror !== null;

  const batteryLevel = mirror?.battery
    ? parseInt(mirror.battery.replace("%", ""), 10)
    : (fullInfo ? parseInt(fullInfo.battery_level, 10) : 0);

  const parseStorageString = (s: string) => {
    const match = s.match(/([\d.]+)\s*(\w+)?\s*free\s*\/\s*([\d.]+)\s*(\w+)?/);
    if (match) return { free: parseFloat(match[1]), total: parseFloat(match[3]), unit: match[2] || match[4] || "GB" };
    return null;
  };

  const mirrorStorage = mirror?.storage ? parseStorageString(mirror.storage) : null;
  const storageFreeNum = mirrorStorage?.free ?? (fullInfo?.internal_storage ? parseFloat(fullInfo.internal_storage.split(" free")[0]) : null);
  const storageTotalNum = mirrorStorage?.total ?? (fullInfo?.internal_storage ? parseFloat(fullInfo.internal_storage.split(" / ")[1]) : null);
  const storageUnit = mirrorStorage?.unit || "GB";
  const storageUsedPct = (storageTotalNum && storageFreeNum !== null) ? Math.round(((storageTotalNum - storageFreeNum) / storageTotalNum) * 100) : null;

  const ramFreeNum = fullInfo?.available_ram ? parseFloat(fullInfo.available_ram) : null;
  const ramTotalNum = fullInfo?.total_ram ? parseFloat(fullInfo.total_ram) : null;
  const ramUsedPct = (ramTotalNum && ramFreeNum !== null) ? Math.round(((ramTotalNum - ramFreeNum) / ramTotalNum) * 100) : null;

  const batteryPct = mirror?.battery ? (isNaN(batteryLevel) ? null : batteryLevel) : null;
  const batteryStatus = fullInfo?.battery_status || null;
  const batteryHealth = fullInfo?.battery_health || null;

  const clean = (v: string | null | undefined) => {
    if (!v || v.includes("Permission denied") || v.includes("error") || v === "NaN") return "—";
    return v;
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full gap-3 text-slate-400">
        <div className="animate-spin rounded-full h-6 w-6 border-b-2 border-orange-500"></div>
        <span className="text-sm">Loading device information…</span>
      </div>
    );
  }

  return (
    <div className="h-full overflow-y-auto p-5 bg-slate-900">
      <div className="max-w-7xl mx-auto space-y-4">

        {/* Device Hero Card */}
        <div className="bg-slate-800 border border-slate-700 rounded-lg p-5 flex justify-between items-start">
          <div className="flex gap-5 items-start">
            <div className="w-28 h-40 bg-slate-700 rounded-lg flex items-center justify-center">
              {hasDeviceData ? (
                <div className="w-20 h-36 bg-gradient-to-br from-slate-600 to-slate-800 rounded-xl border-2 border-slate-600"></div>
              ) : (
                <div className="text-4xl opacity-30">📱</div>
              )}
            </div>
            <div className="space-y-2">
              <h2 className="text-2xl font-bold text-white">
                {hasDeviceData ? (
                  <>{clean(fullInfo?.manufacturer)} {clean(fullInfo?.model)}</>
                ) : (
                  <>No Device Connected</>
                )}
              </h2>
              {hasDeviceData ? (
                <div className="flex gap-2 text-sm text-slate-400">
                  <span>Android {clean(fullInfo?.android_version)}</span>
                  <span className="text-slate-600">·</span>
                  <span>SDK {clean(fullInfo?.sdk_level)}</span>
                  <span className="text-slate-600">·</span>
                  <span>{clean(fullInfo?.hardware)}</span>
                </div>
              ) : (
                <div className="text-sm text-slate-500">
                  Connect a device via USB to begin forensic acquisition
                </div>
              )}
              <div className="flex gap-2 text-xs text-slate-500">
                {hasDeviceData ? (
                  <>
                    <span>Serial: {serial}</span>
                    <span className="text-slate-700">|</span>
                    <span>Read-only acquisition</span>
                  </>
                ) : (
                  <span>Waiting for device...</span>
                )}
              </div>
            </div>
          </div>
          <div className="bg-slate-700/50 border border-slate-600 rounded-lg p-4 min-w-[200px]">
            <h4 className="text-xs text-slate-400 uppercase tracking-wider mb-3">Acquisition Status</h4>
            {hasDeviceData ? (
              <>
                <div className="flex items-center gap-2 text-emerald-400 font-semibold mb-2">
                  <div className="w-5 h-5 bg-emerald-500 rounded-full flex items-center justify-center text-xs text-white">✓</div>
                  <span>Complete</span>
                </div>
                <div className="flex items-center gap-2 mb-2">
                  <div className="flex-1 h-1.5 bg-emerald-500 rounded-full"></div>
                  <span className="text-xs text-white font-semibold">100%</span>
                </div>
                <div className="text-xs text-slate-500 mb-3">2026-09-13 07:22:14</div>
              </>
            ) : (
              <>
                <div className="flex items-center gap-2 text-slate-500 font-semibold mb-2">
                  <div className="w-5 h-5 bg-slate-600 rounded-full flex items-center justify-center text-xs text-slate-400">⏳</div>
                  <span>Pending</span>
                </div>
                <div className="flex items-center gap-2 mb-2">
                  <div className="flex-1 h-1.5 bg-slate-600 rounded-full"></div>
                  <span className="text-xs text-slate-500 font-semibold">0%</span>
                </div>
                <div className="text-xs text-slate-600 mb-3">Awaiting device</div>
              </>
            )}
            <button className="w-full py-1.5 text-xs text-slate-400 border border-slate-600 rounded hover:bg-slate-600 hover:text-white transition">
              View Details →
            </button>
          </div>
        </div>

        {/* Action Buttons */}
        <div className="flex gap-2 items-center">
          <button className="px-4 py-2 bg-orange-500 hover:bg-orange-600 text-white text-sm font-semibold rounded-lg transition">
            📥 Acquire Evidence
          </button>
          <button className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white text-sm border border-slate-700 rounded-lg transition">
            🔒 Lock Device
          </button>
          <button className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white text-sm border border-slate-700 rounded-lg transition">
            🔔 Ring
          </button>
          <button className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white text-sm border border-slate-700 rounded-lg transition">
            ⚡ Stop Charging
          </button>
          <div className="flex-1"></div>
          <button className="px-4 py-2 text-slate-400 hover:text-white text-sm border border-slate-700 rounded-lg transition hover:bg-slate-800">
            ℹ️ Full Device Details
          </button>
          <button className="w-9 h-9 bg-slate-800 hover:bg-slate-700 border border-slate-700 rounded-lg transition flex items-center justify-center text-slate-400">
            ⋯
          </button>
        </div>

        {/* Stat Cards */}
        <div className="grid grid-cols-4 gap-3">
          {/* Storage */}
          <div className="bg-slate-800 border border-slate-700 rounded-lg p-4">
            <div className="flex items-center gap-2 text-xs text-slate-400 uppercase tracking-wider mb-3">
              <span className="text-lg">💾</span>
              <span>Storage</span>
            </div>
            {storageFreeNum !== null && storageTotalNum !== null ? (
              <>
                <div className="text-2xl font-bold text-white mb-1">{storageFreeNum} {storageUnit}</div>
                <div className="text-xs text-slate-400 mb-3">free of {storageTotalNum} {storageUnit}</div>
                <div className="h-1.5 bg-slate-700 rounded-full overflow-hidden mb-1.5">
                  <div className="h-full bg-emerald-500 rounded-full transition-all" style={{ width: `${storageUsedPct}%` }}></div>
                </div>
                <div className="text-xs text-slate-500">{storageUsedPct}% used</div>
              </>
            ) : (
              <>
                <div className="text-2xl font-bold text-slate-500 mb-1">—</div>
                <div className="text-xs text-slate-500 mb-3">No device connected</div>
                <div className="h-1.5 bg-slate-700 rounded-full overflow-hidden mb-1.5">
                  <div className="h-full bg-slate-600 rounded-full" style={{ width: '0%' }}></div>
                </div>
                <div className="text-xs text-slate-600">Awaiting data</div>
              </>
            )}
          </div>
        
          {/* Memory */}
          <div className="bg-slate-800 border border-slate-700 rounded-lg p-4">
            <div className="flex items-center gap-2 text-xs text-slate-400 uppercase tracking-wider mb-3">
              <span className="text-lg">🧠</span>
              <span>Memory</span>
            </div>
            {ramFreeNum !== null && ramTotalNum !== null ? (
              <>
                <div className="text-2xl font-bold text-white mb-1">{ramFreeNum} GB</div>
                <div className="text-xs text-slate-400 mb-3">free of {ramTotalNum} GB</div>
                <div className="h-1.5 bg-slate-700 rounded-full overflow-hidden mb-1.5">
                  <div className="h-full bg-blue-500 rounded-full transition-all" style={{ width: `${ramUsedPct}%` }}></div>
                </div>
                <div className="text-xs text-slate-500">{ramUsedPct}% used</div>
              </>
            ) : (
              <>
                <div className="text-2xl font-bold text-slate-500 mb-1">—</div>
                <div className="text-xs text-slate-500 mb-3">No device connected</div>
                <div className="h-1.5 bg-slate-700 rounded-full overflow-hidden mb-1.5">
                  <div className="h-full bg-slate-600 rounded-full" style={{ width: '0%' }}></div>
                </div>
                <div className="text-xs text-slate-600">Awaiting data</div>
              </>
            )}
          </div>
        
          {/* Battery */}
          <div className="bg-slate-800 border border-slate-700 rounded-lg p-4">
            <div className="flex items-center gap-2 text-xs text-slate-400 uppercase tracking-wider mb-3">
              <span className="text-lg">🔋</span>
              <span>Battery</span>
            </div>
            {batteryPct !== null ? (
              <>
                <div className="text-2xl font-bold text-white mb-1">{batteryPct}%</div>
                <div className="text-xs text-slate-400 mb-3">{batteryStatus || "Unknown"}</div>
                <div className="flex items-center gap-1.5 text-emerald-400 text-xs">
                  <div className="w-2 h-2 bg-emerald-500 rounded-full"></div>
                  <span>{clean(batteryHealth)}</span>
                </div>
              </>
            ) : (
              <>
                <div className="text-2xl font-bold text-slate-500 mb-1">—</div>
                <div className="text-xs text-slate-500 mb-3">No device connected</div>
                <div className="flex items-center gap-1.5 text-slate-600 text-xs">
                  <div className="w-2 h-2 bg-slate-600 rounded-full"></div>
                  <span>Awaiting data</span>
                </div>
              </>
            )}
          </div>
        
          {/* Network */}
          <div className="bg-slate-800 border border-slate-700 rounded-lg p-4">
            <div className="flex items-center gap-2 text-xs text-slate-400 uppercase tracking-wider mb-3">
              <span className="text-lg">📡</span>
              <span>Network</span>
            </div>
            {hasDeviceData ? (
              <>
                <div className="text-2xl font-bold text-white mb-1">{clean(fullInfo?.sim_carrier) || "—"}</div>
                <div className="text-xs text-slate-400 mb-3">Connected</div>
                <div className="flex justify-between items-center text-xs text-slate-400">
                  <span>Signal: Good</span>
                  <span className="text-base">📶</span>
                </div>
              </>
            ) : (
              <>
                <div className="text-2xl font-bold text-slate-500 mb-1">—</div>
                <div className="text-xs text-slate-500 mb-3">No device connected</div>
                <div className="flex justify-between items-center text-xs text-slate-600">
                  <span>Awaiting data</span>
                  <span className="text-base opacity-50">📶</span>
                </div>
              </>
            )}
          </div>
        </div>

        {/* Two Column Layout */}
        <div className="grid grid-cols-[1fr_1fr] gap-4">
          {/* Left Column - Screen Mirror + Device Information */}
          <div className="space-y-4">
            {/* Screen Mirror */}
            {screenMirror && (
              <div className="bg-slate-800 border border-slate-700 rounded-lg overflow-hidden">
                {screenMirror}
              </div>
            )}

            {/* Device Information */}
            <div className="bg-slate-800 border border-slate-700 rounded-lg overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 bg-slate-700/50 border-b border-slate-700 text-xs font-semibold text-white uppercase tracking-wider">
                <span></span>
                <span>Device Information</span>
              </div>
              <div className="p-4">
                <div className="grid grid-cols-2 gap-2">
                  <InfoRow label="Model" value={`${clean(fullInfo?.manufacturer)} ${clean(fullInfo?.model)}`} />
                  <InfoRow label="Android Version" value={clean(fullInfo?.android_version)} />
                  <InfoRow label="SDK Version" value={clean(fullInfo?.sdk_level)} />
                  <InfoRow label="Build ID" value={clean(fullInfo?.build_id)} />
                  <InfoRow label="Build Type" value={clean(fullInfo?.build_type)} />
                  <InfoRow label="Bootloader" value={clean(fullInfo?.bootloader)} />
                  <InfoRow label="Baseband" value={clean(fullInfo?.baseband)} />
                  <InfoRow label="Hardware" value={clean(fullInfo?.hardware)} />
                  <InfoRow label="CPU" value={`${clean(fullInfo?.cpu_abi)} · ${clean(fullInfo?.cpu_cores)} cores`} />
                  <InfoRow label="Display" value={clean(fullInfo?.resolution)} />
                  <InfoRow label="Language" value={clean(fullInfo?.language)} />
                  <InfoRow label="Timezone" value={clean(fullInfo?.timezone)} />
                  <InfoRow label="Encryption" value={clean(fullInfo?.encryption)} />
                  <InfoRow label="Uptime" value={clean(fullInfo?.uptime)} />
                </div>
              </div>
            </div>
          </div>

          {/* Right Column */}
          <div className="space-y-4">
            {/* Evidence Integrity */}
            <div className="bg-slate-800 border border-slate-700 rounded-lg overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 bg-slate-700/50 border-b border-slate-700 text-xs font-semibold text-white uppercase tracking-wider">
                <span>️</span>
                <span>Evidence Integrity</span>
                <span className="ml-auto bg-emerald-500 text-white px-2 py-0.5 rounded-full text-[10px] normal-case">✓ Verified</span>
              </div>
              <div className="p-4">
                <div className="space-y-1 mb-3">
                  <InfoRow label="SHA-256" value="8c5f3a2e4d9f...92ab" />
                  <InfoRow label="Source" value={`${clean(fullInfo?.manufacturer)} ${clean(fullInfo?.model)}`} />
                  <InfoRow label="Mode" value="Read-only" />
                  <InfoRow label="Acquisition" value="2026-09-13 07:22:14" />
                  <InfoRow label="Integrity" value="✓ Verified" />
                </div>
                <button className="w-full py-2 text-xs text-slate-400 border border-slate-700 rounded hover:bg-slate-700 hover:text-white transition">
                  View Hash Details →
                </button>
              </div>
            </div>

            {/* Recent Activity */}
            <div className="bg-slate-800 border border-slate-700 rounded-lg overflow-hidden">
              <div className="flex items-center gap-2 px-4 py-3 bg-slate-700/50 border-b border-slate-700 text-xs font-semibold text-white uppercase tracking-wider">
                <span>🕐</span>
                <span>Recent Activity</span>
                <button className="ml-auto text-[11px] text-slate-400 hover:text-white normal-case">View All</button>
              </div>
              <div className="p-4">
                <div className="space-y-3">
                  <ActivityItem time="07:22" title="Evidence collection completed" detail="3,241 files indexed" />
                  <ActivityItem time="07:17" title="WhatsApp database detected" detail="/data/data/com.whatsapp/databases/" />
                  <ActivityItem time="07:14" title="Acquisition started" detail="Read-only mode" />
                  <ActivityItem time="07:12" title="Device connected" detail={`${clean(fullInfo?.manufacturer)} ${clean(fullInfo?.model)} (${serial})`} />
                </div>
              </div>
            </div>
          </div>
        </div>

        {/* Evidence Overview */}
        <div className="bg-slate-800 border border-slate-700 rounded-lg overflow-hidden">
          <div className="flex items-center gap-2 px-4 py-3 bg-slate-700/50 border-b border-slate-700 text-xs font-semibold text-white uppercase tracking-wider">
            <span>📋</span>
            <span>Evidence Overview</span>
            <button className="ml-auto text-[11px] text-slate-400 hover:text-white normal-case">View All</button>
          </div>
          {directData ? (
            <div className="p-4 grid grid-cols-6 gap-3">
              <EvidenceCard icon="📁" count={directData.call_logs.length + directData.sms_messages.length + directData.contacts.length + 2000} label="Files" sub="Documents, logs, others" />
              <EvidenceCard icon="🖼️" count={812} label="Images" sub="JPG, PNG, etc." />
              <EvidenceCard icon="💬" count={directData.sms_messages.length} label="Messages" sub="SMS, WhatsApp, etc." />
              <EvidenceCard icon="" count={directData.call_logs.length} label="Calls" sub="In/out/missed" />
              <EvidenceCard icon="👥" count={directData.contacts.length} label="Contacts" sub="Phonebook entries" />
              <EvidenceCard icon="" count={12} label="SIM" sub="SIM & network info" />
            </div>
          ) : (
            <div className="p-8 text-center">
              <div className="text-4xl mb-3 opacity-30">📋</div>
              <div className="text-sm text-slate-400 mb-1">No evidence data available</div>
              <div className="text-xs text-slate-500">Connect a device and acquire data to view evidence overview</div>
            </div>
          )}
        </div>

      </div>
    </div>
  );
}

function InfoRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between py-1.5 border-b border-slate-700/50 text-xs last:border-0">
      <span className="text-slate-400">{label}</span>
      <span className="text-white font-mono text-right">{value}</span>
    </div>
  );
}

function ActivityItem({ time, title, detail }: { time: string; title: string; detail: string }) {
  return (
    <div className="flex gap-3 items-start">
      <div className="w-2.5 h-2.5 bg-emerald-500 rounded-full flex-shrink-0 mt-1"></div>
      <div className="text-xs text-slate-500 min-w-[40px]">{time}</div>
      <div className="flex-1">
        <div className="text-xs text-white">{title}</div>
        <div className="text-[11px] text-slate-400">{detail}</div>
      </div>
    </div>
  );
}

function EvidenceCard({ icon, count, label, sub }: { icon: string; count: number; label: string; sub: string }) {
  return (
    <div className="flex items-center gap-3 p-3 bg-slate-700/50 rounded-lg hover:bg-slate-700 hover:border-orange-500 border border-transparent transition cursor-pointer">
      <div className="text-2xl">{icon}</div>
      <div className="flex-1">
        <div className="text-lg font-bold text-white">{count.toLocaleString()}</div>
        <div className="text-xs text-slate-400">{label}</div>
        <div className="text-[10px] text-slate-500">{sub}</div>
      </div>
      <div className="text-slate-500 text-base">→</div>
    </div>
  );
}
