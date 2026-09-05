import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { UiRecovery } from "../types";

interface ExportStepProps {
  result: UiRecovery | null;
  busy: boolean;
  exportedPath: string;
  onExport: (format: string, ext: string) => void;
  onGoTo: (step: "recover" | "case") => void;
}

export function ExportStep({ result, busy, exportedPath, onExport, onGoTo }: ExportStepProps) {
  return (
    <section className="workbench wb-export">
      <div className="panel" style={{ gridArea: "left" }}>
        <div className="panel-bar">💾 Formats</div>
        <div className="panel-body">
          <button className="primary" disabled={!result || busy} onClick={() => onExport("json", "json")}>{"{ }"} JSON</button>
          <button className="primary" disabled={!result || busy} onClick={() => onExport("csv", "csv")}>▦ CSV</button>
          <button className="primary" disabled={!result || busy} onClick={() => onExport("html", "html")}>🌐 HTML report</button>
          <p className="hint">🛡 Every export embeds the source SHA-256 fingerprint for evidence integrity.</p>
        </div>
      </div>

      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">📄 Case summary</div>
        <div className="panel-body">
          {!result ? (
            <div className="empty" style={{ textAlign: "center", padding: "20px 12px" }}>
              <p style={{ margin: "0 0 6px", fontWeight: 600 }}>Nothing recovered yet</p>
              <p className="hint small" style={{ margin: "0 0 10px" }}>Run the Recover step first to produce an export.</p>
              <button className="primary mini" onClick={() => onGoTo("recover")}>Go to ③ Recover</button>
            </div>
          ) : (
            <>
              <div className={result.db_path.toLowerCase().includes("fixture") ? "source demo-src" : "source real-src"}>
                {result.db_path.toLowerCase().includes("fixture")
                  ? <>🎭 <b>DEMO DATA</b> — this export would contain sample data, not your phone's.</>
                  : <>📱 <b>REAL DATA</b> — from your phone's database</>}
                <span className="mono small" title={result.db_path}>source: {result.db_path.split(/[\\/]/).pop()}</span>
              </div>
              <div className="stats">
                <span className="pill ok">{result.total_messages} messages</span>
                <span className={result.recovered_deleted > 0 ? "pill hot" : "pill"}>{result.recovered_deleted} deleted</span>
                <span className="pill">{result.chats} chats</span>
                <span className="pill">{result.contacts} contacts</span>
                <span className="pill">schema: {result.schema}</span>
              </div>
              <p className="hint">
                Source fingerprint: <span className="mono small" title={result.sha256}>{result.sha256 ?? "n/a"}</span>
              </p>
            </>
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "right" }}>
        <div className="panel-bar">📁 Output</div>
        <div className="panel-body">
          {exportedPath ? (
            <>
              <span className="pill ok">saved ✓</span>
              <span className="mono small">{exportedPath}</span>
              <button onClick={() => revealItemInDir(exportedPath)}>Show in Explorer</button>
              <hr className="pdiv" />
              <button className="primary mini" onClick={() => onGoTo("case")}>🗂 Continue to Case →</button>
            </>
          ) : (
            <p className="hint">No export written yet — choose a format on the left.</p>
          )}
        </div>
      </div>

      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {result ? `ready — ${result.total_messages} messages from ${result.db_path.split(/[\\/]/).pop()}` : "waiting for step ③"}
          </span>
        </div>
      </div>
    </section>
  );
}
