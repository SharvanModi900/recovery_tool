import { useCallback, useEffect, useState, type CSSProperties } from "react";
import * as api from "../api";
import { fmtTime } from "../types";
import { TimelineInspector } from "./TimelineInspector";
import { meta } from "./timelineMeta";
import type { CaseSummary, TimelineEvent, TimelinePage } from "../types";

/**
 * Unified cross-source investigative timeline for a consolidated case.
 *
 * Everything lives in `case.db`: this view merges every time-stamped record the
 * case bundled (calls, SMS, WhatsApp chats/calls/messages, calendar, recordings,
 * screen captures, SIM snapshots) into one reverse-chronological stream and
 * adds keyword search + kind facets + a date-range filter across all of it.
 *
 * Self-contained: it loads its own data from `caseId`, so App.tsx only mounts it.
 */

export interface TimelineViewProps {
  caseId: number | null;
  /** Currently connected device serial — used to pull recordings for playback. */
  serial: string;
  /** Whether a recovery run is available to pull WhatsApp message text from. */
  hasRecovered: boolean;
  /** Jump to the Case step's Evidence bundle or chain-of-custody log. */
  onJump?: (target: "evidence" | "custody", category?: string) => void;
  /** Switch user to the Case step (optionally triggering case creation). */
  onGoToCase?: () => void;
  onStatus?: (msg: string) => void;
}

const truncate = (s: string, n: number) => (s.length > n ? s.slice(0, n) + "…" : s);

export function TimelineView({ caseId, serial, hasRecovered, onJump, onGoToCase, onStatus }: TimelineViewProps) {
  const [caseInfo, setCaseInfo] = useState<CaseSummary | null>(null);
  const [page, setPage] = useState<TimelinePage | null>(null);
  const [selected, setSelected] = useState<TimelineEvent | null>(null);
  const [busy, setBusy] = useState(false);

  // Filters
  const [q, setQ] = useState(""); // live input
  const [appliedQ, setAppliedQ] = useState(""); // submitted query
  const [kind, setKind] = useState(""); // "" = all kinds
  const [from, setFrom] = useState("");
  const [to, setTo] = useState("");
  const [limit, setLimit] = useState(500);

  // Load the case label / chain status whenever the case changes.
  useEffect(() => {
    if (caseId == null) {
      setCaseInfo(null);
      setPage(null);
      return;
    }
    api.getCaseSummary(caseId).then(setCaseInfo).catch(() => setCaseInfo(null));
  }, [caseId]);

  const load = useCallback(async () => {
    if (caseId == null) {
      setPage(null);
      return;
    }
    setBusy(true);
    try {
      const fromMs = from ? new Date(from + "T00:00:00").getTime() : null;
      const toMs = to ? new Date(to + "T23:59:59.999").getTime() : null;
      const res = await api.getCaseTimeline(caseId, {
        kind: kind || null,
        q: appliedQ || null,
        from: fromMs,
        to: toMs,
        limit,
        offset: 0,
      });
      setPage(res);
      setSelected(null);
    } catch (e) {
      onStatus?.("Timeline error: " + String(e));
    } finally {
      setBusy(false);
    }
  }, [caseId, kind, appliedQ, from, to, limit, onStatus]);

  useEffect(() => {
    load();
  }, [load]);

  const reset = () => {
    setQ("");
    setAppliedQ("");
    setKind("");
    setFrom("");
    setTo("");
    setLimit(500);
  };

  const addMessages = async () => {
    if (caseId == null) return;
    setBusy(true);
    try {
      const n = await api.ingestRecoveredMessages(caseId);
      onStatus?.(`Added ${n} WhatsApp message(s) to the timeline`);
      await load();
    } catch (e) {
      onStatus?.("Could not add messages: " + String(e));
    } finally {
      setBusy(false);
    }
  };

  const label = caseInfo
    ? caseInfo.case_info.case_number +
      (caseInfo.case_info.title ? " · " + caseInfo.case_info.title : "")
    : "";

  return (
    <section className="workbench wb-timeline">
      {/* TOP strip */}
      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            📊 Unified Timeline{label && <> — <b>{label}</b></>}
            {page && (
              <span style={{ marginLeft: 8 }}>
                {busy ? "loading…" : `${page.total} event${page.total === 1 ? "" : "s"}${appliedQ ? ` matching “${appliedQ}”` : ""}`}
              </span>
            )}
          </span>
        </div>
      </div>

      {/* LEFT — search + filters */}
      <div className="panel" style={{ gridArea: "filters" }}>
        <div className="panel-bar">🔎 Search &amp; filters</div>
        <div className="panel-body">
          {caseId == null ? (
            <div style={{ textAlign: "center", padding: "20px 12px" }}>
              <p className="hint small" style={{ margin: "0 0 8px" }}>Create or select a case to start searching.</p>
              <button className="primary mini" onClick={onGoToCase}>🗂 Go to Case</button>
            </div>
          ) : (
            <>
              <div className="row">
                <input
                  placeholder="Search all sources…"
                  value={q}
                  onChange={(e) => setQ(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") setAppliedQ(q);
                  }}
                  style={{ flex: 1, minWidth: 120 }}
                />
                <button className="primary" disabled={busy} onClick={() => setAppliedQ(q)}>
                  Search
                </button>
              </div>
              <p className="hint small">
                Matches title, body and source across calls, SMS, WhatsApp, calendar, recordings, captures &amp; SIM.
              </p>

              <hr className="pdiv" />
              <div className="small" style={{ color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.5 }}>
                Sources
              </div>
              <div className="stats">
                <button className={kind === "" ? "primary mini" : "mini"} disabled={busy} onClick={() => setKind("")}>
                  All
                </button>
                {(page?.kinds ?? []).map((k) => (
                  <button
                    key={k.kind}
                    className={kind === k.kind ? "primary mini" : "mini"}
                    disabled={busy}
                    onClick={() => setKind(kind === k.kind ? "" : k.kind)}
                  >
                    {meta(k.kind).icon} {meta(k.kind).label} · {k.count}
                  </button>
                ))}
              </div>

              <hr className="pdiv" />
              <div className="small" style={{ color: "var(--muted)", textTransform: "uppercase", letterSpacing: 0.5 }}>
                Date range
              </div>
              <div className="row">
                <input type="date" value={from} onChange={(e) => setFrom(e.target.value)} style={dateStyle} />
                <input type="date" value={to} onChange={(e) => setTo(e.target.value)} style={dateStyle} />
              </div>
              <div className="row">
                <select value={limit} onChange={(e) => setLimit(Number(e.target.value))} style={{ flex: 1 }}>
                  <option value={200}>Show 200</option>
                  <option value={500}>Show 500</option>
                  <option value={2000}>Show 2000</option>
                  <option value={5000}>Show 5000</option>
                </select>
                <button className="mini" disabled={busy} onClick={reset}>↺ Reset</button>
              </div>

              <hr className="pdiv" />
              <button
                className="mini"
                disabled={busy || !hasRecovered}
                title={hasRecovered ? "Add message text from the last Recover run" : "Run the Recover step first"}
                onClick={addMessages}
              >
                ✉️ Add recovered WhatsApp messages
              </button>
              {!hasRecovered && <p className="hint small">Available after a Recover run.</p>}
            </>
          )}
        </div>
      </div>

      {/* MAIN — merged event list */}
      <div className="panel" style={{ gridArea: "list" }}>
        <div className="panel-bar">
          🕑 Events
          {page && page.total > page.returned && (
            <span className="hint" style={{ marginLeft: 6, textTransform: "none", fontWeight: 400 }}>
              showing first {page.returned} of {page.total}
            </span>
          )}
        </div>
        <div className="panel-body">
          {caseId == null ? (
            <div className="empty" style={{ textAlign: "center", padding: "24px 16px" }}>
              <div style={{ fontSize: 28, marginBottom: 8 }}>📊</div>
              <p style={{ margin: "0 0 6px", fontWeight: 600, color: "var(--ink)" }}>No case selected</p>
              <p className="hint small" style={{ margin: "0 0 12px" }}>
                The timeline merges calls, SMS, WhatsApp, recordings, captures &amp; SIM data from a consolidated case.
              </p>
              <div style={{ display: "flex", gap: 8, justifyContent: "center" }}>
                <button className="primary" onClick={onGoToCase}>🗂 Go to Case step</button>
              </div>
            </div>
          ) : busy && !page ? (
            <div className="empty">Loading timeline…</div>
          ) : !page || page.events.length === 0 ? (
            <div className="empty" style={{ textAlign: "center", padding: "24px 16px" }}>
              <div style={{ fontSize: 28, marginBottom: 8 }}>🕓</div>
              <p style={{ margin: "0 0 6px", fontWeight: 600, color: "var(--ink)" }}>No events yet</p>
              <p className="hint small" style={{ margin: "0 0 12px" }}>
                Bundle evidence in the Case step — or widen your filters.
              </p>
              <div style={{ display: "flex", gap: 8, justifyContent: "center" }}>
                <button className="primary" onClick={onGoToCase}>📦 Bundle evidence</button>
                {page && <button onClick={reset}>↺ Reset filters</button>}
              </div>
            </div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              {page.events.map((e) => {
                const m = meta(e.kind);
                const sel = selected?.id === e.id;
                return (
                  <div
                    key={e.id}
                    onClick={() => setSelected(e)}
                    style={{
                      padding: "5px 8px",
                      borderRadius: 6,
                      cursor: "pointer",
                      border: "1px solid " + (sel ? "var(--saffron-deep)" : "var(--line)"),
                      background: sel ? "#fff3dd" : "var(--card)",
                    }}
                  >
                    <div style={{ display: "flex", gap: 8, alignItems: "baseline" }}>
                      <span className="nowrap small" style={{ color: "var(--muted)", minWidth: 132 }}>
                        {fmtTime(e.timestamp)}
                      </span>
                      <span title={m.label}>{m.icon}</span>
                      <b style={{ fontSize: 12 }}>{e.title}</b>
                    </div>
                    {e.detail && (
                      <div
                        className="small"
                        style={{ marginLeft: 152, color: "var(--ink)", whiteSpace: "pre-wrap", wordBreak: "break-word" }}
                      >
                        {truncate(e.detail, 220)}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      {/* RIGHT — inspector */}
      <div className="panel" style={{ gridArea: "detail" }}>
        <div className="panel-bar">🔍 Inspector</div>
        <div className="panel-body">
          <TimelineInspector event={selected} serial={serial} onJump={onJump} onStatus={onStatus} />
        </div>
      </div>
    </section>
  );
}

const dateStyle: CSSProperties = {
  flex: 1,
  minWidth: 120,
  border: "1px solid var(--line)",
  borderRadius: 6,
  padding: "4px 6px",
  background: "var(--card)",
  color: "var(--ink)",
  fontSize: 12,
};
