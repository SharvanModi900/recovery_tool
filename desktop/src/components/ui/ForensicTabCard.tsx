import type { CSSProperties, ReactNode } from "react";
import { TypedDataRenderer } from "./TypedDataRenderer";

export interface ForensicTabCardProps {
  icon: string;
  label: string;
  description: string;
  gradient: string;
  busyLabel: string;
  actionLabel: string;
  isBusy: boolean;
  hasDevice: boolean;
  result: string | undefined;
  error: string | undefined;
  onRun: () => void;
  extraActions?: ReactNode;
  /** Typed data for structured table view */
  typedData?: unknown;
  /** Tab ID for TypedDataRenderer dispatch */
  tabId?: string;
  /** Whether this tab has a typed API available */
  hasTypedApi?: boolean;
  /** Current view mode */
  viewMode?: "table" | "raw";
  /** Callback to switch view mode */
  onViewModeChange?: (mode: "table" | "raw") => void;
}

const btnBase: CSSProperties = {
  padding: "4px 12px",
  fontSize: 11,
  borderRadius: 6,
  border: "1px solid var(--border, #444)",
  background: "var(--panel-alt, #1a1a2e)",
  color: "var(--text, #eee)",
  cursor: "pointer",
};

export function ForensicTabCard({
  icon, label: _label, description, gradient, busyLabel, actionLabel,
  isBusy, hasDevice, result, error, onRun, extraActions,
  typedData, tabId, hasTypedApi, viewMode, onViewModeChange,
}: ForensicTabCardProps) {
  const btnStyle: CSSProperties = {
    ...btnBase,
    background: isBusy ? "#555" : gradient,
    color: "#fff",
    fontWeight: 600,
    opacity: !hasDevice ? 0.6 : 1,
  };

  return (
    <div>
      <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
        <button disabled={isBusy} onClick={onRun} style={btnStyle}>
          {isBusy ? `⏳ ${busyLabel}` : `${icon} ${actionLabel}`}
        </button>
        {extraActions}
        {hasTypedApi && !isBusy && (result || typedData != null) && (
          <div style={{ display: "flex", gap: 2, marginLeft: "auto" }}>
            <button onClick={() => onViewModeChange?.("table")} style={{ ...btnBase, fontSize: 9, padding: "2px 8px", background: viewMode === "table" ? "rgba(59,130,246,0.2)" : "transparent", border: viewMode === "table" ? "1px solid #3b82f6" : "1px solid #333", color: viewMode === "table" ? "#93c5fd" : "#888" }}>📊 Table</button>
            <button onClick={() => onViewModeChange?.("raw")} style={{ ...btnBase, fontSize: 9, padding: "2px 8px", background: viewMode === "raw" ? "rgba(59,130,246,0.2)" : "transparent", border: viewMode === "raw" ? "1px solid #3b82f6" : "1px solid #333", color: viewMode === "raw" ? "#93c5fd" : "#888" }}>📝 Raw</button>
          </div>
        )}
        <span style={{ fontSize: 10, color: "#888" }}>{description}</span>
      </div>
      {renderResult(isBusy, result, error, typedData, tabId, viewMode, hasTypedApi)}
    </div>
  );
}

function renderResult(isBusy: boolean, result: string | undefined, error: string | undefined, typedData?: unknown, tabId?: string, viewMode?: string, hasTypedApi?: boolean) {
  if (isBusy) return (
    <div style={{ textAlign: "center", padding: 40, color: "#888" }}>
      <div style={{ fontSize: 32, marginBottom: 8 }}>⏳</div>
      Analyzing... Please wait
    </div>
  );
  if (error) return (
    <div style={{ padding: 16, color: "#f87171", background: "rgba(239,68,68,0.08)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.2)" }}>
      <div style={{ fontWeight: 600, marginBottom: 4 }}>Error</div>
      <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 11, whiteSpace: "pre-wrap" }}>{error}</pre>
    </div>
  );
  // Table view: show typed data if available and in table mode
  if (viewMode === "table" && typedData && tabId) {
    return <TypedDataRenderer tabId={tabId} data={typedData} />;
  }
  // Raw view or no typed data: show text result
  if (result) return (
    <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 11, color: "#ccc", whiteSpace: "pre-wrap", lineHeight: 1.6, background: "rgba(0,0,0,0.2)", padding: 12, borderRadius: 8, border: "1px solid #222", maxHeight: 400, overflow: "auto" }}>
      {result}
    </pre>
  );
  // If we have typed data but no raw result, show typed data in table mode by default
  if (hasTypedApi && typedData && tabId) {
    return <TypedDataRenderer tabId={tabId} data={typedData} />;
  }
  return (
    <div style={{ textAlign: "center", padding: 40, color: "#555" }}>
      Click the button above to start
    </div>
  );
}
