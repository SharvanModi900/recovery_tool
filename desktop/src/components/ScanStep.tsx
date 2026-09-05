import type { ScanReport } from "../types";
import { fmtSize } from "../types";

interface ScanStepProps {
  serial: string;
  busy: boolean;
  includeMedia: boolean;
  report: ScanReport | null;
  dbPath: string;
  cryptPath: string;
  onScanDevice: (pull: boolean) => void;
  onScanFolder: () => void;
  onIncludeMediaChange: (v: boolean) => void;
  onGoTo: (step: "connect" | "recover") => void;
}

export function ScanStep({
  serial, busy, includeMedia, report, dbPath, cryptPath,
  onScanDevice, onScanFolder, onIncludeMediaChange, onGoTo,
}: ScanStepProps) {
  return (
    <section className="workbench wb-scan">
      <div className="panel" style={{ gridArea: "left" }}>
        <div className="panel-bar">⚙ Actions</div>
        <div className="panel-body">
          <button className="primary" disabled={!serial || busy} onClick={() => onScanDevice(false)}>🔍 Scan device</button>
          <button className="primary" disabled={!serial || busy} onClick={() => onScanDevice(true)}>⬇ Scan & pull</button>
          <label className="check">
            <input type="checkbox" checked={includeMedia} onChange={(e) => onIncludeMediaChange(e.target.checked)} />
            include media
          </label>
          <hr className="pdiv" />
          <button disabled={busy} onClick={onScanFolder}>📂 Import folder…</button>
          <p className="hint">Scan enumerates WhatsApp databases, encrypted backups & media without copying. Scan & pull also copies them locally for recovery.</p>
        </div>
      </div>

      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">
          🗂 Artifacts
          <span className="sep" />
          <span className="hint">{report ? `${report.artifacts.length} found` : "run a scan"}</span>
        </div>
        <div className="panel-body">
          {report ? (
            report.artifacts.length > 0 ? (
              <table className="grid">
                <thead><tr><th>Kind</th><th>Path</th><th>Size</th></tr></thead>
                <tbody>
                  {report.artifacts.map((a) => (
                    <tr key={a.path} className={a.path === dbPath || a.path === cryptPath ? "sel" : ""}>
                      <td><span className={`pill kind-${a.kind.toLowerCase()}`}>{a.kind}</span></td>
                      <td className="mono small" title={a.path}>{a.path}</td>
                      <td>{fmtSize(a.size)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            ) : (
              <div className="empty">No artifacts found on this device.</div>
            )
          ) : (
            <div className="empty" style={{ textAlign: "center", padding: "20px 12px" }}>
              {serial ? (
                <>
                  <p style={{ margin: "0 0 6px", fontWeight: 600 }}>Ready to scan</p>
                  <p className="hint small" style={{ margin: "0 0 10px" }}>Click <b>🔍 Scan device</b> to enumerate WhatsApp databases and backups.</p>
                </>
              ) : (
                <>
                  <p style={{ margin: "0 0 6px", fontWeight: 600 }}>No device connected</p>
                  <p className="hint small" style={{ margin: "0 0 10px" }}>Connect a device in the Connect step first.</p>
                  <button className="primary mini" onClick={() => onGoTo("connect")}>Go to ① Connect</button>
                </>
              )}
            </div>
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "right" }}>
        <div className="panel-bar">📋 Summary</div>
        <div className="panel-body">
          {report ? (
            <>
              <div className="stats col">
                <span className={report.whatsapp_found ? "pill ok" : "pill warn"}>
                  {report.whatsapp_found ? "WhatsApp data found" : "no WhatsApp data"}
                </span>
                <span className="pill">{report.artifacts.filter((a) => a.kind === "database").length} databases</span>
                <span className="pill">{report.artifacts.filter((a) => a.kind === "backup").length} encrypted backups</span>
                <span className="pill">{report.artifacts.filter((a) => a.kind === "media").length} media</span>
              </div>
              {report.notes.map((n, i) => (
                <p key={i} className="hint">• {n}</p>
              ))}
            </>
          ) : (
            <p className="hint">The scan summary will appear here.</p>
          )}
        </div>
      </div>

      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {report
              ? `${report.artifacts.length} artifacts · ${report.artifacts.filter((a) => a.kind === "database").length} databases · ${report.artifacts.filter((a) => a.kind === "backup").length} backups`
              : "no scan yet"}
          </span>
          {report && (
            <button className="primary next" disabled={report.artifacts.length === 0} onClick={() => onGoTo("recover")}>
              Continue to Recover →
            </button>
          )}
        </div>
      </div>
    </section>
  );
}
