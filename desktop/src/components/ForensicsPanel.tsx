import { useState, useCallback } from "react";
import * as api from "../api";
import {
  FORENSIC_TABS, SPECIAL_TABS, CATEGORY_LABELS,
  type ForensicCategory, type ForensicTabDef,
} from "./forensics/forensicTabs.config";
import { ForensicTabCard } from "./ui/ForensicTabCard";

const OUT = "D:\\recovery-tool\\workspace";

interface ForensicsPanelProps {
  serial: string | null;
  onClose: () => void;
}

const CATEGORIES: ForensicCategory[] = ["device", "extraction", "network", "analysis", "security"];

// All tabs for the tab bar (FORENSIC_TABS + thermal special)
const ALL_TAB_BAR: { id: string; icon: string; label: string; category: ForensicCategory }[] = [
  ...FORENSIC_TABS.map(t => ({ id: t.id, icon: t.icon, label: t.label, category: t.category })),
  { id: SPECIAL_TABS.thermal.id, icon: SPECIAL_TABS.thermal.icon, label: SPECIAL_TABS.thermal.label, category: SPECIAL_TABS.thermal.category },
];

const LONG_RUNNING = new Set(FORENSIC_TABS.filter(t => t.longRunning).map(t => t.id));
LONG_RUNNING.add("thermal");

export function ForensicsPanel({ serial, onClose }: ForensicsPanelProps) {
  const [tab, setTab] = useState("identity");
  const [results, setResults] = useState<Record<string, string>>({});
  const [typedResults, setTypedResults] = useState<Record<string, unknown>>({});
  const [viewModes, setViewModes] = useState<Record<string, "table" | "raw">>({});
  const [busy, setBusy] = useState<Set<string>>(new Set());
  const [errors, setErrors] = useState<Record<string, string | undefined>>({});
  const [statusMsg, setStatusMsg] = useState("");

  const run = useCallback(async (id: string, fn: () => Promise<string>) => {
    if (!serial) {
      setStatusMsg("No device connected — please connect a device first");
      setErrors(prev => ({ ...prev, [id]: "No device connected. Serial is empty." }));
      return;
    }
    setBusy(prev => new Set(prev).add(id));
    setErrors(prev => ({ ...prev, [id]: undefined }));
    setStatusMsg(`Running ${id}...`);
    console.log(`[ForensicsPanel] Running ${id} with serial=${serial}`);
    try {
      const timeoutMs = LONG_RUNNING.has(id) ? 180000 : 60000;
      const timeoutPromise = new Promise<string>((_, reject) =>
        setTimeout(() => reject(new Error(`Command timed out after ${timeoutMs / 1000} seconds`)), timeoutMs)
      );
      const result = await Promise.race([fn(), timeoutPromise]);
      console.log(`[ForensicsPanel] ${id} completed, result length=${result.length}`);
      setResults(prev => ({ ...prev, [id]: result }));
      setStatusMsg(`${id} completed successfully`);
    } catch (e) {
      const errMsg = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
      console.error(`[ForensicsPanel] ${id} failed:`, errMsg);
      setErrors(prev => ({ ...prev, [id]: errMsg }));
      setStatusMsg(`${id} failed`);
    } finally {
      setBusy(prev => { const n = new Set(prev); n.delete(id); return n; });
    }
  }, [serial]);

  const isBusy = (id: string) => busy.has(id);
  const result = (id: string) => results[id];
  const error = (id: string) => errors[id];

  const tabBtn = (active: boolean): React.CSSProperties => ({
    padding: "4px 10px", fontSize: 10.5, borderRadius: 6,
    border: active ? "1px solid #3b82f6" : "1px solid var(--border, #444)",
    background: active ? "rgba(59,130,246,0.15)" : "var(--panel-alt, #1a1a2e)",
    color: active ? "#93c5fd" : "var(--text, #eee)",
    cursor: "pointer", fontWeight: active ? 600 : 400,
    whiteSpace: "nowrap",
  });

  // Run a standard tab's apiCall
  const runTab = (t: ForensicTabDef) => {
    if (t.id === "report") {
      const caseId = prompt("Enter Case ID:") || `CASE-${Date.now()}`;
      run(t.id, () => api.generatePdfReport(serial!, OUT, caseId));
    } else {
      run(t.id, () => t.apiCall(serial || "NONE"));
      // Also fetch typed data if available
      if (t.typedApi && serial) {
        t.typedApi(serial).then(data => {
          setTypedResults(prev => ({ ...prev, [t.id]: data }));
          // Default to table view for typed tabs
          setViewModes(prev => prev[t.id] ? prev : { ...prev, [t.id]: "table" });
        }).catch(() => { /* typed fetch failed, raw still works */ });
      }
    }
  };

  // Find the active tab definition
  const activeTabDef = FORENSIC_TABS.find(t => t.id === tab);
  const isThermal = tab === "thermal";

  const renderContent = () => {
    if (isThermal) {
      const st = SPECIAL_TABS.thermal;
      return (
        <ForensicTabCard
          icon={st.icon} label={st.label} description={st.description}
          gradient={st.gradient1} busyLabel="Stress Testing…"
          actionLabel="Run Thermal Stress Test" isBusy={isBusy(tab)}
          hasDevice={!!serial} result={result(tab)} error={error(tab)}
          onRun={() => {
            const durStr = prompt("Stress test duration in seconds (5-120, default 30):", "30");
            const dur = durStr ? parseInt(durStr, 10) : 30;
            const duration = isNaN(dur) ? 30 : Math.max(5, Math.min(120, dur));
            run(tab, () => api.runThermalStressTest(serial || "NONE", duration));
          }}
          extraActions={
            <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.getDeviceTemperatures(serial || "NONE"))}
              style={{ padding: "4px 12px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: isBusy(tab) ? "#555" : st.gradient2, color: "#fff", fontWeight: 600, cursor: "pointer", opacity: !serial ? 0.6 : 1 }}>
              {isBusy(tab) ? "⏳ Reading…" : "🌡 Read Temperature"}
            </button>
          }
        />
      );
    }
    if (!activeTabDef) return null;
    return (
      <ForensicTabCard
        icon={activeTabDef.icon} label={activeTabDef.label}
        description={activeTabDef.description} gradient={activeTabDef.gradient}
        busyLabel={activeTabDef.busyLabel} actionLabel={activeTabDef.actionLabel}
        isBusy={isBusy(tab)} hasDevice={!!serial}
        result={result(tab)} error={error(tab)}
        onRun={() => runTab(activeTabDef)}
        typedData={typedResults[tab]}
        tabId={tab}
        hasTypedApi={!!activeTabDef.typedApi}
        viewMode={viewModes[tab] || "raw"}
        onViewModeChange={(mode) => setViewModes(prev => ({ ...prev, [tab]: mode }))}
      />
    );
  };

  const completedCount = FORENSIC_TABS.filter(t => results[t.id]).length + (results["thermal"] ? 1 : 0);
  const totalTabs = ALL_TAB_BAR.length;

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 999, background: "rgba(0,0,0,0.7)", display: "flex", alignItems: "center", justifyContent: "center" }} onClick={onClose}>
      <div onClick={e => e.stopPropagation()} style={{ background: "var(--bg, #0f0f1a)", border: "1px solid var(--border, #333)", borderRadius: 12, width: 860, maxHeight: "90vh", display: "flex", flexDirection: "column", boxShadow: "0 8px 40px rgba(0,0,0,0.5)" }}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "10px 16px", borderBottom: "1px solid var(--border, #333)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ fontSize: 14, fontWeight: 700, color: "var(--text, #eee)" }}>🔬 Forensic Analysis Suite</span>
            <span style={{ fontSize: 10, padding: "2px 8px", borderRadius: 4, background: serial ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)", color: serial ? "#4ade80" : "#f87171", border: `1px solid ${serial ? "rgba(34,197,94,0.3)" : "rgba(239,68,68,0.3)"}` }}>
              {serial ? `📱 ${serial}` : "⚠ No Device"}
            </span>
          </div>
          <button onClick={onClose} style={{ background: "none", border: "none", color: "var(--text-secondary, #aaa)", cursor: "pointer", fontSize: 16 }}>✕</button>
        </div>

        {/* Status bar */}
        {statusMsg && (
          <div style={{ padding: "4px 16px", fontSize: 10, color: statusMsg.includes("failed") ? "#f87171" : statusMsg.includes("completed") ? "#4ade80" : "#60a5fa", background: "rgba(0,0,0,0.3)", borderBottom: "1px solid var(--border, #222)" }}>
            {statusMsg}
          </div>
        )}

        {/* Tab bar — grouped by category */}
        <div style={{ padding: "6px 12px", borderBottom: "1px solid var(--border, #222)" }}>
          {CATEGORIES.map(cat => {
            const catTabs = ALL_TAB_BAR.filter(t => t.category === cat);
            if (catTabs.length === 0) return null;
            return (
              <div key={cat} style={{ marginBottom: 4 }}>
                <span style={{ fontSize: 9, color: "#666", textTransform: "uppercase", letterSpacing: 0.5, marginRight: 6 }}>{CATEGORY_LABELS[cat]}</span>
                <span style={{ display: "inline-flex", gap: 3, flexWrap: "wrap" }}>
                  {catTabs.map(t => (
                    <button key={t.id} onClick={() => setTab(t.id)} style={tabBtn(tab === t.id)}>
                      {t.icon} {t.label}
                    </button>
                  ))}
                </span>
              </div>
            );
          })}
        </div>

        {/* Content */}
        <div style={{ flex: 1, overflow: "auto", padding: "12px 16px", minHeight: 300 }}>
          {!serial && (
            <div style={{ padding: 20, marginBottom: 12, background: "rgba(239,68,68,0.08)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.2)", color: "#f87171", fontSize: 12, textAlign: "center" }}>
              ⚠ No device connected. Please connect an Android device via ADB before using forensic tools.
            </div>
          )}
          {renderContent()}
        </div>

        {/* Footer */}
        <div style={{ padding: "6px 16px", borderTop: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #444)" }}>Output: {OUT}</span>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #444)" }}>
            {completedCount}/{totalTabs} analyses complete
          </span>
        </div>
      </div>
    </div>
  );
}
