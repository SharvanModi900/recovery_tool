import { useState, useCallback } from "react";
import { DeviceToolsDashboard } from "./device-tools/DeviceToolsDashboard";
import { DeviceToolsInfo } from "./device-tools/DeviceToolsInfo";
import { DeviceToolsQuickActions } from "./device-tools/DeviceToolsQuickActions";
import { DeviceToolsFilesTab } from "./device-tools/DeviceToolsFilesTab";
import { DeviceToolsProcessesTab } from "./device-tools/DeviceToolsProcessesTab";
import { DeviceToolsCoolingTab } from "./device-tools/DeviceToolsCoolingTab";
import { DeviceToolsEvidenceTab } from "./device-tools/DeviceToolsEvidenceTab";
import { DeviceToolsDisplayTab } from "./device-tools/DeviceToolsDisplayTab";
import type { ToolResult } from "./device-tools/shared";

type TabId = "dashboard" | "info" | "quickactions" | "files" | "processes" | "cooling" | "evidence" | "display";

interface DeviceToolsPanelProps {
  serial: string | null;
}

const TABS: { id: TabId; icon: string; label: string }[] = [
  { id: "dashboard", icon: "🏠", label: "Dashboard" },
  { id: "info", icon: "📱", label: "Info" },
  { id: "quickactions", icon: "⚡", label: "Quick Actions" },
  { id: "files", icon: "📂", label: "Files" },
  { id: "processes", icon: "⚙", label: "Processes" },
  { id: "cooling", icon: "❄", label: "Cooling" },
  { id: "evidence", icon: "🔍", label: "Evidence" },
  { id: "display", icon: "📱", label: "Display" },
];

export function DeviceToolsPanel({ serial }: DeviceToolsPanelProps) {
  const [tab, setTab] = useState<TabId>("dashboard");
  const [results, setResults] = useState<ToolResult[]>([]);
  const [busy, setBusy] = useState<Set<string>>(new Set());

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

  const tabProps = { serial, results, busy, run, addResult };

  const btnStyle = (active: boolean): React.CSSProperties => ({
    padding: "4px 10px", fontSize: 10.5, borderRadius: 6,
    border: active ? "1px solid #3b82f6" : "1px solid var(--border, #444)",
    background: active ? "rgba(59,130,246,0.15)" : "var(--panel-alt, #1a1a2e)",
    color: active ? "#93c5fd" : "var(--text, #eee)",
    cursor: "pointer", fontWeight: active ? 600 : 400,
    whiteSpace: "nowrap",
  });

  const renderTab = () => {
    switch (tab) {
      case "dashboard": return <DeviceToolsDashboard {...tabProps} />;
      case "info": return <DeviceToolsInfo {...tabProps} />;
      case "quickactions": return <DeviceToolsQuickActions {...tabProps} />;
      case "files": return <DeviceToolsFilesTab {...tabProps} />;
      case "processes": return <DeviceToolsProcessesTab {...tabProps} />;
      case "cooling": return <DeviceToolsCoolingTab {...tabProps} />;
      case "evidence": return <DeviceToolsEvidenceTab {...tabProps} />;
      case "display": return <DeviceToolsDisplayTab {...tabProps} />;
    }
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}>
      {/* Tab bar */}
      <div style={{ display: "flex", gap: 3, padding: "6px 12px", borderBottom: "1px solid var(--border, #222)", flexWrap: "wrap", alignItems: "center" }}>
        <span style={{ fontSize: 13, fontWeight: 700, color: "var(--text, #eee)", marginRight: 8 }}>🛠 Device Tools</span>
        <span style={{ fontSize: 10, padding: "2px 8px", borderRadius: 4, background: serial ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)", color: serial ? "#4ade80" : "#f87171", border: `1px solid ${serial ? "rgba(34,197,94,0.3)" : "rgba(239,68,68,0.3)"}`, marginRight: 8 }}>
          {serial ? `📱 ${serial}` : "⚠ No Device"}
        </span>
        {TABS.map(t => (
          <button key={t.id} onClick={() => setTab(t.id)} style={btnStyle(tab === t.id)}>
            {t.icon} {t.label}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div style={{ flex: 1, overflow: "auto", minHeight: 300 }}>
        {!serial && (
          <div style={{ padding: 20, marginBottom: 12, background: "rgba(239,68,68,0.08)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.2)", color: "#f87171", fontSize: 12, textAlign: "center" }}>
            ⚠ No device connected. Please connect an Android device via ADB before using device tools.
          </div>
        )}
        {renderTab()}
      </div>

      {/* Results panel */}
      <div style={{ padding: "6px 12px", minHeight: 60, maxHeight: 140, overflow: "auto", borderTop: "1px solid var(--border, #222)" }}>
        {results.length === 0 ? (
          <div style={{ color: "var(--text-secondary, #555)", fontSize: 10, textAlign: "center", padding: 8 }}>
            Tool results appear here
          </div>
        ) : results.map((r, i) => (
          <div key={r.ts + i} style={{ padding: "3px 6px", marginBottom: 2, borderRadius: 4, fontSize: 10, background: r.ok ? "rgba(34,197,94,0.06)" : "rgba(239,68,68,0.06)", border: `1px solid ${r.ok ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)"}` }}>
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span style={{ fontWeight: 600, color: r.ok ? "var(--ok, #22c55e)" : "var(--err, #ef4444)", fontSize: 10 }}>{r.ok ? "✅" : "❌"} {r.id}</span>
              <span style={{ fontSize: 8, color: "var(--text-secondary, #555)" }}>{new Date(r.ts).toLocaleTimeString()}</span>
            </div>
            <pre style={{ margin: "2px 0 0", whiteSpace: "pre-wrap", wordBreak: "break-all", fontFamily: "monospace", fontSize: 9.5, color: "var(--text, #bbb)", maxHeight: 60, overflow: "auto" }}>{r.text}</pre>
          </div>
        ))}
      </div>

      {/* Footer */}
      <div style={{ padding: "4px 12px", borderTop: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ fontSize: 8, color: "var(--text-secondary, #444)" }}>Output: D:\recovery-tool\workspace</span>
        <button onClick={() => setResults([])} style={{ fontSize: 9, background: "none", border: "1px solid var(--border, #333)", borderRadius: 3, padding: "1px 6px", color: "var(--text-secondary, #666)", cursor: "pointer" }}>Clear Results</button>
      </div>
    </div>
  );
}
