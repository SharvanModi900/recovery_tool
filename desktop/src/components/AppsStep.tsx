import { useState } from "react";
import type { AppInfo, AppInventory, UiRecovery, InstagramData, MessengerData, TelegramData, ForensicExtraction } from "../types";
import { AppIcon, WaEvidence, AppTable, appTag } from "./HelperComponents";
import { AppsPanel } from "./AppsPanel";
import type { ReactNode } from "react";

type AppExtractResult = InstagramData | MessengerData | TelegramData | ForensicExtraction;

interface AppsStepProps {
  serial: string;
  busy: boolean;
  appInventory: AppInventory | null;
  appInventoryBusy: boolean;
  apps: AppInfo[] | null;
  appsBusy: boolean;
  appFilter: string;
  showSystem: boolean;
  shownApps: AppInfo[];
  appSel: AppInfo | null;
  waStage: string;
  result: UiRecovery | null;
  appExtractResults: Map<string, AppExtractResult>;
  appExtractBusy: boolean;
  foregroundApp: string | null;
  screenMirror?: ReactNode;
  onDetectApps: () => void;
  onLoadApps: () => void;
  onOpenApp: (a: AppInfo) => void;
  onExtractApp: (appId: string) => void;
  onLaunchApp: (pkg: string) => Promise<void>;
  onFilterChange: (v: string) => void;
  onShowSystemChange: (v: boolean) => void;
  onReadWhatsApp: () => Promise<any>;
  onGoTo: (step: "scan") => void;
}

export function AppsStep({
  serial, busy, appInventory, appInventoryBusy, apps, appsBusy,
  appFilter, showSystem, shownApps, appSel, waStage, result,
  appExtractResults, appExtractBusy, foregroundApp, screenMirror,
  onDetectApps, onLoadApps, onOpenApp, onExtractApp, onLaunchApp, onFilterChange, onShowSystemChange,
  onReadWhatsApp, onGoTo,
}: AppsStepProps) {
  // Determine what to show in the right panel for the selected app
  const selectedAppResult = appSel ? appExtractResults.get(appIdForPackage(appSel.package)) : undefined;
  const [launching, setLaunching] = useState(false);

  return (
    <section className="workbench wb-apps">
      <div className="panel" style={{ gridArea: "strip", borderBottom: "1px solid #ddd" }}>
        <div className="panel-bar" style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span>💬 Messaging Apps Detection</span>
          <span className="hint">
            {appInventory
              ? `${appInventory.total_installed} installed · ${appInventory.total_with_data} with data`
              : "Scan for WhatsApp, Instagram, Snapchat, Messenger & more"}
          </span>
        </div>
        <div className="panel-body" style={{ padding: 0 }}>
          <AppsPanel
            inventory={appInventory}
            onDetect={onDetectApps}
            onExtract={onExtractApp}
            loading={appInventoryBusy || appExtractBusy}
            extractedIds={new Set(appExtractResults.keys())}
          />
        </div>
      </div>

      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {apps
              ? `${apps.length} apps · ${apps.filter((a) => !a.system).length} user-installed · ${apps.filter((a) => a.system).length} system`
              : appsBusy
              ? "reading the package list…"
              : "app inventory not loaded yet"}
          </span>
          <button className="mini" disabled={!serial || appsBusy} onClick={onLoadApps} title="Reload the installed app inventory">
            ⟳
          </button>
          {apps && (
            <button className="primary next" onClick={() => onGoTo("scan")}>
              Continue to Scan →
            </button>
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "left" }}>
        <div className="panel-bar">🎛 Filters</div>
        <div className="panel-body">
          <button className="primary" disabled={!serial || appsBusy} onClick={onLoadApps}>
            {appsBusy ? "Reading device…" : "📥 Load apps from device"}
          </button>
          <input
            type="text"
            placeholder="search package…"
            value={appFilter}
            onChange={(e) => onFilterChange(e.target.value)}
          />
          <label className="check">
            <input type="checkbox" checked={showSystem} onChange={(e) => onShowSystemChange(e.target.checked)} />
            show system apps
          </label>
          <hr className="pdiv" />
          <p className="hint">
            The inventory is read with read-only `pm` / `dumpsys` queries — nothing on the
            phone is installed, changed or removed. Click any app to inspect its full
            forensic configuration.
          </p>
          {appExtractResults.size > 0 && (
            <>
              <hr className="pdiv" />
              <p className="hint" style={{ fontWeight: 600 }}>✅ Extracted ({appExtractResults.size})</p>
              {Array.from(appExtractResults.keys()).map(id => (
                <span key={id} className="pill ok" style={{ fontSize: 10, padding: "2px 6px", marginRight: 4, marginBottom: 4, display: "inline-block" }}>
                  {id}
                </span>
              ))}
            </>
          )}
        </div>
      </div>

      {/* Screen mirror panel for interactive control */}
      {screenMirror && (
        <div style={{ gridArea: "mirror" }}>
          {screenMirror}
        </div>
      )}

      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">
          📦 Installed apps
          <span className="sep" />
          <span className="hint">{apps ? `${shownApps.length} shown` : ""}</span>
          {foregroundApp && (
            <>
              <span className="sep" />
              <span className="pill ok" style={{ fontSize: 9, padding: "0 6px" }}>
                ● Running: {foregroundApp}
              </span>
            </>
          )}
        </div>
        <div className="panel-body">
          {!apps ? (
            <div className="empty">
              {appsBusy ? "reading the package list…" : serial ? "press 📥 Load apps from device" : "select a device first"}
            </div>
          ) : (
            <AppTable apps={shownApps} serial={serial} sel={appSel?.package ?? ""} onOpen={onOpenApp} runningPkg={foregroundApp} />
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "right" }}>
        <div className="panel-bar">
          {appSel && /whatsapp/i.test(appSel.package) ? "📖 WhatsApp evidence" : selectedAppResult ? "🔍 Extraction Result" : "🔎 Configuration"}
        </div>
        <div className="panel-body">
          {!appSel ? (
            <div className="empty">select an app on the left</div>
          ) : (
            <>
              <div className="app-insp-head">
                <AppIcon pkg={appSel.package} serial={serial} big />
                <b>{appSel.package}</b>
              </div>
              {/whatsapp/i.test(appSel.package) ? (
                <div className="wa-panel">
                  {waStage ? (
                    <div className="empty">
                      <span className="spin" style={{ marginRight: 7 }} />
                      {waStage}
                    </div>
                  ) : result ? (
                    <WaEvidence res={result} />
                  ) : (
                    <div className="empty">
                      Reads WhatsApp's chats &amp; contacts from the phone through the
                      read-only pipeline.
                      <br />
                      <button
                        className="primary"
                        style={{ marginTop: 10 }}
                        disabled={busy}
                        onClick={() => onReadWhatsApp().finally(() => {})}
                      >
                        📖 Read chats &amp; contacts
                      </button>
                    </div>
                  )}
                  <button
                    className="primary"
                    style={{ marginTop: 6, width: "100%", background: "var(--info, #3b82f6)", borderColor: "var(--info, #3b82f6)" }}
                    disabled={launching || !serial}
                    onClick={async () => {
                      setLaunching(true);
                      try { await onLaunchApp(appSel.package); } finally { setLaunching(false); }
                    }}
                  >
                    {launching ? "🚀 Launching…" : "🚀 Launch on Device"}
                  </button>
                </div>
              ) : selectedAppResult ? (
                <ExtractionResultView appId={appIdForPackage(appSel.package)} data={selectedAppResult} />
              ) : (
                <>
                  <div className="mirror-stats">
                    <div><span>Version</span><b>{appSel.version_name ?? "—"}</b></div>
                    <div><span>Version code</span><b>{appSel.version_code ?? "…"}</b></div>
                    <div><span>Target SDK</span><b>{appSel.target_sdk ?? "…"}</b></div>
                    <div><span>First install</span><b>{appSel.first_install || "…"}</b></div>
                    <div><span>Last update</span><b>{appSel.last_update || "…"}</b></div>
                    <div><span>Perms granted</span><b>{appSel.perms}</b></div>
                    <div><span>Type</span><b>{appSel.system ? "system" : "user"}</b></div>
                  </div>
                  <p className="hint small">Data directory</p>
                  <p className="mono small">{appSel.data_dir ?? "…"}</p>
                  <p className="hint small">APK path</p>
                  <p className="mono small">{appSel.apk_path}</p>
                  <p className="hint small">Signing certificate</p>
                  <p className="mono small">{appSel.signer ?? "…"}</p>
                  {appTag(appSel.package) && (
                    <p className="hint lock-warn">
                      🔍 Forensic interest: {appTag(appSel.package)} — include this app's data in
                      the Scan &amp; Recover steps.
                    </p>
                  )}
                  {appInventory?.apps.find(a => a.id === appIdForPackage(appSel.package) && a.has_data) && (
                    <button
                      className="primary"
                      style={{ marginTop: 10, width: "100%" }}
                      disabled={appExtractBusy || !serial}
                      onClick={() => onExtractApp(appIdForPackage(appSel.package))}
                    >
                      {appExtractBusy ? "⏳ Extracting…" : "🔍 Extract Forensic Data"}
                    </button>
                  )}
                  <button
                    className="primary"
                    style={{ marginTop: 6, width: "100%", background: "var(--info, #3b82f6)", borderColor: "var(--info, #3b82f6)" }}
                    disabled={launching || !serial}
                    onClick={async () => {
                      setLaunching(true);
                      try { await onLaunchApp(appSel.package); } finally { setLaunching(false); }
                    }}
                  >
                    {launching ? "🚀 Launching…" : "🚀 Launch on Device"}
                  </button>
                </>
              )}
            </>
          )}
        </div>
      </div>
    </section>
  );
}

/** Map a package name to the app ID used by the extraction backend. */
function appIdForPackage(pkg: string): string {
  const map: Record<string, string> = {
    "com.whatsapp": "whatsapp",
    "com.instagram.android": "instagram",
    "com.snapchat.android": "snapchat",
    "com.facebook.orca": "messenger",
    "org.telegram.messenger": "telegram",
    "org.thoughtcrime.securesms": "signal",
    "com.viber.voip": "viber",
    "jp.naver.line.android": "line",
    "com.tencent.mm": "wechat",
    "com.skype.raider": "skype",
    "com.discord": "discord",
    "com.twitter.android": "twitter",
    "com.zhiliaoapp.musically": "tiktok",
    "ch.threema.app": "threema",
  };
  return map[pkg] ?? pkg.replace(/^com\./, "").replace(/\./g, "_");
}

/** Render extraction results for Instagram, Messenger, Telegram, or generic forensic. */
function ExtractionResultView({ appId, data }: { appId: string; data: AppExtractResult }) {
  // Instagram
  if ("media" in data && "has_database" in data && !("carved_messages" in data)) {
    const d = data as InstagramData | MessengerData | TelegramData;
    return (
      <div>
        <div className="mirror-stats">
          <div><span>App</span><b style={{ textTransform: "capitalize" }}>{appId}</b></div>
          <div><span>Media files</span><b>{d.media.length}</b></div>
          <div><span>Database found</span><b>{d.has_database ? "✅ Yes" : "❌ No"}</b></div>
          <div><span>Database accessible</span><b>{d.database_accessible ? "✅ Yes" : "🔒 No"}</b></div>
        </div>
        {d.media.length > 0 && (
          <>
            <p className="hint small" style={{ marginTop: 8 }}>Media files</p>
            <div className="ov-scroll" style={{ maxHeight: 200 }}>
              <table className="fx-table">
                <thead><tr><th>Filename</th><th>Type</th><th>Size</th></tr></thead>
                <tbody>
                  {d.media.map((m, i) => (
                    <tr key={i}>
                      <td className="mono" style={{ fontSize: 10 }}>{m.filename}</td>
                      <td>{m.media_type}</td>
                      <td>{m.size > 0 ? `${(m.size / 1024).toFixed(0)} KB` : "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
        {d.media.length === 0 && (
          <p className="hint small" style={{ marginTop: 8 }}>No media files found on the device.</p>
        )}
      </div>
    );
  }

  // Generic ForensicExtraction
  if ("carved_messages" in data) {
    const d = data as ForensicExtraction;
    return (
      <div>
        <div className="mirror-stats">
          <div><span>App</span><b>{d.app_name}</b></div>
          <div><span>Media files</span><b>{d.media_files.length}</b></div>
          <div><span>Carved messages</span><b>{d.carved_messages.length}</b></div>
          <div><span>Databases pulled</span><b>{d.databases_pulled.length}</b></div>
        </div>
        {d.databases_found.length > 0 && (
          <>
            <p className="hint small" style={{ marginTop: 8 }}>Databases found</p>
            {d.databases_found.map((db, i) => (
              <div key={i} className="mono small" style={{ color: "var(--info)", padding: "1px 0" }}>{db}</div>
            ))}
          </>
        )}
        {d.carved_messages.length > 0 && (
          <>
            <p className="hint small" style={{ marginTop: 8 }}>Carved messages (preview)</p>
            <div className="ov-scroll" style={{ maxHeight: 180 }}>
              <table className="fx-table">
                <thead><tr><th>Chat</th><th>Direction</th><th>Text</th></tr></thead>
                <tbody>
                  {d.carved_messages.slice(0, 50).map((m, i) => (
                    <tr key={i}>
                      <td className="mono" style={{ fontSize: 10 }}>{m.chat_jid || m.sender || "—"}</td>
                      <td>{m.from_me ? "→ Sent" : "← Recv"}</td>
                      <td style={{ maxWidth: 250, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{m.text || "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </>
        )}
        {d.extraction_notes.length > 0 && (
          <>
            <p className="hint small" style={{ marginTop: 8 }}>Notes</p>
            {d.extraction_notes.map((n, i) => (
              <p key={i} className="hint small" style={{ color: "var(--text-secondary)" }}>• {n}</p>
            ))}
          </>
        )}
      </div>
    );
  }

  return <div className="empty">Extraction complete — no detailed view available for this app type.</div>;
}
