import type { WhatsAppDeepResult, RootCheckResult } from "../../types";

interface ConnectWhatsAppTabProps {
  serial: string;
  busy: boolean;
  waDeepResult: WhatsAppDeepResult | null;
  waDeepBusy: boolean;
  rootCheck: RootCheckResult | null;
  rootBusy: boolean;
  onWhatsappDeep: () => void;
  onCheckRoot: () => void;
  onWhatsappDowngrade: () => void;
  onImportTxt: () => void;
  onProvideKey: () => void;
}

export function ConnectWhatsAppTab({
  serial, busy, waDeepResult, waDeepBusy, rootCheck, rootBusy,
  onWhatsappDeep, onCheckRoot, onWhatsappDowngrade, onImportTxt, onProvideKey,
}: ConnectWhatsAppTabProps) {
  return (
    <div className="panel" style={{ gridArea: "left" }}>
      <div className="panel-bar">🔐 WhatsApp Deep Extraction</div>
      <div className="panel-body">
        <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
          <button className="primary" disabled={!serial || waDeepBusy || busy} onClick={onWhatsappDeep} style={{ flex: 1 }}>
            {waDeepBusy ? "Extracting…" : "🔓 Auto-Extract Chats"}
          </button>
          <button className="mini" disabled={!serial || rootBusy} onClick={onCheckRoot} title="Check if device has root access">
            {rootBusy ? "…" : "🔍 Root"}
          </button>
        </div>
        <button
          className="mini"
          disabled={!serial || waDeepBusy || busy}
          onClick={onWhatsappDowngrade}
          title="Downgrade WhatsApp to extract key + database (requires LegacyWhatsApp.apk)"
          style={{ width: "100%", marginBottom: 8, background: "rgba(0,120,255,0.08)", border: "1px solid rgba(0,120,255,0.2)" }}
        >
          ⬇️ Advanced: Downgrade & Extract (No Root)
        </button>

        {rootCheck && (
          <div style={{ marginBottom: 12, padding: 8, background: rootCheck.has_root ? "rgba(0,200,100,0.1)" : "rgba(255,100,0,0.1)", borderRadius: 6 }}>
            <div className="stats">
              <span className={`pill ${rootCheck.has_root ? "ok" : ""}`}>
                {rootCheck.has_root ? "✅ Root" : "❌ No Root"}
              </span>
              {rootCheck.has_magisk && <span className="pill ok">Magisk</span>}
              {rootCheck.has_root && rootCheck.can_read_whatsapp_key && <span className="pill ok">🔑 Key OK</span>}
              {rootCheck.has_root && !rootCheck.can_read_whatsapp_key && <span className="pill">🔑 No access</span>}
              {rootCheck.android_sdk && <span className="pill">SDK {rootCheck.android_sdk}</span>}
            </div>
            {rootCheck.details.length > 0 && (
              <div style={{ marginTop: 6, fontSize: 11, color: "var(--muted)" }}>
                {rootCheck.details.map((d, i) => <div key={i}>• {d}</div>)}
              </div>
            )}
            {!rootCheck.has_root && (
              <div style={{ marginTop: 10, padding: 8, background: "rgba(255,200,0,0.1)", borderRadius: 4, fontSize: 12 }}>
                <strong>⚠️ No root access</strong>
                <p style={{ margin: "6px 0 4px" }}>Tried all automatic methods. If you have the key file, provide it below:</p>
                <button className="mini" style={{ marginTop: 4 }} onClick={onImportTxt}>
                  📄 Import Chat Export (.txt)
                </button>
              </div>
            )}
            {rootCheck.has_root && !rootCheck.can_read_whatsapp_key && (
              <div style={{ marginTop: 10, padding: 8, background: "rgba(255,200,0,0.1)", borderRadius: 4, fontSize: 12 }}>
                <strong>⚠️ Root detected but WhatsApp key not accessible</strong>
                <p style={{ margin: "6px 0 0" }}>
                  Grant root access to ADB: Magisk → Settings → Superuser → ADB → Allow
                </p>
              </div>
            )}
          </div>
        )}

        <p className="hint small">
          Auto-extracts: run-as → root → local key → pull → ADB backup → decrypt → parse → chats.
          Downgrade method: legacy WA → backup → key+DB → restore (Android 4–13).
        </p>
        {waDeepResult && (
          <>
            <hr className="pdiv" />
            <div className="stats">
              <span className={`pill ${waDeepResult.key_obtained ? "ok" : ""}`}>
                {waDeepResult.key_obtained ? "🔑 Key obtained" : "🔒 No key"}
              </span>
              <span className={`pill ${waDeepResult.database_decrypted ? "ok" : ""}`}>
                {waDeepResult.database_decrypted ? "✅ Decrypted" : "❌ Not decrypted"}
              </span>
              {waDeepResult.database_decrypted && (
                <>
                  <span className="pill ok">{waDeepResult.total_messages} messages</span>
                  <span className="pill ok">{waDeepResult.total_contacts} chats</span>
                  <span className="pill ok">{waDeepResult.media_with_contacts.length} media</span>
                </>
              )}
            </div>
            {waDeepResult.key_method && (
              <p className="hint small" style={{ marginTop: 8 }}>Key method: {waDeepResult.key_method}</p>
            )}
            {waDeepResult.crypt_format && (
              <p className="hint small">Database format: {waDeepResult.crypt_format}</p>
            )}
            {waDeepResult.notes.length > 0 && (
              <div className="hint" style={{ marginTop: 8 }}>
                {waDeepResult.notes.map((n, i) => (
                  <p key={i} className={n.includes("failed") || n.includes("not") ? "err" : ""}>• {n}</p>
                ))}
              </div>
            )}
            {!waDeepResult.key_obtained && waDeepResult.crypt_file_path && (
              <div style={{ marginTop: 12, padding: 10, background: "rgba(100,150,255,0.1)", borderRadius: 6 }}>
                <p style={{ margin: "0 0 8px", fontSize: 12 }}>
                  <strong>💡 Have the key file?</strong> If you obtained the WhatsApp key from another source
                  (rooted device, backup extraction, etc.), you can provide it now to decrypt the database.
                </p>
                <button className="mini" onClick={onProvideKey} disabled={waDeepBusy}>
                  🔑 Provide Key File & Decrypt
                </button>
                <p style={{ margin: "6px 0 0", fontSize: 11, color: "var(--muted)" }}>
                  Encrypted DB: {waDeepResult.crypt_file_path.split('\\').pop()}
                </p>
              </div>
            )}
            {waDeepResult.chats.length > 0 && (
              <div style={{ marginTop: 12 }}>
                <p style={{ fontWeight: 500, marginBottom: 8 }}>Extracted Chats:</p>
                <div style={{ maxHeight: 200, overflowY: "auto", border: "1px solid var(--line)", borderRadius: 6, padding: 8 }}>
                  {waDeepResult.chats.slice(0, 20).map((chat, i) => (
                    <div key={i} style={{ padding: "4px 0", borderBottom: i < 19 ? "1px solid var(--line)" : "none" }}>
                      <strong>{chat.name || chat.jid}</strong>
                      <span style={{ marginLeft: 8, color: "var(--muted)" }}>
                        {chat.message_count} msgs
                      </span>
                      {chat.last_message && (
                        <p style={{ margin: "2px 0 0", fontSize: 12, color: "var(--muted)" }}>
                          {chat.last_message.slice(0, 50)}{chat.last_message.length > 50 ? "…" : ""}
                        </p>
                      )}
                    </div>
                  ))}
                  {waDeepResult.chats.length > 20 && (
                    <p style={{ textAlign: "center", color: "var(--muted)", marginTop: 8 }}>
                      + {waDeepResult.chats.length - 20} more chats
                    </p>
                  )}
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </div>
  );
}
