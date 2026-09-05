import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type { Case, CaseSummary, EvidenceItem, AuditEntry } from "../types";
import { fmtTime, fmtSize } from "../types";

interface CaseStepProps {
  serial: string;
  cases: Case[];
  activeCaseId: number | null;
  caseSummary: CaseSummary | null;
  caseEvidence: EvidenceItem[];
  caseAudit: AuditEntry[];
  caseBusy: boolean;
  caseMsg: string;
  caseForm: { case_number: string; title: string; examiner: string; subject: string; notes: string };
  caseEvidenceFilter: string;
  custodyHighlight: boolean;
  custodyRef: React.RefObject<HTMLDivElement | null>;
  exportedPath: string;
  onSelectCase: (id: number) => void;
  onCreateCase: () => void;
  onConsolidate: () => void;
  onVerifyChain: () => void;
  onExportCase: (format: "json" | "html") => void;
  onCloseCase: () => void;
  onDeleteCase: (id: number) => void;
  onCaseFormChange: (field: string, value: string) => void;
  onEvidenceFilterChange: (v: string) => void;
  onLoadCases: () => void;
  onGoToTimeline: () => void;
}

export function CaseStep({
  serial, cases, activeCaseId, caseSummary, caseEvidence, caseAudit,
  caseBusy, caseMsg, caseForm, caseEvidenceFilter, custodyHighlight, custodyRef,
  exportedPath, onSelectCase, onCreateCase, onConsolidate, onVerifyChain,
  onExportCase, onCloseCase, onDeleteCase, onCaseFormChange, onEvidenceFilterChange,
  onLoadCases, onGoToTimeline,
}: CaseStepProps) {
  return (
    <section className="workbench wb-recover">
      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            🗂 Consolidated Case File &amp; Chain of Custody
            {caseSummary && (
              <>
                {" — "}<b>{caseSummary.case_info.case_number}</b>
                <span className={caseSummary.chain_valid ? "pill ok" : "pill hot"} style={{ marginLeft: 8 }}>
                  {caseSummary.chain_valid ? "🔏 chain intact" : "⚠ chain broken"}
                </span>
              </>
            )}
            {caseMsg && <span style={{ marginLeft: 10, color: "var(--saffron-deep)" }}>{caseMsg}</span>}
          </span>
        </div>
      </div>

      <div className="panel" style={{ gridArea: "left" }}>
        <div className="panel-bar">🗂 Cases</div>
        <div className="panel-body">
          <button className="mini" disabled={caseBusy} onClick={onLoadCases}>⟳ Refresh list</button>
          <div style={{ overflowY: "auto", maxHeight: 190, margin: "8px 0" }}>
            {cases.length === 0 && (
              <div style={{ textAlign: "center", padding: "12px 0" }}>
                <p className="hint small" style={{ margin: "0 0 8px" }}>No cases yet — create your first case below.</p>
              </div>
            )}
            {cases.map((c) => (
              <div
                key={c.id}
                onClick={() => onSelectCase(c.id)}
                style={{
                  padding: "6px 8px", marginBottom: 4, borderRadius: 6, cursor: "pointer",
                  border: "1px solid var(--line)",
                  background: activeCaseId === c.id ? "var(--saffron-deep)" : "var(--card)",
                  color: activeCaseId === c.id ? "#fff" : "var(--ink)",
                }}
              >
                <div style={{ fontWeight: 600, fontSize: 12 }}>
                  {c.case_number}{c.closed_at ? " 🔒" : ""}
                </div>
                <div className="small" style={{ opacity: 0.85 }}>
                  {c.title || "untitled"} · {fmtTime(c.created_at)}
                </div>
              </div>
            ))}
          </div>

          <hr className="pdiv" />
          <div className="small" style={{ color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.5, marginBottom: 4 }}>
            New case
          </div>
          <input placeholder="Case number (auto if blank)" value={caseForm.case_number}
            onChange={(e) => onCaseFormChange("case_number", e.target.value)}
            style={{ width: "100%", marginBottom: 4, boxSizing: "border-box" }} />
          <input placeholder="Title" value={caseForm.title}
            onChange={(e) => onCaseFormChange("title", e.target.value)}
            style={{ width: "100%", marginBottom: 4, boxSizing: "border-box" }} />
          <input placeholder="Examiner" value={caseForm.examiner}
            onChange={(e) => onCaseFormChange("examiner", e.target.value)}
            style={{ width: "100%", marginBottom: 4, boxSizing: "border-box" }} />
          <input placeholder="Subject" value={caseForm.subject}
            onChange={(e) => onCaseFormChange("subject", e.target.value)}
            style={{ width: "100%", marginBottom: 4, boxSizing: "border-box" }} />
          <textarea placeholder="Notes" value={caseForm.notes}
            onChange={(e) => onCaseFormChange("notes", e.target.value)}
            style={{ width: "100%", marginBottom: 6, minHeight: 42, boxSizing: "border-box", border: "1px solid var(--line)", borderRadius: 6, padding: "5px 8px", background: "var(--card)", fontSize: 12, color: "var(--ink)", outline: "none", fontFamily: "inherit", resize: "vertical" }} />
          <button className="primary" disabled={caseBusy} onClick={onCreateCase}>
            {caseBusy ? "Working…" : "＋ Create case"}
          </button>
          <p className="hint small">Device: {serial || "none connected"}</p>
        </div>
      </div>

      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">📄 Case summary &amp; evidence</div>
        <div className="panel-body">
          {!caseSummary ? (
            <div className="empty">Select or create a case to view its consolidated evidence bundle.</div>
          ) : (
            <>
              <div className="stats">
                <span className="pill">{caseSummary.evidence_count} evidence items</span>
                <span className="pill">{caseSummary.total_records} records</span>
                <span className="pill">{caseSummary.audit_count} custody entries</span>
                <span className={caseSummary.chain_valid ? "pill ok" : "pill hot"}>
                  {caseSummary.chain_valid ? "chain intact" : "chain broken"}
                </span>
              </div>

              <div className="row" style={{ margin: "8px 0" }}>
                <button className="primary" disabled={caseBusy || !serial} onClick={onConsolidate}>
                  {caseBusy ? "Bundling…" : "📥 Acquire &amp; bundle all sources"}
                </button>
                <button disabled={caseBusy} onClick={onVerifyChain}>🔏 Verify chain</button>
                <button disabled={caseBusy} onClick={() => onExportCase("html")}>🌐 HTML report</button>
                <button disabled={caseBusy} onClick={() => onExportCase("json")}>{"{ }"} JSON</button>
                {!caseSummary.case_info.closed_at && (
                  <button disabled={caseBusy} onClick={onCloseCase}>🔒 Close case</button>
                )}
                <button className="primary" style={{ marginLeft: "auto" }} onClick={onGoToTimeline}>📊 Timeline →</button>
              </div>

              {exportedPath && (
                <p className="hint small">
                  Report saved: <span className="mono">{exportedPath}</span>{" "}
                  <button className="mini" onClick={() => revealItemInDir(exportedPath)}>Show in Explorer</button>
                </p>
              )}

              {caseSummary.by_category.length > 0 && (
                <div className="stats" style={{ marginTop: 2 }}>
                  {caseSummary.by_category.map((cc) => (
                    <span key={cc.category} className="pill">
                      {cc.category}: {cc.evidence_items} ({cc.records})
                    </span>
                  ))}
                </div>
              )}

              <hr className="pdiv" />
              <div className="small" style={{ color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.5, marginBottom: 4 }}>
                Evidence ({caseEvidence.length})
                {caseEvidenceFilter && (
                  <span className="pill" style={{ marginLeft: 6, textTransform: "none" }}>
                    filter: {caseEvidenceFilter}{" "}
                    <button className="mini" onClick={() => onEvidenceFilterChange("")}>✕</button>
                  </span>
                )}
              </div>
              <div style={{ overflowY: "auto", maxHeight: "44vh" }}>
                <table style={{ width: "100%", fontSize: 11.5, borderCollapse: "collapse" }}>
                  <thead>
                    <tr style={{ background: "#fbe6c4", color: "var(--ink)" }}>
                      <th style={{ textAlign: "left", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>Acquired</th>
                      <th style={{ textAlign: "left", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>Category</th>
                      <th style={{ textAlign: "left", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>Description</th>
                      <th style={{ textAlign: "right", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>Recs</th>
                      <th style={{ textAlign: "right", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>Size</th>
                      <th style={{ textAlign: "left", padding: "4px 6px", border: "1px solid var(--line)", fontSize: 11 }}>SHA-256</th>
                    </tr>
                  </thead>
                  <tbody>
                    {caseEvidence.filter((e) => !caseEvidenceFilter || e.category === caseEvidenceFilter).map((e) => (
                      <tr key={e.id}>
                        <td className="nowrap" style={{ padding: "4px 6px", border: "1px solid var(--line)", verticalAlign: "top" }}>{fmtTime(e.timestamp)}</td>
                        <td style={{ padding: "4px 6px", border: "1px solid var(--line)", verticalAlign: "top" }}>{e.category}</td>
                        <td title={e.source} style={{ padding: "4px 6px", border: "1px solid var(--line)", verticalAlign: "top" }}>{e.description}</td>
                        <td style={{ padding: "4px 6px", border: "1px solid var(--line)", textAlign: "right" }}>{e.item_count}</td>
                        <td style={{ padding: "4px 6px", border: "1px solid var(--line)", textAlign: "right" }}>{fmtSize(e.size_bytes)}</td>
                        <td className="mono small" title={e.sha256} style={{ padding: "4px 6px", border: "1px solid var(--line)", verticalAlign: "top" }}>{e.sha256.slice(0, 12)}…</td>
                      </tr>
                    ))}
                    {caseEvidence.filter((e) => !caseEvidenceFilter || e.category === caseEvidenceFilter).length === 0 && (
                      <tr><td colSpan={6} className="hint" style={{ padding: "4px 6px", border: "1px solid var(--line)" }}>{caseEvidenceFilter ? `No evidence in category "${caseEvidenceFilter}".` : `No evidence yet \u2014 click \u201CAcquire & bundle all sources\u201D.`}</td></tr>
                    )}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>
      </div>

      <div className="panel" ref={custodyRef} style={{ gridArea: "right", boxShadow: custodyHighlight ? "0 0 0 2px var(--saffron-deep)" : undefined, transition: "box-shadow .25s" }}>
        <div className="panel-bar">🔏 Chain of custody</div>
        <div className="panel-body">
          <p className="hint small">
            Tamper-evident log — each entry's hash covers the previous one, so any edit breaks verification.
          </p>
          <div style={{ overflowY: "auto", maxHeight: "56vh" }}>
            {caseAudit.map((a, i) => (
              <div key={a.id} style={{ padding: "5px 0", borderBottom: "1px solid var(--line)" }}>
                <div style={{ fontSize: 11.5 }}>
                  <b>{i + 1}. {a.action}</b>
                  <span className="small" style={{ color: "var(--muted)", marginLeft: 6 }}>{fmtTime(a.timestamp)}</span>
                </div>
                <div className="small">{a.detail}</div>
                <div className="small" style={{ color: "var(--muted)" }}>by {a.actor || "—"}</div>
                <div className="mono" style={{ fontSize: 9.5, color: "var(--saffron-deep)", wordBreak: "break-all" }}>{a.hash_chain}</div>
              </div>
            ))}
            {caseAudit.length === 0 && <p className="hint small">No custody entries yet.</p>}
          </div>
          {activeCaseId != null && (
            <button className="mini" style={{ marginTop: 8, color: "#8c2f22" }} onClick={() => onDeleteCase(activeCaseId)}>
              🗑 Delete case
            </button>
          )}
        </div>
      </div>
    </section>
  );
}
