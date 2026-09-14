import { useState, useCallback } from "react";
import * as api from "../api";
import { ImageForensicsPanel } from "./ImageForensicsPanel";
import { ForensicsPanel } from "./ForensicsPanel";

interface ToolsPanelProps { serial: string | null; onClose: () => void; }
type ToolId = string;
interface ToolResult { id: ToolId; ok: boolean; text: string; ts: number; }
interface FileEntry { type: string; size: string; name: string; perms: string; }

const OUT = "D:\\recovery-tool\\workspace";

export function ToolsPanel({ serial, onClose }: ToolsPanelProps) {
  const [tab, setTab] = useState<"tools" | "files" | "processes" | "cooling" | "evidence" | "security" | "display">("tools");
  const [results, setResults] = useState<ToolResult[]>([]);
  const [busy, setBusy] = useState<Set<string>>(new Set());
  // Evidence state
  const [hashPath, setHashPath] = useState(OUT);
  const [keyword, setKeyword] = useState("");
  const [searchDir, setSearchDir] = useState(OUT);
  const [logcatLines, setLogcatLines] = useState<string[]>([]);
  // File explorer state
  const [filePath, setFilePath] = useState("/sdcard");
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [fileBusy, setFileBusy] = useState(false);
  // Pull input
  const [pullPkg, setPullPkg] = useState("com.whatsapp");
  const [searchPath, setSearchPath] = useState("");
  const [searchQuery, setSearchQuery] = useState("");
  const [exportData, setExportData] = useState("");
  const [exportName, setExportName] = useState("export.json");
  // Cooling state
  const [brightness, setBrightness] = useState(100);
  // Display zoom state
  const [zoomLevel, setZoomLevel] = useState(0);
  const [baseDensity, setBaseDensity] = useState(0);
  const [zoomMsg, setZoomMsg] = useState<string | null>(null);
  // Animation state
  const [activeAnim, setActiveAnim] = useState<string | null>(null);
  // Image forensics overlay
  const [showImageForensics, setShowImageForensics] = useState(false);
  // Forensics panel overlay
  const [showForensics, setShowForensics] = useState(false);

  const addResult = useCallback((id: string, ok: boolean, text: string) => {
    setResults(prev => [{ id, ok, text, ts: Date.now() }, ...prev].slice(0, 50));
  }, []);

  const run = useCallback(async (id: string, fn: () => Promise<string>) => {
    if (!serial) return;
    setBusy(prev => new Set(prev).add(id));
    try { addResult(id, true, await fn()); }
    catch (e) { addResult(id, false, typeof e === "string" ? e : (e as Error)?.message ?? String(e)); }
    finally { setBusy(prev => { const n = new Set(prev); n.delete(id); return n; }); }
  }, [serial, addResult]);

  const loadFiles = useCallback(async (path: string) => {
    if (!serial) return;
    setFileBusy(true);
    setFilePath(path);
    try {
      const raw = await api.listDeviceFiles(serial, path);
      const entries: FileEntry[] = raw.split("\n").filter(Boolean).map(line => {
        const [type, size, name, perms] = line.split("|");
        return { type, size, name, perms };
      });
      setFiles(entries);
    } catch { setFiles([]); }
    finally { setFileBusy(false); }
  }, [serial]);

  const isBusy = (id: string) => busy.has(id);
  const btn = (active: boolean) => ({
    padding: "4px 12px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)",
    background: active ? "var(--info, #3b82f6)" : "var(--panel-alt, #1a1a2e)",
    color: "#fff", cursor: "pointer", fontWeight: active ? 600 : 400,
  });

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 999, background: "rgba(0,0,0,0.6)", display: "flex", alignItems: "center", justifyContent: "center" }} onClick={onClose}>
      <div onClick={e => e.stopPropagation()} style={{ background: "var(--bg, #0f0f1a)", border: "1px solid var(--border, #333)", borderRadius: 12, width: 740, maxHeight: "88vh", display: "flex", flexDirection: "column", boxShadow: "0 8px 40px rgba(0,0,0,0.5)" }}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "10px 16px", borderBottom: "1px solid var(--border, #333)" }}>
          <div style={{ display: "flex", gap: 4 }}>
            <button style={btn(tab === "tools")} onClick={() => setTab("tools")}>🛠 Tools</button>
            <button style={btn(tab === "files")} onClick={() => { setTab("files"); if (files.length === 0) loadFiles("/sdcard"); }}>📂 Files</button>
            <button style={btn(tab === "processes")} onClick={() => { setTab("processes"); run("procs", () => api.getRunningProcesses(serial!)); }}>⚙ Processes</button>
            <button style={btn(tab === "cooling")} onClick={() => setTab("cooling")}>❄ Cooling</button>
            <button style={btn(tab === "evidence")} onClick={() => setTab("evidence")}>🔍 Evidence</button>
            <button style={btn(tab === "security")} onClick={() => setTab("security")}>🛡 Security</button>
            <button style={btn(tab === "display")} onClick={() => { setTab("display"); if (!baseDensity && serial) { api.deviceGetDensity(serial).then(info => { const m = info.match(/(\d{2,3})/); if (m) setBaseDensity(parseInt(m[1])); }).catch(() => {}); } }}>📱 Display</button>
          </div>
          <button onClick={onClose} style={{ background: "none", border: "none", color: "var(--text-secondary, #aaa)", cursor: "pointer", fontSize: 16 }}>✕</button>
        </div>

        {/* TAB: Tools */}
        {tab === "tools" && <>
          <div style={{ display: "flex", gap: 4, padding: "8px 12px", flexWrap: "wrap", borderBottom: "1px solid var(--border, #222)" }}>
            {[
              { id: "clipboard", icon: "📋", label: "Clipboard", fn: () => api.getClipboard(serial!) },
              { id: "notifications", icon: "🔔", label: "Notifications", fn: () => api.getNotifications(serial!) },
              { id: "wifi", icon: "📶", label: "WiFi Creds", fn: () => api.getWifiCredentials(serial!) },
              { id: "battery", icon: "🔋", label: "Battery Stats", fn: () => api.getBatteryStats(serial!) },
              { id: "backup", icon: "💾", label: "Full Backup", fn: () => api.createDeviceBackup(serial!, OUT) },
              { id: "fullextract", icon: "🚀", label: "1-Click Extract", fn: () => api.fullDeviceExtract(serial!, OUT) },
              { id: "thumbnails", icon: "🖼", label: "Recover Thumbs", fn: () => api.scanThumbnails(serial!, OUT) },
              { id: "temp", icon: "🌡", label: "Temperature", fn: () => api.getDeviceTemperatures(serial!) },
            ].map(t => (
              <button key={t.id} disabled={!serial || isBusy(t.id)} onClick={() => run(t.id, t.fn)} title={t.label}
                style={{ padding: "4px 10px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy(t.id) ? "var(--info, #3b82f6)" : "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: serial ? "pointer" : "default", opacity: !serial ? 0.4 : 1, whiteSpace: "nowrap" }}>
                {isBusy(t.id) ? "⏳" : t.icon} {t.label}
              </button>
            ))}
            <button onClick={() => setShowImageForensics(true)} title="Image Forensics — ELA manipulation detection"
              style={{ padding: "4px 10px", fontSize: 11, borderRadius: 6, border: "1px solid #a78bfa", background: "linear-gradient(135deg, rgba(167,139,250,0.15), rgba(59,130,246,0.15))", color: "#c4b5fd", cursor: "pointer", whiteSpace: "nowrap", fontWeight: 600 }}>
              📸 Image Forensics
            </button>
            <button onClick={() => setShowForensics(true)} title="Full Forensic Analysis Suite — 10 analysis tools"
              style={{ padding: "4px 10px", fontSize: 11, borderRadius: 6, border: "1px solid #f472b6", background: "linear-gradient(135deg, rgba(244,114,182,0.15), rgba(239,68,68,0.15))", color: "#fda4af", cursor: "pointer", whiteSpace: "nowrap", fontWeight: 600 }}>
              🔬 Forensics Suite
            </button>
          </div>
          {/* Input tools row */}
          <div style={{ display: "flex", gap: 8, padding: "6px 12px", flexWrap: "wrap", borderBottom: "1px solid var(--border, #222)", alignItems: "center" }}>
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
              <button disabled={isBusy("search") || !searchPath || !searchQuery} onClick={() => run("search", () => api.searchDatabase(searchPath, searchQuery))} style={miniBtn}>
                {isBusy("search") ? "⏳" : "🔍"} Search
              </button>
            </div>
            <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
              <input value={exportName} onChange={e => setExportName(e.target.value)} placeholder="file.json" style={inputStyle(90)} />
              <button disabled={isBusy("export") || !exportData.trim()} onClick={() => run("export", () => api.exportDataJson(exportData, exportName, OUT))} style={miniBtn}>
                {isBusy("export") ? "⏳" : "📤"} Export
              </button>
            </div>
          </div>
          <div style={{ padding: "4px 12px", borderBottom: "1px solid var(--border, #222)" }}>
            <textarea value={exportData} onChange={e => setExportData(e.target.value)} placeholder="Paste data to export..." rows={2} style={{ ...inputStyle("100%" as any), fontFamily: "monospace", resize: "vertical", fontSize: 10 }} />
          </div>
        </>}

        {/* TAB: File Explorer */}
        {tab === "files" && <>
          <div style={{ display: "flex", gap: 4, padding: "6px 12px", borderBottom: "1px solid var(--border, #222)", alignItems: "center" }}>
            <button disabled={fileBusy || filePath === "/"} onClick={() => { const p = filePath.split("/").slice(0, -1).join("/") || "/"; loadFiles(p); }} style={miniBtn}>⬆ Up</button>
            <input value={filePath} onChange={e => setFilePath(e.target.value)} onKeyDown={e => e.key === "Enter" && loadFiles(filePath)} style={{ ...inputStyle("100%" as any), flex: 1 }} />
            <button disabled={fileBusy} onClick={() => loadFiles(filePath)} style={miniBtn}>{fileBusy ? "⏳" : "🔄"} Go</button>
            <button disabled={!serial} onClick={() => run("pullfile", () => api.pullDeviceFile(serial!, filePath, OUT))} style={miniBtn}>📥 Pull</button>
          </div>
          <div style={{ flex: 1, overflow: "auto", padding: "4px 12px", minHeight: 200, maxHeight: 400 }}>
            {files.length === 0 ? (
              <div style={{ color: "var(--text-secondary, #666)", fontSize: 11, textAlign: "center", padding: 20 }}>
                {fileBusy ? "Loading..." : "No files found or path doesn't exist"}
              </div>
            ) : (
              <table style={{ width: "100%", fontSize: 10.5, borderCollapse: "collapse" }}>
                <thead><tr style={{ borderBottom: "1px solid var(--border, #333)", color: "var(--text-secondary, #888)" }}>
                  <th style={{ textAlign: "left", padding: "3px 6px" }}>Name</th>
                  <th style={{ textAlign: "right", padding: "3px 6px", width: 60 }}>Size</th>
                  <th style={{ textAlign: "left", padding: "3px 6px", width: 80 }}>Perms</th>
                </tr></thead>
                <tbody>
                  {files.map((f, i) => (
                    <tr key={i} style={{ cursor: f.type === "D" ? "pointer" : "default", background: i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)" }}
                      onClick={() => f.type === "D" && loadFiles(`${filePath}/${f.name}`)}
                      onDoubleClick={() => f.type === "F" && run("pullfile", () => api.pullDeviceFile(serial!, `${filePath}/${f.name}`, OUT))}>
                      <td style={{ padding: "2px 6px", fontFamily: "monospace" }}>
                        <span style={{ marginRight: 4 }}>{f.type === "D" ? "📁" : "📄"}</span>{f.name}
                      </td>
                      <td style={{ padding: "2px 6px", textAlign: "right", fontFamily: "monospace", color: "var(--text-secondary, #888)" }}>
                        {f.type === "F" ? formatSize(parseInt(f.size)) : "—"}
                      </td>
                      <td style={{ padding: "2px 6px", fontFamily: "monospace", fontSize: 9, color: "var(--text-secondary, #666)" }}>{f.perms}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </>}

        {/* TAB: Processes */}
        {tab === "processes" && <>
          <div style={{ padding: "6px 12px", borderBottom: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
            <span style={{ fontSize: 11, color: "var(--text-secondary, #888)" }}>{files.length} processes</span>
            <button disabled={!serial || isBusy("procs")} onClick={() => run("procs", () => api.getRunningProcesses(serial!))} style={miniBtn}>
              {isBusy("procs") ? "⏳" : "🔄"} Refresh
            </button>
          </div>
          <div style={{ flex: 1, overflow: "auto", padding: "4px 12px", minHeight: 200, maxHeight: 400 }}>
            {results.filter(r => r.id === "procs" && r.ok).length > 0 ? (
              <table style={{ width: "100%", fontSize: 10.5, borderCollapse: "collapse" }}>
                <thead><tr style={{ borderBottom: "1px solid var(--border, #333)", color: "var(--text-secondary, #888)" }}>
                  <th style={{ textAlign: "left", padding: "3px 6px", width: 60 }}>PID</th>
                  <th style={{ textAlign: "left", padding: "3px 6px" }}>Process</th>
                </tr></thead>
                <tbody>
                  {results.find(r => r.id === "procs" && r.ok)?.text.split("\n").filter(Boolean).map((line, i) => {
                    const [pid, name] = line.split("|");
                    return (
                      <tr key={i} style={{ background: i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)" }}>
                        <td style={{ padding: "2px 6px", fontFamily: "monospace", color: "var(--info, #3b82f6)" }}>{pid}</td>
                        <td style={{ padding: "2px 6px", fontFamily: "monospace" }}>{name}</td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            ) : (
              <div style={{ color: "var(--text-secondary, #666)", fontSize: 11, textAlign: "center", padding: 20 }}>
                {isBusy("procs") ? "Loading processes..." : "Click Refresh to load"}
              </div>
            )}
          </div>
        </>}

        {/* TAB: Cooling */}
        {tab === "cooling" && <>
          <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
            <div style={{ fontSize: 11, color: "var(--text-secondary, #888)", marginBottom: 8 }}>
              ❄ Device Cooling Controls — reduce temperature by lowering power draw
            </div>
            {/* One-click cooling */}
            <button disabled={!serial || isBusy("cool")} onClick={() => run("cool", () => api.applyCoolingMode(serial!))}
              style={{ padding: "6px 16px", fontSize: 12, borderRadius: 6, border: "1px solid var(--border, #444)", background: "linear-gradient(135deg, #3b82f6, #06b6d4)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600, marginBottom: 10 }}>
              {isBusy("cool") ? "⏳ Applying..." : "❄ Apply Full Cooling Mode"}
            </button>
            {/* Individual controls */}
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
            {results.filter(r => r.id === "temp" && r.ok).length > 0 && (
              <pre style={{ margin: "4px 0 0", fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap" }}>
                {results.find(r => r.id === "temp" && r.ok)?.text}
              </pre>
            )}
          </div>
        </>}

        {/* TAB: Evidence */}
        {tab === "evidence" && <>
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
              <button disabled={isBusy("search") || !keyword.trim()} onClick={() => run("search", () => api.globalKeywordSearch(searchDir, keyword))} style={miniBtn}>
                {isBusy("search") ? "⏳" : "🔍"} Search All
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
          {/* Timeline + Report + Permissions */}
          <div style={{ display: "flex", gap: 4, padding: "8px 12px", flexWrap: "wrap" }}>
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
        </>}

        {/* TAB: Security */}
        {tab === "security" && <>
          <div style={{ padding: "10px 12px", borderBottom: "1px solid var(--border, #222)" }}>
            <div style={{ fontSize: 11, color: "var(--text-secondary, #888)", marginBottom: 8 }}>
              🛡 Security & Forensics — detect threats, linked devices, network activity
            </div>
            {/* Quick scan buttons */}
            <div style={{ display: "flex", gap: 6, flexWrap: "wrap", marginBottom: 10 }}>
              <button disabled={!serial || isBusy("spyware")} onClick={() => run("spyware", () => api.detectSpyware(serial!))}
                style={{ padding: "6px 14px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy("spyware") ? "var(--err, #ef4444)" : "linear-gradient(135deg, #ef4444, #f97316)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600 }}>
                {isBusy("spyware") ? "⏳ Scanning..." : "🕵️ Spyware Detector"}
              </button>
              <button disabled={!serial || isBusy("cloning")} onClick={() => run("cloning", () => api.detectLinkedDevices(serial!))}
                style={{ padding: "6px 14px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy("cloning") ? "var(--info, #3b82f6)" : "linear-gradient(135deg, #3b82f6, #8b5cf6)", color: "#fff", cursor: serial ? "pointer" : "default", fontWeight: 600 }}>
                {isBusy("cloning") ? "⏳ Checking..." : "📱 Device Cloning"}
              </button>
            </div>
            {/* Additional security tools */}
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
              <button disabled={!serial || isBusy("perms2")} onClick={() => run("perms2", () => api.auditAppPermissions(serial!))} style={miniBtn}>
                {isBusy("perms2") ? "⏳" : "🔑"} Permission Audit
              </button>
            </div>
          </div>
          {/* Spyware results highlight */}
          {results.filter(r => r.id === "spyware" && r.ok).length > 0 && (
            <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)", background: "rgba(239,68,68,0.05)" }}>
              <div style={{ fontSize: 11, fontWeight: 600, color: "var(--err, #ef4444)", marginBottom: 4 }}>🕵️ Spyware Scan Results</div>
              <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap", maxHeight: 120, overflow: "auto" }}>
                {results.find(r => r.id === "spyware" && r.ok)?.text}
              </pre>
            </div>
          )}
          {/* Cloning results highlight */}
          {results.filter(r => r.id === "cloning" && r.ok).length > 0 && (
            <div style={{ padding: "8px 12px", borderBottom: "1px solid var(--border, #222)", background: "rgba(59,130,246,0.05)" }}>
              <div style={{ fontSize: 11, fontWeight: 600, color: "var(--info, #3b82f6)", marginBottom: 4 }}>📱 Linked/Cloned Devices</div>
              <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 10, color: "var(--text, #bbb)", whiteSpace: "pre-wrap", maxHeight: 120, overflow: "auto" }}>
                {results.find(r => r.id === "cloning" && r.ok)?.text}
              </pre>
            </div>
          )}
        </>}

        {/* TAB: Display — Zoom + Rotation + Fun Animations */}
        {tab === "display" && <>
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
                  <button
                    key={p.level}
                    disabled={!serial || isBusy("zoom")}
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
                    }}
                  >
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
                <button
                  key={p.rot}
                  disabled={!serial || isBusy("rotate")}
                  onClick={() => run("rotate", () => api.deviceRotate(serial!, p.rot))}
                  style={miniBtn}
                >
                  {isBusy("rotate") ? "⏳" : p.label}
                </button>
              ))}
              <button
                disabled={!serial || isBusy("auto")}
                onClick={() => run("auto", () => api.deviceAutoRotate(serial!))}
                style={{ ...miniBtn, borderColor: "rgba(34,197,94,0.3)", color: "#86efac" }}
              >
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
                <button
                  key={fx.id}
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
                  }}
                >
                  {fx.label}
                </button>
              ))}
            </div>
          </div>
        </>}

        {/* Results panel (shared) */}
        <div style={{ flex: 1, overflow: "auto", padding: "6px 12px", minHeight: 80, maxHeight: 180, borderTop: "1px solid var(--border, #222)" }}>
          {results.filter(r => r.id !== "procs").length === 0 ? (
            <div style={{ color: "var(--text-secondary, #555)", fontSize: 10, textAlign: "center", padding: 8 }}>
              Tool results appear here
            </div>
          ) : results.filter(r => r.id !== "procs").map((r, i) => (
            <div key={r.ts + i} style={{ padding: "4px 6px", marginBottom: 3, borderRadius: 4, fontSize: 10, background: r.ok ? "rgba(34,197,94,0.06)" : "rgba(239,68,68,0.06)", border: `1px solid ${r.ok ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)"}` }}>
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span style={{ fontWeight: 600, color: r.ok ? "var(--ok, #22c55e)" : "var(--err, #ef4444)", fontSize: 10 }}>{r.ok ? "✅" : "❌"} {r.id}</span>
                <span style={{ fontSize: 8, color: "var(--text-secondary, #555)" }}>{new Date(r.ts).toLocaleTimeString()}</span>
              </div>
              <pre style={{ margin: "2px 0 0", whiteSpace: "pre-wrap", wordBreak: "break-all", fontFamily: "monospace", fontSize: 9.5, color: "var(--text, #bbb)", maxHeight: 80, overflow: "auto" }}>{r.text}</pre>
            </div>
          ))}
        </div>

        {/* Footer */}
        <div style={{ padding: "4px 12px", borderTop: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 8, color: "var(--text-secondary, #444)" }}>Output: {OUT}</span>
          <button onClick={() => setResults([])} style={{ fontSize: 9, background: "none", border: "1px solid var(--border, #333)", borderRadius: 3, padding: "1px 6px", color: "var(--text-secondary, #666)", cursor: "pointer" }}>Clear</button>
        </div>
      </div>
      {/* Image Forensics overlay */}
      {showImageForensics && <ImageForensicsPanel onClose={() => setShowImageForensics(false)} />}
      {/* Forensics Suite overlay */}
      {showForensics && <ForensicsPanel serial={serial} onClose={() => setShowForensics(false)} />}
    </div>
  );
}

const inputStyle = (w: any): React.CSSProperties => ({
  width: w, padding: "3px 6px", fontSize: 10.5, borderRadius: 4,
  border: "1px solid var(--border, #444)", background: "var(--panel-alt, #111)", color: "var(--text, #eee)",
});
const miniBtn: React.CSSProperties = {
  padding: "3px 8px", fontSize: 10.5, borderRadius: 4,
  border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: "pointer",
};
function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
}
