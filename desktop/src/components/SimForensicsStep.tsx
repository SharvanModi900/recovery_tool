import type { SimSnapshot, HistoricalSim, SimSwapVerdict } from "../types";

type Step = "connect" | "apps" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "forensics" | "device_tools" | "recover" | "export" | "case" | "timeline";

interface SimForensicsStepProps {
  serial: string;
  simSnapshot: SimSnapshot | null;
  simBaseline: SimSnapshot | null;
  simHistorical: HistoricalSim[];
  simVerdict: SimSwapVerdict | null;
  simHistory: SimSnapshot[];
  simBusy: boolean;
  onCollectSim: () => void;
  onSimBaseline: (id: number) => void;
  onSimSwapCheck: () => void;
  onDeleteSimSnap: (id: number) => void;
  onGoTo: (step: Step) => void;
}

export function SimForensicsStep({
  serial, simSnapshot, simBaseline, simHistorical, simVerdict, simHistory, simBusy,
  onCollectSim, onSimBaseline, onSimSwapCheck, onDeleteSimSnap, onGoTo,
}: SimForensicsStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">
          📶 SIM / Network Forensics
          {simBaseline && <span className="pill ok" style={{ marginLeft: 8 }}>★ baseline set</span>}
          {simVerdict && simVerdict.risk === "HIGH" && (
            <span className="pill" style={{ marginLeft: 8, background: "#e05e00", color: "#fff" }}>⚠ swap risk</span>
          )}
        </div>
        <div className="panel-body">
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            <button className="primary" disabled={!serial || simBusy} onClick={onCollectSim} style={{ flex: 1 }}>
              {simBusy ? "Reading…" : "📡 Collect snapshot"}
            </button>
            <button className="mini" disabled={!serial || simBusy || !simSnapshot} onClick={() => simSnapshot && onSimBaseline(simSnapshot.id)} title="Mark the current snapshot as the known-good baseline">
              🎯 Baseline
            </button>
            <button className="mini" disabled={!serial || simBusy} onClick={onSimSwapCheck} title="Collect + compare against baseline and the device's SIM history">
              🔍 Check swap
            </button>
          </div>
          <p className="hint small">
            Reads SIM & operator identity (ICCID, number, MCC/MNC, carrier, state) plus the device's own historical SIM records — all read-only, no root.
          </p>

          {simVerdict && (() => {
            const r = simVerdict.risk;
            const label = r === "HIGH" ? "⚠ HIGH — SIM-swap indicator" : r === "LOW" ? "◑ LOW — minor change" : "✓ NONE — no swap detected";
            return (
              <div className={`sim-verdict ${r.toLowerCase()}`}>
                <b>{label}</b>
                <ul>{simVerdict.reasons.map((x, i) => <li key={i}>{x}</li>)}</ul>
              </div>
            );
          })()}

          {simSnapshot ? (
            <div className="sim-grid">
              <span className="sim-k">Carrier</span><span className="sim-v">{simSnapshot.carrier_name || "—"}</span>
              <span className="sim-k">Number</span><span className="sim-v">{simSnapshot.msisdn || "—"}</span>
              <span className="sim-k">ICCID</span><span className="sim-v mono">{simSnapshot.iccid || "—"}</span>
              <span className="sim-k">MCC / MNC</span><span className="sim-v">{simSnapshot.mcc || "?"} / {simSnapshot.mnc || "?"}</span>
              <span className="sim-k">Country</span><span className="sim-v">{(simSnapshot.country_iso || "—").toUpperCase()}</span>
              <span className="sim-k">SIM state</span><span className="sim-v">{simSnapshot.sim_state || "—"}</span>
              <span className="sim-k">Slot</span><span className="sim-v">{simSnapshot.slot_index ?? "—"}</span>
              <span className="sim-k">Network</span><span className="sim-v">{simSnapshot.network_operator || "—"} · {simSnapshot.network_type || "—"}</span>
              <span className="sim-k">Roaming</span><span className="sim-v">{simSnapshot.roaming || "—"}</span>
              <span className="sim-k">Baseband</span><span className="sim-v mono">{simSnapshot.baseband || "—"}</span>
              <span className="sim-k">IMEI</span><span className="sim-v mono">{simSnapshot.imei || "—"}</span>
              <span className="sim-k">IMSI</span><span className="sim-v mono">{simSnapshot.imsi || "—"}</span>
            </div>
          ) : (
            <p className="hint small">No snapshot yet — connect a device and tap "Collect snapshot".</p>
          )}

          {simBaseline && (
            <p className="hint small" style={{ marginTop: 8 }}>
              ★ Baseline: <span className="mono">{simBaseline.iccid ? `${simBaseline.iccid.slice(0, 6)}…${simBaseline.iccid.slice(-4)}` : "—"}</span>
              {simBaseline.carrier_name ? ` · ${simBaseline.carrier_name}` : ""} · {new Date(simBaseline.timestamp).toLocaleString()}
            </p>
          )}

          {simHistorical.length > 0 && (
            <>
              <div className="sim-sub">Device SIM records ({simHistorical.length})</div>
              <div className="sim-scroll">
                <table className="sim-table">
                  <thead><tr><th>ICCID</th><th>Carrier</th><th>Slot</th><th></th></tr></thead>
                  <tbody>
                    {simHistorical.map((h, i) => (
                      <tr key={i}>
                        <td className="mono">{h.iccid ? `${h.iccid.slice(0, 6)}…${h.iccid.slice(-4)}` : "—"}</td>
                        <td>{h.carrier || "—"}</td>
                        <td>{h.slot ?? "—"}</td>
                        <td><span className={`sim-tag ${h.active ? "active" : "inactive"}`}>{h.active ? "active" : "past"}</span></td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}

          {simHistory.length > 0 && (
            <>
              <div className="sim-sub">Snapshot history ({simHistory.length})</div>
              <div className="sim-scroll">
                {simHistory.map((s) => (
                  <div className="sim-hist-item" key={s.id}>
                    <span>{s.is_baseline ? "★" : "•"}</span>
                    <span className="mono">{s.iccid ? `${s.iccid.slice(0, 6)}…${s.iccid.slice(-4)}` : "—"}</span>
                    <span style={{ color: "var(--muted)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{s.carrier_name || ""}</span>
                    <span style={{ marginLeft: "auto", fontSize: 9.5, color: "var(--muted)", whiteSpace: "nowrap" }}>{new Date(s.timestamp).toLocaleTimeString()}</span>
                    {!s.is_baseline && <button className="mini" title="Set as baseline" onClick={() => onSimBaseline(s.id)}>🎯</button>}
                    <button className="mini" title="Delete" onClick={() => onDeleteSimSnap(s.id)}>🗑</button>
                  </div>
                ))}
              </div>
            </>
          )}

          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("contacts")}>← Contacts</button>
            <button className="mini" onClick={() => onGoTo("forensics")}>Forensics →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
