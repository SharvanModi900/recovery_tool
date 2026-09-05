import type { AppInfo, AppInventory, UiRecovery } from "../types";
import { AppIcon, WaEvidence, AppTable, appTag } from "./HelperComponents";
import { AppsPanel } from "./AppsPanel";

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
  onDetectApps: () => void;
  onLoadApps: () => void;
  onOpenApp: (a: AppInfo) => void;
  onFilterChange: (v: string) => void;
  onShowSystemChange: (v: boolean) => void;
  onReadWhatsApp: () => Promise<any>;
  onGoTo: (step: "scan") => void;
}

export function AppsStep({
  serial, busy, appInventory, appInventoryBusy, apps, appsBusy,
  appFilter, showSystem, shownApps, appSel, waStage, result,
  onDetectApps, onLoadApps, onOpenApp, onFilterChange, onShowSystemChange,
  onReadWhatsApp, onGoTo,
}: AppsStepProps) {
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
            onExtract={(_appId: string) => {
              // TODO: Implement per-app extraction
            }}
            loading={appInventoryBusy}
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
        </div>
      </div>

      <div className="panel" style={{ gridArea: "main" }}>
        <div className="panel-bar">
          📦 Installed apps
          <span className="sep" />
          <span className="hint">{apps ? `${shownApps.length} shown` : ""}</span>
        </div>
        <div className="panel-body">
          {!apps ? (
            <div className="empty">
              {appsBusy ? "reading the package list…" : serial ? "press 📥 Load apps from device" : "select a device first"}
            </div>
          ) : (
            <AppTable apps={shownApps} serial={serial} sel={appSel?.package ?? ""} onOpen={onOpenApp} />
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "right" }}>
        <div className="panel-bar">
          {appSel && /whatsapp/i.test(appSel.package) ? "📖 WhatsApp evidence" : "🔎 Configuration"}
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
                </div>
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
                      the Scan & Recover steps.
                    </p>
                  )}
                </>
              )}
            </>
          )}
        </div>
      </div>
    </section>
  );
}
