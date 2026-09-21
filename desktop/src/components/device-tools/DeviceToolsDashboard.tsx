import { useState } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, OUT, toolBtn, inputStyle, miniBtn } from "./shared";

export function DeviceToolsDashboard({ serial, results, busy, run }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const [pullPkg, setPullPkg] = useState("com.whatsapp");
  const [searchPath, setSearchPath] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [exportData, setExportData] = useState("");
  const [exportName, setExportName] = useState("export.json");

  const quickTools = [
    { id: "clipboard", icon: "📋", label: "Clipboard", fn: () => api.getClipboard(serial!) },
    { id: "notifications", icon: "🔔", label: "Notifications", fn: () => api.getNotifications(serial!) },
    { id: "wifi", icon: "📶", label: "WiFi Creds", fn: () => api.getWifiCredentials(serial!) },
    { id: "battery", icon: "🔋", label: "Battery Stats", fn: () => api.getBatteryStats(serial!) },
    { id: "backup", icon: "💾", label: "Full Backup", fn: () => api.createDeviceBackup(serial!, OUT) },
    { id: "fullextract", icon: "🚀", label: "1-Click Extract", fn: () => api.fullDeviceExtract(serial!, OUT) },
    { id: "thumbnails", icon: "🖼", label: "Recover Thumbs", fn: () => api.scanThumbnails(serial!, OUT) },
  ];

  return (
    <div style={{ padding: "10px 12px" }}>
      <div style={{ fontSize: 11, color: "var(--text-secondary, #888)", marginBottom: 10 }}>
        Quick device tools — run commands and collect data from the connected device.
      </div>
      <div style={{ display: "flex", gap: 4, flexWrap: "wrap", marginBottom: 12 }}>
        {quickTools.map(t => (
          <button key={t.id} disabled={!serial || isBusy(t.id)} onClick={() => run(t.id, t.fn)} title={t.label}
            style={toolBtn(isBusy(t.id), !serial)}>
            {isBusy(t.id) ? "⏳" : t.icon} {t.label}
          </button>
        ))}
      </div>
      {/* Input tools row */}
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 12, alignItems: "center" }}>
        <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
          <input value={pullPkg} onChange={e => setPullPkg(e.target.value)} placeholder="package" style={inputStyle(140)} />
          <button disabled={!serial || isBusy("pull")} onClick={() => run("pull", () => api.pullAppData(serial!, pullPkg, OUT))} style={miniBtn}>
            {isBusy("pull") ? "⏳" : "📦"} Pull
          </button>
          <button disabled={!serial || isBusy("apk")} onClick={() => run("apk", () => api.extractApk(serial!, pullPkg, OUT))} style={miniBtn}>
            {isBusy("apk") ? "⏳" : "📱"} APK
          </button>
        </div>
        <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
          <input value={searchPath} onChange={e => setSearchPath(e.target.value)} placeholder="DB path" style={inputStyle(120)} />
          <input value={searchQuery} onChange={e => setSearchQuery(e.target.value)} placeholder="search" style={inputStyle(90)} />
          <button disabled={isBusy("dbsearch") || !searchPath || !searchQuery} onClick={() => run("dbsearch", () => api.searchDatabase(searchPath, searchQuery))} style={miniBtn}>
            {isBusy("dbsearch") ? "⏳" : "🔍"} Search
          </button>
        </div>
        <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
          <input value={exportName} onChange={e => setExportName(e.target.value)} placeholder="file.json" style={inputStyle(90)} />
          <button disabled={isBusy("export") || !exportData.trim()} onClick={() => run("export", () => api.exportDataJson(exportData, exportName, OUT))} style={miniBtn}>
            {isBusy("export") ? "⏳" : "📤"} Export
          </button>
        </div>
      </div>
      <div style={{ marginBottom: 12 }}>
        <textarea value={exportData} onChange={e => setExportData(e.target.value)} placeholder="Paste data to export..." rows={2} style={{ ...inputStyle("100%" as any), fontFamily: "monospace", resize: "vertical", fontSize: 10 }} />
      </div>
      {/* Recent results feed */}
      {results.length > 0 && (
        <div>
          <div style={{ fontSize: 10, fontWeight: 600, color: "var(--text-secondary, #888)", marginBottom: 6, textTransform: "uppercase", letterSpacing: 0.5 }}>
            Recent Results
          </div>
          {results.slice(0, 5).map((r, i) => (
            <div key={r.ts + i} style={{ padding: "4px 6px", marginBottom: 3, borderRadius: 4, fontSize: 10, background: r.ok ? "rgba(34,197,94,0.06)" : "rgba(239,68,68,0.06)", border: `1px solid ${r.ok ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)"}` }}>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span style={{ fontWeight: 600, color: r.ok ? "var(--ok, #22c55e)" : "var(--err, #ef4444)", fontSize: 10 }}>{r.ok ? "✅" : "❌"} {r.id}</span>
                <span style={{ fontSize: 8, color: "var(--text-secondary, #555)" }}>{new Date(r.ts).toLocaleTimeString()}</span>
              </div>
              <pre style={{ margin: "2px 0 0", whiteSpace: "pre-wrap", wordBreak: "break-all", fontFamily: "monospace", fontSize: 9.5, color: "var(--text, #bbb)", maxHeight: 60, overflow: "auto" }}>{r.text}</pre>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
