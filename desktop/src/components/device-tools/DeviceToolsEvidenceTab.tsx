import { useState } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, OUT, inputStyle, miniBtn } from "./shared";

export function DeviceToolsEvidenceTab({ serial, results, busy, run }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const [hashPath, setHashPath] = useState(OUT);
  const [keyword, setKeyword] = useState("");
  const [searchDir, setSearchDir] = useState(OUT);
  const [logcatLines, setLogcatLines] = useState<string[]>([]);

  return (
    <div>
      {/* Evidence Hashing */}
      <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 6 }}>🔐 Evidence Hashing (SHA-256)</div>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input value={hashPath} onChange={e => setHashPath(e.target.value)} placeholder="File or directory path" style={{ ...inputStyle("100%" as any), flex: 1 }} />
          <button disabled={isBusy("hash1")} onClick={() => run("hash1", () => api.hashEvidence(hashPath))} style={miniBtn}>
            {isBusy("hash1") ? "⏳" : "📄"} Hash File
          </button>
          <button disabled={isBusy("hashdir")} onClick={() => run("hashdir", () => api.hashDirectory(hashPath))} style={miniBtn}>
            {isBusy("hashdir") ? "⏳" : "📁"} Hash Dir
          </button>
        </div>
      </div>
      {/* Keyword Search */}
      <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 6 }}>🔍 Global Keyword Search</div>
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input value={searchDir} onChange={e => setSearchDir(e.target.value)} placeholder="Search directory" style={inputStyle(180)} />
          <input value={keyword} onChange={e => setKeyword(e.target.value)} placeholder="Keyword..." style={inputStyle(140)} />
          <button disabled={isBusy("ksearch") || !keyword.trim()} onClick={() => run("ksearch", () => api.globalKeywordSearch(searchDir, keyword))} style={miniBtn}>
            {isBusy("ksearch") ? "⏳" : "🔍"} Search All
          </button>
        </div>
      </div>
      {/* Live Logcat */}
      <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 6 }}>
          <span style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)" }}>📡 Live Logcat</span>
          <div style={{ display: "flex", gap: 4 }}>
            <button disabled={!serial || isBusy("logcat")} onClick={() => { run("logcat", () => api.getLogcatSnapshot(serial!, 200)); }} style={miniBtn}>
              {isBusy("logcat") ? "⏳" : "📸"} Snapshot
            </button>
            <button disabled={!serial} onClick={async () => {
              if (!serial) return;
              try {
                const lines: string[] = [];
                const unsub = (window as any).__TAURI__.event.listen("logcat-line", (e: any) => {
                  lines.push(String(e.payload));
                  setLogcatLines(prev => [...prev, String(e.payload)].slice(-300));
                });
                await api.startLogcat(serial);
                (await unsub)();
              } catch {}
            }} style={miniBtn}>▶ Stream</button>
            <button onClick={() => setLogcatLines([])} style={miniBtn}>🗑 Clear</button>
          </div>
        </div>
        {logcatLines.length > 0 && (
          <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 9, color: "var(--text-secondary, #888)", maxHeight: 120, overflow: "auto", background: "rgba(0,0,0,0.2)", padding: 4, borderRadius: 4 }}>
            {logcatLines.slice(-100).join("\n")}
          </pre>
        )}
      </div>
      {/* Forensic tools row */}
      <div style={{ display: "flex", gap: 4, padding: "8px 12px", flexWrap: "wrap", borderBottom: "1px solid var(--border, #222)" }}>
        <button disabled={!serial || isBusy("timeline")} onClick={() => run("timeline", () => api.getUsageTimeline(serial!))} style={miniBtn}>
          {isBusy("timeline") ? "⏳" : "📊"} Usage Timeline
        </button>
        <button disabled={!serial || isBusy("perms")} onClick={() => run("perms", () => api.auditAppPermissions(serial!))} style={miniBtn}>
          {isBusy("perms") ? "⏳" : "🔑"} Permission Audit
        </button>
        <button disabled={isBusy("report")} onClick={() => run("report", () => api.generateEvidenceReport("RK-2026", serial || "unknown", OUT, OUT))} style={miniBtn}>
          {isBusy("report") ? "⏳" : "📄"} Generate HTML Report
        </button>
      </div>
      {/* Security & Forensics */}
      <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)" }}>
        <div style={{ fontSize: 11, fontWeight: 600, color: "var(--text, #eee)", marginBottom: 8 }}>
          🛡 Security & Forensics
        </div>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 8 }}>
          <button disabled={!serial || isBusy("spyware")} onClick={() => run("spyware", () => api.detectSpyware(serial!))}
            style={{ padding: "6px 14px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy("spyware") ? "var(--err, #ef4444)" : "linear-gradient(135deg, #ef4444, #f97316)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600 }}>
            {isBusy("spyware") ? "⏳ Scanning..." : "🕵️ Spyware Detector"}
          </button>
          <button disabled={!serial || isBusy("cloning")} onClick={() => run("cloning", () => api.detectLinkedDevices(serial!))}
            style={{ padding: "6px 14px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy("cloning") ? "var(--info, #3b82f6)" : "linear-gradient(135deg, #3b82f6, #8b5cf6)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600 }}>
            {isBusy("cloning") ? "⏳ Checking..." : "📱 Device Cloning"}
          </button>
        </div>
        <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
          <button disabled={!serial || isBusy("bt")} onClick={() => run("bt", () => api.getBluetoothForensics(serial!))} style={miniBtn}>
            {isBusy("bt") ? "⏳" : "📶"} Bluetooth History
          </button>
          <button disabled={!serial || isBusy("net")} onClick={() => run("net", () => api.getNetworkConnections(serial!))} style={miniBtn}>
            {isBusy("net") ? "⏳" : "🌐"} Network Map
          </button>
          <button disabled={!serial || isBusy("clip")} onClick={() => run("clip", () => api.getClipboardHistory(serial!, 10))} style={miniBtn}>
            {isBusy("clip") ? "⏳" : "📋"} Clipboard Monitor
          </button>
        </div>
      </div>
      {/* Highlighted results for spyware/cloning */}
      {results.filter(r => r.id === "spyware" && r.ok).length > 0 && (
        <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)", background: "rgba(239,68,68,0.05)" }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: "var(--err, #ef4444)", marginBottom: 4 }}>🕵️ Spyware Scan Results</div>
          <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap", maxHeight: 120, overflow: "auto" }}>
            {results.find(r => r.id === "spyware" && r.ok)?.text}
          </pre>
        </div>
      )}
      {results.filter(r => r.id === "cloning" && r.ok).length > 0 && (
        <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)", background: "rgba(59,130,246,0.05)" }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: "var(--info, #3b82f6)", marginBottom: 4 }}>📱 Linked/Cloned Devices</div>
          <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap", maxHeight: 120, overflow: "auto" }}>
            {results.find(r => r.id === "cloning" && r.ok)?.text}
          </pre>
        </div>
      )}
    </div>
  );
}
