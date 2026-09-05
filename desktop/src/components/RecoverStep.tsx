import { open } from "@tauri-apps/plugin-dialog";
import type { UiRecovery, ScanReport } from "../types";
import { fmtTime } from "../types";

interface RecoverStepProps {
  serial: string;
  busy: boolean;
  dbPath: string;
  cryptPath: string;
  keyPath: string;
  deep: boolean;
  result: UiRecovery | null;
  filter: string;
  onlyDeleted: boolean;
  decryptNote: string;
  filtered: UiRecovery["messages"];
  dbArtifacts: NonNullable<ScanReport["artifacts"]>;
  cryptArtifacts: NonNullable<ScanReport["artifacts"]>;
  nameOf: (jid: string | null) => string;
  onDbPathChange: (v: string) => void;
  onCryptPathChange: (v: string) => void;
  onKeyPathChange: (v: string) => void;
  onDeepChange: (v: boolean) => void;
  onFilterChange: (v: string) => void;
  onOnlyDeletedChange: (v: boolean) => void;
  onRecover: () => void;
  onDecrypt: () => void;
  onImportTxt: () => void;
  onImportSms: () => void;
  onImportVcf: () => void;
  onClearDemo: () => void;
  onGoTo: (step: "scan" | "export") => void;
  onFetchKey: () => void;
}

export function RecoverStep({
  serial, busy, dbPath, cryptPath, keyPath, deep, result, filter, onlyDeleted,
  decryptNote, filtered, dbArtifacts, cryptArtifacts, nameOf,
  onDbPathChange, onCryptPathChange, onKeyPathChange, onDeepChange,
  onFilterChange, onOnlyDeletedChange, onRecover, onDecrypt,
  onImportTxt, onImportSms, onImportVcf, onClearDemo, onGoTo, onFetchKey,
}: RecoverStepProps) {
  return (
    <section className="workbench wb-recover">
      {/* LEFT — evidence sources */}
      <div className="panel" style={{ gridArea: "left" }}>
        <div className="panel-bar">🗄 Sources</div>
        <div className="panel-body">
          <div className="field">
            <label>Database (msgstore.db)</label>
            <div className="row">
              <input className="mono" value={dbPath} onChange={(e) => onDbPathChange(e.target.value)} placeholder="path to msgstore.db" />
              <select value={dbPath} onChange={(e) => onDbPathChange(e.target.value)} disabled={dbArtifacts.length === 0}>
                <option value="">— found in scan —</option>
                {dbArtifacts.map((a) => (
                  <option key={a.path} value={a.path}>{a.path.split(/[\\/]/).pop()}</option>
                ))}
              </select>
              <button onClick={async () => {
                const f = await open({ filters: [{ name: "SQLite", extensions: ["db"] }] });
                if (f) onDbPathChange(f as string);
              }}>📄…</button>
            </div>
            {!dbPath && (
              <p className="hint">
                👉 No database selected yet — go to <b>② Scan</b> and use <b>📂 Import folder…</b> to load a
                WhatsApp backup folder (the path auto-fills), or click <b>📄…</b> to browse a msgstore.db manually.
                Then press <b>▶ Run recovery</b> below.
              </p>
            )}
            {!dbPath && (
              <button className="mini" style={{ marginTop: 4 }} onClick={() => onGoTo("scan")}>Go to ② Scan →</button>
            )}
          </div>

          <div className="field sub">
            <label>💬 No root? Import evidence files instead — works for every app that can export</label>
            <div className="row">
              <button className="primary" disabled={busy} onClick={onImportTxt}>
                💬 WhatsApp chat (.txt) …
              </button>
              <button className="primary" disabled={busy} onClick={onImportSms}>
                📨 SMS / MMS (.xml) …
              </button>
              <button className="primary" disabled={busy} onClick={onImportVcf}>
                📇 Contacts (.vcf) …
              </button>
            </div>
            <p className="hint">
              <b>WhatsApp:</b> chat → ⋯ → More → Export chat → Without media · <b>SMS:</b> install the free
              "SMS Backup &amp; Restore" app → Back up → copy the .xml here · <b>Contacts:</b> Contacts app →
              ⋯ → Export → .vcf. All real data, <b>no key and no root needed</b>. (Deleted items are not part of exports.)
            </p>
          </div>

          {(cryptArtifacts.length > 0 || cryptPath) && (
            <div className="field sub">
              <label>🔒 Encrypted backup (crypt12/14/15) + device key</label>
              <div className="row">
                <select value={cryptPath} onChange={(e) => onCryptPathChange(e.target.value)}>
                  <option value="">— encrypted backup —</option>
                  {cryptArtifacts.map((a) => (
                    <option key={a.path} value={a.path}>{a.path.split(/[\\/]/).pop()}</option>
                  ))}
                </select>
                <input className="mono" value={keyPath} onChange={(e) => onKeyPathChange(e.target.value)} placeholder="key file path — paste it here, or use Browse key" />
                <button onClick={async () => {
                  const f = await open({ title: "Choose WhatsApp key file (usually a 158-byte file named 'key')" });
                  if (f) onKeyPathChange(f as string);
                }}>🗝 Browse key…</button>
                <button
                  disabled={!serial || busy}
                  title="Requires a rooted phone: reads /data/data/com.whatsapp/files/key via su"
                  onClick={onFetchKey}
                >
                  📥 Fetch key (root)
                </button>
                <button className="primary" disabled={!cryptPath || !keyPath || busy} onClick={onDecrypt}>
                  Decrypt
                </button>
              </div>
              <p className="hint">
                crypt12/14/15 backups need WhatsApp's key file (normally <span className="mono">/data/data/com.whatsapp/files/key</span>).
                On a rooted phone use <b>📥 Fetch key</b>; otherwise browse a previously extracted key with 🗝.
                Pick the latest <b>full</b> backup (<span className="mono">msgstore-YYYY-MM-DD…crypt14</span>) —
                <span className="mono">-increment-</span> files are only deltas and can't be recovered on their own.
                Files still on the phone are pulled automatically.
              </p>
              {decryptNote && <p className="hint ok">{decryptNote}</p>}
            </div>
          )}

          <div className="row">
            <label className="check">
              <input type="checkbox" checked={deep} onChange={(e) => onDeepChange(e.target.checked)} />
              deep recovery — carve deleted records (freeblocks, freelist, WAL)
            </label>
            <button className="primary" disabled={!dbPath || busy} onClick={onRecover}>
              ▶ Run recovery
            </button>
          </div>
        </div>
      </div>

      {/* CENTER — recovered evidence */}
      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">
          💬 Evidence
          <span className="sep" />
          <span className="hint">{result ? `${filtered.length} shown` : "run recovery or import files"}</span>
        </div>
        <div className="panel-body">
          {result && (
            <>
              <div className={result.db_path.toLowerCase().includes("fixture") ? "source demo-src" : "source real-src"}>
                {result.db_path.toLowerCase().includes("fixture") ? (
                  <>
                    🎭 <b>DEMO DATA</b> — sample fixture, not from your phone.
                    <button onClick={onClearDemo}>
                      🧹 Clear demo — start with real data
                    </button>
                  </>
                ) : (
                  <>📱 <b>REAL DATA</b> — recovered from your phone's database</>
                )}
                <span className="mono small" title={result.db_path}>source: {result.db_path.split(/[\\/]/).pop()}</span>
              </div>
              <div className="stats">
                <span className="pill ok">{result.total_messages} messages</span>
                <span className={result.recovered_deleted > 0 ? "pill hot" : "pill"}>
                  {result.recovered_deleted} recovered deleted
                </span>
                <span className="pill">{result.chats} chats</span>
                <span className="pill">{result.contacts} contacts</span>
                <span className="pill">{result.media} media refs</span>
                <span className="pill">schema: {result.schema}</span>
              </div>

              {result.contacts_list.length > 0 && result.messages.length === 0 && (
                <div className="scroll big">
                  <table className="grid">
                    <thead>
                      <tr>
                        <th>Name</th>
                        <th>Number</th>
                      </tr>
                    </thead>
                    <tbody>
                      {result.contacts_list.map(([cname, num], i) => (
                        <tr key={i}>
                          <td>{cname}</td>
                          <td className="mono">{num}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              <div className="row">
                <input
                  placeholder="🔎 filter messages…"
                  value={filter}
                  onChange={(e) => onFilterChange(e.target.value)}
                />
                <label className="check">
                  <input type="checkbox" checked={onlyDeleted} onChange={(e) => onOnlyDeletedChange(e.target.checked)} />
                  deleted only
                </label>
                <span className="hint">{filtered.length} shown{result.messages_hidden > 0 ? ` · ${result.messages_hidden} held back for export` : ""}</span>
              </div>

              <div className="scroll big">
                <table className="grid">
                  <thead>
                    <tr>
                      <th>Time</th>
                      <th>Chat</th>
                      <th>Dir</th>
                      <th>Message</th>
                      <th>Source</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filtered.map((m, i) => (
                      <tr key={i} className={m.source !== "live" ? "deleted" : ""}>
                        <td className="nowrap">{fmtTime(m.timestamp)}</td>
                        <td className="small" title={m.chat_jid ?? undefined}>
                          {nameOf(m.chat_jid) || <span className="mono">{m.chat_jid ?? "—"}</span>}
                        </td>
                        <td>{m.from_me ? "↗" : "↙"}</td>
                        <td className="text">{m.text ?? <i className="hint">no text</i>}</td>
                        <td>
                          <span className={`pill ${m.source === "freelist" ? "hot" : m.source === "wal" ? "warn" : "ok"}`}>
                            {m.source === "freelist" ? "deleted" : m.source}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
        </div>
      </div>

      {/* RIGHT — analysis panel */}
      <div className="panel" style={{ gridArea: "right" }}>
        <div className="panel-bar">📈 Analysis</div>
        <div className="panel-body">
          {result ? (
            <div className="insights col">
              <div className="insight">
                <h3>📊 Activity</h3>
                <p className="hint">
                  {fmtTime(result.analysis.first_ts)} → {fmtTime(result.analysis.last_ts)}
                  <br />
                  outgoing share: <b>{Math.round(result.analysis.from_me_ratio * 100)}%</b>
                  {result.recovered_deleted > 0 && (
                    <> · deleted recovered: <b>{result.recovered_deleted}</b></>
                  )}
                </p>
                <div className="spark" title="messages per day">
                  {result.analysis.per_day.slice(-30).map(([day, n]) => {
                    const max = Math.max(...result.analysis.per_day.map(([, c]) => c), 1);
                    return (
                      <span key={day} className="spark-bar" style={{ height: `${Math.max(8, (n / max) * 100)}%` }} title={`${day}: ${n}`} />
                    );
                  })}
                </div>
              </div>
              {result.analysis.top_chats.length > 0 && (
                <div className="insight">
                  <h3>💬 Top chats</h3>
                  {result.analysis.top_chats.map(([jid, name, n]) => (
                    <div key={jid} className="toprow">
                      <span className="name">{name || jid}</span>
                      <span className="bar">
                        <i style={{ width: `${(n / result.analysis.top_chats[0][2]) * 100}%` }} />
                      </span>
                      <span className="count">{n}</span>
                    </div>
                  ))}
                </div>
              )}
              <div className="insight">
                <h3>🛡 Evidence integrity</h3>
                <p className="hint">
                  SHA-256 of source database
                  <br />
                  <span className="mono small" title={result.sha256}>
                    {result.sha256 ? `${result.sha256.slice(0, 24)}…` : "n/a"}
                  </span>
                  <br />
                  Included in every export — proof the data is unmodified.
                </p>
              </div>
            </div>
          ) : (
            <p className="hint">Analytics (activity chart, top chats, evidence hash) appear here after recovery.</p>
          )}
        </div>
      </div>

      {/* TOP — strip */}
      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {result
              ? `${result.total_messages} messages · ${result.recovered_deleted} deleted recovered · ${result.chats} chats · ${result.contacts} contacts · ${result.media} media refs`
              : "no recovery run yet — pick a source on the left"}
          </span>
          {result && (
            <button className="primary next" onClick={() => onGoTo("export")}>
              Continue to Export →
            </button>
          )}
        </div>
      </div>
    </section>
  );
}

