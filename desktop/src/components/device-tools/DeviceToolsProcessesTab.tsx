import * as api from "../../api";
import { type DeviceToolsTabProps, miniBtn } from "./shared";

export function DeviceToolsProcessesTab({ serial, results, busy, run }: DeviceToolsTabProps) {
  const isBusy = (id: string) => busy.has(id);
  const procsResult = results.find(r => r.id === "procs" && r.ok);

  return (
    <div>
      <div style={{ padding: "6px 12px", borderBottom: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <span style={{ fontSize: 11, color: "var(--text-secondary, #888)" }}>
          {procsResult ? procsResult.text.split("\n").filter(Boolean).length : 0} processes
        </span>
        <button disabled={!serial || isBusy("procs")} onClick={() => run("procs", () => api.getRunningProcesses(serial!))} style={miniBtn}>
          {isBusy("procs") ? "⏳" : "🔄"} Refresh
        </button>
      </div>
      <div style={{ flex: 1, overflow: "auto", padding: "4px 12px", minHeight: 200, maxHeight: 400 }}>
        {procsResult ? (
          <table style={{ width: "100%", fontSize: 10.5, borderCollapse: "collapse" }}>
            <thead><tr style={{ borderBottom: "1px solid var(--border, #333)", color: "var(--text-secondary, #888)" }}>
              <th style={{ textAlign: "left", padding: "3px 6px", width: 60 }}>PID</th>
              <th style={{ textAlign: "left", padding: "3px 6px" }}>Process</th>
            </tr></thead>
            <tbody>
              {procsResult.text.split("\n").filter(Boolean).map((line, i) => {
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
    </div>
  );
}
