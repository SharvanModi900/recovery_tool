import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openPath, revealItemInDir } from "@tauri-apps/plugin-opener";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import type { AppInfo, Device, DeviceMirror, MediaFile, MediaInspect, MediaRecovery, ScanReport, UiRecovery, DirectAcquisition, AppInventory, WhatsAppDeepResult, RootCheckResult, CallEvent } from "./types";
import { fmtSize, fmtTime } from "./types";
import { DirectDataViewer, AppsPanel } from "./components";

// Per-device session state for multi-device support
export interface DeviceSession {
  serial: string;
  model: string;
  // Device mirror
  mirror: DeviceMirror | null;
  shot: string;
  shotTs: number;
  shotDark: boolean;
  mirrorErr: string;
  autoRefresh: boolean;
  mirrorBusy: boolean;
  liveStream: boolean;
  streamFps: number;
  streamMode: string;
  isVideoFrame: boolean;
  // Scan
  report: ScanReport | null;
  includeMedia: boolean;
  // Recover
  dbPath: string;
  cryptPath: string;
  keyPath: string;
  decryptNote: string;
  recovery: UiRecovery | null;
  recoveryBusy: boolean;
  // Media
  media: MediaRecovery | null;
  mediaBusy: boolean;
  mediaSource: string;
  preview: string | null;
  inspect: { size: number; sha256: string; format: string; valid: boolean } | null;
  selected: string | null;
  lostSet: Set<string> | null;
  // Apps
  apps: AppInfo[];
  appsBusy: boolean;
  selectedApp: string | null;
  appDetails: AppInfo | null;
  appIcon: string | null;
  waRecovery: UiRecovery | null;
  waBusy: boolean;
  waTab: "chats" | "contacts";
  // Direct acquisition
  directData: DirectAcquisition | null;
  directBusy: boolean;
  // Export
  exportBusy: boolean;
}

const createEmptySession = (serial: string, model: string = ""): DeviceSession => ({
  serial,
  model,
  mirror: null,
  shot: "",
  shotTs: 0,
  shotDark: false,
  mirrorErr: "",
  autoRefresh: true,
  mirrorBusy: false,
  liveStream: false,
  streamFps: 15,
  streamMode: "normal",
  isVideoFrame: false,
  report: null,
  includeMedia: false,
  dbPath: "",
  cryptPath: "",
  keyPath: "",
  decryptNote: "",
  recovery: null,
  recoveryBusy: false,
  media: null,
  mediaBusy: false,
  mediaSource: "whatsapp",
  preview: null,
  inspect: null,
  selected: null,
  lostSet: null,
  apps: [],
  appsBusy: false,
  selectedApp: null,
  appDetails: null,
  appIcon: null,
  waRecovery: null,
  waBusy: false,
  waTab: "chats",
  directData: null,
  directBusy: false,
  exportBusy: false,
});

type Step = "connect" | "apps" | "scan" | "media" | "recover" | "export";

const STEPS: { id: Step; label: string; icon: string }[] = [
  { id: "connect", label: "Connect", icon: "①" },
  { id: "apps", label: "Apps", icon: "📦" },
  { id: "scan", label: "Scan", icon: "②" },
  { id: "media", label: "Media", icon: "🖼" },
  { id: "recover", label: "Recover", icon: "③" },
  { id: "export", label: "Export", icon: "④" },
];

/**
 * Memoized: with 2000+ tiles, re-rendering the grid on every preview click
 * froze the UI. Only the two tiles whose selection changed now re-render.
 * `presence` is only passed once a device check ran: "" = unknown,
 * "device" = still on the phone, "lost" = local copy is the only evidence.
 */
const MediaTile = memo(function MediaTile({
  f,
  onOpen,
  selected,
  presence,
}: {
  f: MediaFile;
  onOpen: (f: MediaFile) => void;
  selected: boolean;
  presence?: string;
}) {
  const [broken, setBroken] = useState(false);
  const click = useCallback(() => onOpen(f), [f, onOpen]);
  if (f.kind === "video" || broken) {
    return (
      <div className={`tile${selected ? " sel" : ""}`} title={`${f.path}\nClick to inspect & verify`} onClick={click}>
        <span className="tile-icon">{f.kind === "video" ? "🎬" : "🖼"}</span>
        <span className="tile-name mono small">{f.path.split(/[\\/]/).pop()}</span>
        <span className="hint small">{fmtSize(f.size)}</span>
        {presence && <span className={`presence ${presence}`}>{presence === "lost" ? "✗ lost" : "✓"}</span>}
      </div>
    );
  }
  return (
    <div className={`tile${selected ? " sel" : ""}`} title={`${f.path}\nClick to inspect & verify`} onClick={click}>
      <img src={convertFileSrc(f.path)} loading="lazy" alt="" onError={() => setBroken(true)} />
      <span className="tile-name mono small">{f.path.split(/[\\/]/).pop()}</span>
      {presence && <span className={`presence ${presence}`}>{presence === "lost" ? "✗ lost" : "✓"}</span>}
    </div>
  );
});

/** The whole gallery, isolated from the rest of the app's re-renders. */
const MediaGrid = memo(function MediaGrid({
  files,
  selectedPath,
  onOpen,
  lost,
}: {
  files: MediaFile[];
  selectedPath: string;
  onOpen: (f: MediaFile) => void;
  lost: Set<string> | null;
}) {
  return (
    <div className="media-grid">
      {files.map((f) => (
        <MediaTile
          key={f.path}
          f={f}
          selected={selectedPath === f.path}
          onOpen={onOpen}
          presence={lost ? (lost.has(f.path) ? "lost" : "device") : undefined}
        />
      ))}
    </div>
  );
});

/** Icon cache: package → asset URL, or "none" when the app ships no bitmap icon. */
const iconCache = new Map<string, string>();

/** Deterministic placeholder color per package. */
function iconColor(pkg: string): string {
  let h = 0;
  for (let i = 0; i < pkg.length; i++) h = (h * 31 + pkg.charCodeAt(i)) >>> 0;
  return `hsl(${h % 360} 45% 48%)`;
}

/**
 * The real launcher icon, lazily extracted from the on-device APK when the
 * row scrolls into view. Shows a monogram until the bytes arrive.
 */
function AppIcon({ pkg, serial, big }: { pkg: string; serial: string; big?: boolean }) {
  const cached = iconCache.get(pkg);
  const [url, setUrl] = useState(cached && cached !== "none" ? cached : "");
  const [none, setNone] = useState(cached === "none");
  const ref = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (url || none || !serial) return;
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        io.disconnect();
        api
          .appIcon(serial, pkg)
          .then((r) => {
            const u = convertFileSrc(r.path);
            iconCache.set(pkg, u);
            setUrl(u);
          })
          .catch(() => {
            iconCache.set(pkg, "none");
            setNone(true);
          });
      },
      { rootMargin: "300px" }
    );
    io.observe(el);
    return () => io.disconnect();
  }, [pkg, serial, url, none]);

  if (url) return <img className={`app-icon${big ? " big" : ""}`} src={url} alt="" />;
  const letters = (pkg.split(".").pop() ?? pkg).slice(0, 2).toUpperCase();
  return (
    <span ref={ref} className={`app-icon mono${big ? " big" : ""}`} style={{ background: iconColor(pkg) }}>
      {letters}
    </span>
  );
}

const ROW_H = 26;

/** Contacts + chats viewer for a loaded WhatsApp evidence database. */
function WaEvidence({ res }: { res: UiRecovery }) {
  const [tab, setTab] = useState<"contacts" | "chats">("chats");
  const nameOf = (jid: string) =>
    res.names.find(([j]) => j === jid)?.[1] || res.names.find(([j]) => j === jid.split("@")[0])?.[1] || jid.split("@")[0];
  const topChats = res.analysis.top_chats; // [jid, name, count]
  return (
    <>
      <div className="grid-3">
        <div className="stat">
          <b>{res.chats}</b>
          <span>chats</span>
        </div>
        <div className="stat">
          <b>{res.contacts}</b>
          <span>contacts</span>
        </div>
        <div className="stat">
          <b>{res.total_messages.toLocaleString()}</b>
          <span>messages</span>
        </div>
      </div>
      {res.recovered_deleted > 0 && (
        <p className="hint lock-warn" style={{ marginTop: 6 }}>
          ⚠ {res.recovered_deleted} deleted message{res.recovered_deleted === 1 ? "" : "s"} carved from the
          database.
        </p>
      )}
      <div className="legend" style={{ marginTop: 8 }}>
        <span className={`lg-tab${tab === "chats" ? " active" : ""}`} onClick={() => setTab("chats")}>
          Chats
        </span>
        <span className={`lg-tab${tab === "contacts" ? " active" : ""}`} onClick={() => setTab("contacts")}>
          Contacts
        </span>
      </div>
      {tab === "contacts" ? (
        res.contacts_list.length ? (
          <div className="wa-list">
            {res.contacts_list.map(([name, num], i) => (
              <div className="wa-item" key={i}>
                <b>{name || num}</b>
                {name && <span className="small">{num}</span>}
              </div>
            ))}
          </div>
        ) : (
          <div className="empty">No contact book entries in this database.</div>
        )
      ) : res.messages.length ? (
        <>
          {topChats.length > 0 && (
            <div className="legend" style={{ justifyContent: "flex-start", flexWrap: "wrap" }}>
              {topChats.slice(0, 6).map(([jid, name, n]) => (
                <span className="pill kind-media" key={jid}>
                  {name || jid.split("@")[0]} · {n}
                </span>
              ))}
            </div>
          )}
          <div className="wa-list">
            {res.messages.slice(0, 600).map((m) => (
              <div className="wa-item" key={m.row_id ?? `${m.chat_jid}-${m.timestamp}`}>
                <div className="wa-item-head">
                  <b>
                    {m.from_me ? "→ " : ""}
                    {m.chat_jid ? nameOf(m.chat_jid) : m.sender ?? "unknown"}
                  </b>
                  <span className="wa-ts">{m.timestamp ? new Date(m.timestamp).toLocaleString() : "?"}</span>
                </div>
                <span className="wa-body">
                  {m.text || (m.source !== "live" ? `(carved · ${m.source})` : "")}
                </span>
              </div>
            ))}
          </div>
          {res.messages.length > 600 && (
            <span className="small" style={{ display: "block", textAlign: "center" }}>
              first 600 shown — open ③ Recover for all {res.messages.length.toLocaleString()}
            </span>
          )}
        </>
      ) : (
        <div className="empty">No messages parsed from this database.</div>
      )}
    </>
  );
}

/** Renders only the visible rows so a 300+ app inventory never stalls the UI. */
function AppTable({
  apps,
  serial,
  sel,
  onOpen,
}: {
  apps: AppInfo[];
  serial: string;
  sel: string;
  onOpen: (a: AppInfo) => void;
}) {
  const [scrollTop, setScrollTop] = useState(0);
  const [viewH, setViewH] = useState(400);
  const boxRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = boxRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewH(el.clientHeight));
    ro.observe(el);
    setViewH(el.clientHeight);
    return () => ro.disconnect();
  }, []);

  // New list (device switch / filter) starts at the top
  useEffect(() => {
    boxRef.current?.scrollTo({ top: 0 });
    setScrollTop(0);
  }, [apps]);

  const start = Math.max(0, Math.floor(scrollTop / ROW_H) - 6);
  const end = Math.min(apps.length, Math.ceil((scrollTop + viewH) / ROW_H) + 6);

  return (
    <div className="app-table-box" ref={boxRef} onScroll={(e) => setScrollTop(e.currentTarget.scrollTop)}>
      <table className="grid app-table">
        <thead>
          <tr>
            <th></th>
            <th>Package</th>
            <th>Version</th>
            <th>Installed</th>
            <th>Updated</th>
            <th>Type</th>
            <th>Evidence</th>
          </tr>
        </thead>
        {start > 0 && (
          <tbody>
            <tr style={{ height: start * ROW_H }}>
              <td colSpan={7} />
            </tr>
          </tbody>
        )}
        <tbody>
          {apps.slice(start, end).map((a) => (
            <tr
              key={a.package}
              style={{ height: ROW_H }}
              className={sel === a.package ? "sel" : ""}
              onClick={() => onOpen(a)}
            >
              <td>
                <AppIcon pkg={a.package} serial={serial} />
              </td>
              <td className="mono small">{a.package}</td>
              <td className="small nowrap">{a.version_name ?? "…"}</td>
              <td className="small nowrap">{a.first_install || "…"}</td>
              <td className="small nowrap">{a.last_update || "…"}</td>
              <td className="small">{a.system ? "system" : "user"}</td>
              <td>{appTag(a.package) && <span className="pill kind-media">{appTag(a.package)}</span>}</td>
            </tr>
          ))}
        </tbody>
        {end < apps.length && (
          <tbody>
            <tr style={{ height: (apps.length - end) * ROW_H }}>
              <td colSpan={7} />
            </tr>
          </tbody>
        )}
      </table>
    </div>
  );
}

/** Forensic relevance tag derived from the package name alone. */
function appTag(pkg: string): string {
  if (/whatsapp/i.test(pkg)) return "chat evidence";
  if (/telegram|signal|messenger|instagram|facebook|snapchat/i.test(pkg)) return "social";
  if (/messaging|\bsms\b|mms/i.test(pkg)) return "SMS";
  if (/contacts|dialer|\bphone\b/i.test(pkg)) return "contacts/calls";
  if (/chrome|browser|firefox/i.test(pkg)) return "browser";
  if (/gmail|\bmail\b|email/i.test(pkg)) return "email";
  if (/gallery|camera|photos/i.test(pkg)) return "media";
  if (/bank|\bpay\b|wallet|paytm|gpay|upi/i.test(pkg)) return "financial";
  return "";
}

export default function App() {
  const [step, setStep] = useState<Step>("connect");

  // Multi-device session management
  const [sessions, setSessions] = useState<Map<string, DeviceSession>>(new Map());
  const [activeSerial, setActiveSerial] = useState<string>("");
  
  // Connect
  const [devices, setDevices] = useState<Device[]>([]);

  // Helper to update a specific session (used by session setters for multi-device state)
  // @ts-ignore - will be used when session setters are connected
  const updateSession = useCallback((serial: string, updates: Partial<DeviceSession>) => {
    setSessions(prev => {
      const next = new Map(prev);
      const session = next.get(serial);
      if (session) {
        next.set(serial, { ...session, ...updates });
      }
      return next;
    });
  }, []);
  
  // Get active session
  const activeSession = activeSerial ? sessions.get(activeSerial) : null;
  
  // Open a device session
  const openDevice = useCallback((serial: string, model: string = "") => {
    setSessions(prev => {
      const next = new Map(prev);
      if (!next.has(serial)) {
        next.set(serial, createEmptySession(serial, model));
      }
      return next;
    });
    setActiveSerial(serial);
  }, []);
  
  // Close a device session
  const closeDevice = useCallback((serial: string) => {
    setSessions(prev => {
      const next = new Map(prev);
      next.delete(serial);
      return next;
    });
    if (activeSerial === serial) {
      const remaining = Array.from(sessions.keys()).filter(s => s !== serial);
      setActiveSerial(remaining.length > 0 ? remaining[0] : "");
    }
  }, [activeSerial, sessions]);

  // Session accessor for serial - fixes build error
  const serial = activeSession?.serial ?? "";
  const setSerial = (s: string) => {
    if (s && !sessions.has(s)) openDevice(s);
    setActiveSerial(s);
  };

  // Device mirror (live telemetry + screen snapshot)
  const [mirror, setMirror] = useState<DeviceMirror | null>(null);
  const [shot, setShot] = useState("");
  const [shotTs, setShotTs] = useState(0);
  const [shotDark, setShotDark] = useState(false);
  const [mirrorErr, setMirrorErr] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [mirrorBusy, setMirrorBusy] = useState(false);
  const [liveStream, setLiveStream] = useState(false);
  const [streamFps, setStreamFps] = useState(15);
  const [streamMode, setStreamMode] = useState("normal");
  const [isVideoFrame, setIsVideoFrame] = useState(false);

  // Scan
  const [report, setReport] = useState<ScanReport | null>(null);
  const [includeMedia, setIncludeMedia] = useState(false);

  // Recover
  const [dbPath, setDbPath] = useState("");
  const [cryptPath, setCryptPath] = useState("");
  const [keyPath, setKeyPath] = useState("");
  const [decryptNote, setDecryptNote] = useState("");
  const [deep, setDeep] = useState(true);
  const [result, setResult] = useState<UiRecovery | null>(null);
  const [filter, setFilter] = useState("");
  const [onlyDeleted, setOnlyDeleted] = useState(false);

  // Media
  const [media, setMedia] = useState<MediaRecovery | null>(null);
  const [mediaBusy, setMediaBusy] = useState(false);
  const [mediaProgress, setMediaProgress] = useState<{ folder: string; index: number; total: number } | null>(null);
  const [liveMedia, setLiveMedia] = useState<MediaRecovery | null>(null);
  const [preview, setPreview] = useState<MediaFile | null>(null);
  const [inspect, setInspect] = useState<MediaInspect | null>(null);
  const [inspecting, setInspecting] = useState(false);
  const [showImages, setShowImages] = useState(true);
  const [showVideos, setShowVideos] = useState(true);
  // Device-presence check: local paths that are NO LONGER on the phone.
  const [lostSet, setLostSet] = useState<Set<string> | null>(null);
  const [presenceBusy, setPresenceBusy] = useState(false);
  const [presenceAt, setPresenceAt] = useState(0);

  // Installed apps inventory
  const [apps, setApps] = useState<AppInfo[] | null>(null);
  const [appsBusy, setAppsBusy] = useState(false);
  const [appSel, setAppSel] = useState<AppInfo | null>(null);
  const [appFilter, setAppFilter] = useState("");
  const [showSystem, setShowSystem] = useState(false);
  const [waStage, setWaStage] = useState("");

  // Messaging apps detection
  const [appInventory, setAppInventory] = useState<AppInventory | null>(null);
  const [appInventoryBusy, setAppInventoryBusy] = useState(false);

  // Direct acquisition (call logs, SMS, contacts)
  const [directData, setDirectData] = useState<DirectAcquisition | null>(null);
  const [directBusy, setDirectBusy] = useState(false);
  const [directView, setDirectView] = useState<"summary" | "full">("summary");
  const [waDeepResult, setWaDeepResult] = useState<WhatsAppDeepResult | null>(null);
  const [waDeepBusy, setWaDeepBusy] = useState(false);
  const [rootCheck, setRootCheck] = useState<RootCheckResult | null>(null);
  const [rootBusy, setRootBusy] = useState(false);
  
  // Call monitoring
  const [callMonitorActive, setCallMonitorActive] = useState(false);
  const [callEvents, setCallEvents] = useState<CallEvent[]>([]);
  const [currentCallNotification, setCurrentCallNotification] = useState<string | null>(null);
  const callMonitorRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Global
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [exportedPath, setExportedPath] = useState("");

  const run = useCallback(
    async <T,>(fn: () => Promise<T>, ok?: string): Promise<T | undefined> => {
      setBusy(true);
      setError("");
      setStatus("");
      try {
        const out = await fn();
        if (ok) setStatus(ok);
        return out;
      } catch (e) {
        setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
        return undefined;
      } finally {
        setBusy(false);
      }
    },
    []
  );

  const refreshDevices = useCallback(
    () =>
      run(async () => {
        const d = await api.listDevices();
        setDevices(d);
        if (d.length && !d.some((x) => x.serial === serial)) setSerial(d[0].serial);
        return d;
      }),
    [run, serial]
  );

  useEffect(() => {
    refreshDevices();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // ----- Actions ----------------------------------------------------------

  const doScanDevice = (pull: boolean) =>
    run(async () => {
      const [rep, pulled] = await api.scanDevice(serial, pull, includeMedia);
      setReport(rep);
      if (pulled.length) {
        const db = pulled.find((p) => /msgstore\.db$/i.test(p));
        if (db) setDbPath(db);
        const crypt = pulled.find((p) => /\.crypt\d+$/i.test(p) && !p.includes("increment"))
          ?? pulled.find((p) => /\.crypt\d+$/i.test(p));
        if (crypt) setCryptPath(crypt);
      }
      return rep;
    }, pull ? `Scanned & pulled artifacts from ${serial}` : `Scanned ${serial}`);

  const doScanFolder = () =>
    run(async () => {
      const dir = await open({ directory: true, title: "Choose backup folder" });
      if (!dir) return;
      const rep = await api.scanFolder(dir as string);
      setReport(rep);
      const db = rep.artifacts.find((a) => a.kind === "database" && /msgstore\.db$/i.test(a.path));
      if (db) setDbPath(db.path);
      const crypt = rep.artifacts.find((a) => a.kind === "backup" && !a.path.includes("increment"))
        ?? rep.artifacts.find((a) => a.kind === "backup");
      if (crypt) setCryptPath(crypt.path);
      return rep;
    });

  const doDecrypt = () =>
    run(async () => {
      const out = await api.decryptBackup(cryptPath, keyPath, undefined, serial || undefined);
      setDbPath(out.output);
      setDecryptNote(
        out.format === "plain"
          ? "Input was already a plain SQLite database."
          : `Decrypted ${out.format} · key@${out.key_offset} iv@${out.iv_offset} · ${fmtSize(out.plaintext_size)}`
      );
      return out;
    }, "Backup decrypted ✓");

  const doRecover = () =>
    run(async () => {
      const res = await api.recoverDb(dbPath, deep, serial || undefined);
      setResult(res);
      setStep("recover");
      return res;
    }, deep ? "Deep recovery finished (live + carved records)" : "Live parse finished");

  /**
   * One-click WhatsApp evidence read: scan → pull → decrypt (when a key is
   * available) → deep-parse, with a live stage indicator. Everything goes
   * through the same read-only pipeline as the manual steps.
   */
  const readWhatsApp = () =>
    run(async () => {
      setError("");
      setDecryptNote("");
      setWaStage("Scanning WhatsApp storage on the phone…");
      const [rep, pulled] = await api.scanDevice(serial, true, false);
      setReport(rep);
      if (!rep.artifacts.some((a) => a.kind === "database" || a.kind === "backup")) {
        throw new Error(
          "No WhatsApp databases/backups are accessible over USB. Live chat data sits in the app's private folder, which Android exposes only to rooted devices."
        );
      }
      const db = pulled.find((p) => /msgstore\.db$/i.test(p)) ?? "";
      if (db) setDbPath(db);
      const crypt =
        pulled.find((p) => /\.crypt\d+$/i.test(p) && !p.includes("increment")) ??
        pulled.find((p) => /\.crypt\d+$/i.test(p)) ??
        "";
      if (crypt) setCryptPath(crypt);

      let finalDb = db;
      if (!finalDb && crypt) {
        if (!keyPath) {
          throw new Error(
            "Only an encrypted backup (.crypt) is accessible and no key file is loaded. Provide the keyfile in ③ Recover, or import a .txt chat export."
          );
        }
        setWaStage("Decrypting the encrypted backup…");
        const out = await api.decryptBackup(crypt, keyPath, undefined, serial || undefined);
        setDbPath(out.output);
        setDecryptNote(
          out.format === "plain"
            ? "Input was already a plain SQLite database."
            : `Decrypted ${out.format} · key@${out.key_offset} iv@${out.iv_offset} · ${fmtSize(out.plaintext_size)}`
        );
        finalDb = out.output;
      }
      if (!finalDb) {
        throw new Error("No WhatsApp database could be located on the device.");
      }
      setWaStage("Parsing database — messages, chats & contacts…");
      const res = await api.recoverDb(finalDb, true, serial || undefined);
      setResult(res);
      return res;
    }, "WhatsApp evidence loaded — contacts & chats ready");

  const doDirectAcquisition = () =>
    run(async () => {
      setDirectBusy(true);
      try {
        const data = await api.directAcquisition(serial);
        setDirectData(data);
        setStatus(`Direct acquisition complete — ${data.call_logs.length} calls, ${data.sms_messages.length} SMS, ${data.contacts.length} contacts, ${data.call_recordings.length} recordings, ${data.whatsapp_media.length} WA media, ${data.calendar_events.length} calendar`);
        return data;
      } finally {
        setDirectBusy(false);
      }
    }, "System data acquired");

  const doWhatsappDeep = () =>
    run(async () => {
      setWaDeepBusy(true);
      try {
        const data = await api.extractWhatsappDeep(serial);
        setWaDeepResult(data);
        const status = data.database_decrypted
          ? `WhatsApp deep extraction complete — ${data.total_messages} messages, ${data.total_contacts} chats, ${data.media_with_contacts.length} media files`
          : `WhatsApp: ${data.key_obtained ? "Key obtained" : "No key"}, ${data.crypt_file_path ? "database found" : "no database"} — ${data.notes.join(", ")}`;
        setStatus(status);
        return data;
      } finally {
        setWaDeepBusy(false);
      }
    }, "WhatsApp deep extraction complete");

  const doCheckRoot = () =>
    run(async () => {
      setRootBusy(true);
      try {
        const data = await api.checkRoot(serial);
        setRootCheck(data);
        setStatus(data.has_root 
          ? `Root access confirmed${data.can_read_whatsapp_key ? " — WhatsApp key accessible" : ""}`
          : "No root access — WhatsApp key extraction requires root");
        return data;
      } finally {
        setRootBusy(false);
      }
    }, "Root check complete");

  // Call monitoring functions
  const startCallMonitor = useCallback(() => {
    if (!serial || callMonitorRef.current) return;
    setCallMonitorActive(true);
    setCallEvents([]);
    setCurrentCallNotification(null);
    
    // Poll every 2 seconds for call state changes
    callMonitorRef.current = setInterval(async () => {
      try {
        const notification = await api.getCallNotification(serial);
        setCurrentCallNotification(notification);
        
        if (notification) {
          // There's an active call - log it
          const event = await api.pollCallState(serial);
          if (event && event.event_type !== "Idle" && event.event_type !== "Unknown") {
            setCallEvents(prev => {
              // Don't add duplicate events within 5 seconds
              const recent = prev.find(e => 
                e.event_type === event.event_type && 
                Math.abs(e.timestamp - event.timestamp) < 5000
              );
              if (recent) return prev;
              return [...prev, event].slice(-50); // Keep last 50 events
            });
          }
        }
      } catch (e) {
        // Silently ignore errors during monitoring
      }
    }, 2000);
  }, [serial]);

  const stopCallMonitor = useCallback(() => {
    if (callMonitorRef.current) {
      clearInterval(callMonitorRef.current);
      callMonitorRef.current = null;
    }
    setCallMonitorActive(false);
    setCurrentCallNotification(null);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (callMonitorRef.current) {
        clearInterval(callMonitorRef.current);
      }
    };
  }, []);

  const doProvideKey = async () => {
    const f = await open({
      title: "Select WhatsApp key file",
      filters: [{ name: "Key files", extensions: ["key", "*"] }],
    });
    if (!f) return;
    
    // Copy key to temp location
    const keyPath = f as string;
    setWaDeepBusy(true);
    try {
      // Use existing decrypt function with provided key
      if (waDeepResult?.crypt_file_path) {
        const outcome = await api.decryptBackup(
          waDeepResult.crypt_file_path,
          keyPath,
          "D:\\RecoverKit_Temp\\msgstore_decrypted.db"
        );
        setStatus(`Decrypted! Format: ${outcome.format}, key_offset: ${outcome.key_offset}, iv_offset: ${outcome.iv_offset}`);
        // Re-run extraction to parse the decrypted database
        const data = await api.extractWhatsappDeep(serial);
        setWaDeepResult(data);
      }
    } catch (e: any) {
      setStatus(`Decryption failed: ${e.message || e}`);
    } finally {
      setWaDeepBusy(false);
    }
  };

  const doImportTxt = () =>
    run(async () => {
      const f = await open({
        title: "Choose WhatsApp chat export (.txt)",
        filters: [{ name: "WhatsApp export", extensions: ["txt"] }],
      });
      if (!f) return;
      const res = await api.importChatTxt(f as string);
      setResult(res);
      setStatus(`Imported ${res.total_messages} real messages from the chat export`);
      return res;
    });

  const doImportSms = () =>
    run(async () => {
      const f = await open({
        title: "Choose SMS backup (.xml)",
        filters: [{ name: "SMS Backup & Restore", extensions: ["xml"] }],
      });
      if (!f) return;
      const res = await api.importSmsXml(f as string);
      setResult(res);
      setStatus(`Imported ${res.total_messages} real SMS/MMS messages`);
      return res;
    });

  const doImportVcf = () =>
    run(async () => {
      const f = await open({
        title: "Choose contacts export (.vcf)",
        filters: [{ name: "vCard", extensions: ["vcf"] }],
      });
      if (!f) return;
      const res = await api.importVcf(f as string);
      setResult(res);
      setStatus(`Imported ${res.contacts_list.length} real contacts`);
      return res;
    });

  // Streams progress events: the gallery fills up folder by folder and the
  // rest of the app stays usable while the phone is being copied.
  const doRecoverMedia = async (source?: string) => {
    if (!serial || mediaBusy) return;
    setMediaBusy(true);
    setError("");
    setLiveMedia(null);
    setLostSet(null);
    setMediaProgress({ folder: "Starting…", index: 0, total: 0 });
    const unlisten = await listen<{
      phase: string;
      folder: string;
      index: number;
      total: number;
      files: MediaFile[];
    }>("media-progress", (e) => {
      const p = e.payload;
      setMediaProgress({ folder: p.folder, index: p.index, total: p.total });
      if (p.files && p.files.length) {
        setLiveMedia({ files: p.files, pulled_dirs: p.index, failed_dirs: 0 });
      }
    });
    try {
      const m = await api.recoverMedia(serial, source);
      setMedia(m);
      setStatus(`Media recovered — ${m.files.length} photos/videos copied from the phone`);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      unlisten();
      setMediaBusy(false);
      setMediaProgress(null);
      setLiveMedia(null);
    }
  };

  /** Gallery source while recovering (live stream) or after (final). */
  const viewMedia = liveMedia ?? media;

  /** Visible tiles — recomputed only when the list or filters change. */
  const gridFiles = useMemo(
    () => (viewMedia ? viewMedia.files.filter((f) => (f.kind === "image" ? showImages : showVideos)) : []),
    [viewMedia, showImages, showVideos]
  );

  /**
   * Re-reads the phone's media folders (read-only) and works out, per
   * recovered file, whether it still exists on the device or is lost from
   * it — in which case the local copy is the only remaining evidence.
   */
  const verifyOnDevice = async () => {
    if (!serial || !viewMedia || presenceBusy) return;
    setPresenceBusy(true);
    setError("");
    try {
      const onDevice = new Set(await api.devicePresence(serial));
      const lost = new Set<string>();
      for (const f of viewMedia.files) {
        // Local mirror: …/pulled/<serial>/sdcard/… → device /sdcard/…
        const dev = "/" + f.path.replace(/\\/g, "/").split(`/pulled/${serial}/`)[1];
        if (!onDevice.has(dev)) lost.add(f.path);
      }
      setLostSet(lost);
      setPresenceAt(Date.now());
      setStatus(
        lost.size === 0
          ? `Device verified — all ${viewMedia.files.length} recovered files still exist on the phone`
          : `Device verified — ${lost.size} file(s) lost from the phone, local copies are the only evidence`
      );
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setPresenceBusy(false);
    }
  };

  // ----- Device mirror -----------------------------------------------------

  const loadMirror = (s: string) => {
    api.deviceMirror(s).then((m) => { setMirror(m); setMirrorErr(""); }).catch(() => setMirror(null));
  };

  const capture = async (s: string) => {
    setMirrorBusy(true);
    try {
      const r = await api.deviceScreenshot(s);
      setShot(r.path);
      setShotDark(r.dark);
      setShotTs(Date.now());
      setMirrorErr("");
    } catch (e) {
      // Never hide the reason — a silent stale frame is worse than an error.
      setMirrorErr(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setMirrorBusy(false);
    }
  };

  const startStream = async (s: string) => {
    try {
      await api.startScreenStream(s, streamFps, streamMode);
      setLiveStream(true);
    } catch (e) {
      setMirrorErr(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    }
  };

  const stopStream = async (s: string) => {
    try {
      await api.stopScreenStream(s);
      setLiveStream(false);
    } catch (e) {
      console.error("Failed to stop stream:", e);
    }
  };

  // Listen for screen-frame events when live stream is active
  useEffect(() => {
    if (!liveStream || !serial) return;
    const unlisten = listen<{ serial: string; path: string; frame: number; timestamp: number; isVideo: boolean; mode: string }>(
      "screen-frame",
      (event) => {
        if (event.payload.serial === serial) {
          setShot(event.payload.path);
          setShotTs(event.payload.timestamp);
          setShotDark(false); // Stream frames are never dark
          setIsVideoFrame(event.payload.isVideo || false);
        }
      }
    );
    return () => {
      unlisten.then((u) => u());
    };
  }, [liveStream, serial]);

  useEffect(() => {
    // A different device means a different inventory
    setApps(null);
    setAppSel(null);
    setLostSet(null);
    setPresenceAt(0);
    if (!serial) {
      setMirror(null);
      setShot("");
      setShotTs(0);
      setShotDark(false);
      setMirrorErr("");
      return;
    }
    loadMirror(serial);
    capture(serial);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial]);

  useEffect(() => {
    if (!serial || !autoRefresh || step !== "connect") return;
    // keep BOTH the screen snapshot and the header telemetry fresh
    const t = setInterval(() => {
      capture(serial);
      loadMirror(serial);
    }, 5000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, autoRefresh, step]);

  // ----- Installed apps inventory ------------------------------------------

  const loadApps = async () => {
    if (!serial || appsBusy) return;
    setAppsBusy(true);
    setError("");
    try {
      const list = await api.listApps(serial);
      setApps(list);
      setStatus(`Inventory complete — ${list.length} installed apps`);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setAppsBusy(false);
    }
  };

  const detectMessagingApps = async () => {
    if (!serial || appInventoryBusy) return;
    setAppInventoryBusy(true);
    setError("");
    try {
      const inventory = await api.detectApps(serial);
      setAppInventory(inventory);
      setStatus(`Detected ${inventory.total_installed} messaging apps — ${inventory.total_with_data} with data`);
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setAppInventoryBusy(false);
    }
  };

  // Read the inventory once on the first visit of the Apps step
  useEffect(() => {
    if (step === "apps" && serial && !apps && !appsBusy) loadApps();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, serial]);

  const openApp = async (a: AppInfo) => {
    setAppSel(a); // list row shows immediately, details stream in
    if (!serial) return;
    // Clicking a WhatsApp row reads its chats & contacts straight away
    if (/whatsapp/i.test(a.package) && !busy) {
      readWhatsApp().finally(() => setWaStage(""));
    }
    try {
      const d = await api.appDetails(serial, a.package);
      setAppSel((cur) =>
        cur && cur.package === a.package
          ? { ...a, ...d, apk_path: a.apk_path, system: a.system }
          : cur
      );
    } catch {
      /* dumpsys failed — the list row stays visible */
    }
  };

  const shownApps = useMemo(() => {
    if (!apps) return [];
    const q = appFilter.trim().toLowerCase();
    return apps.filter(
      (a) => (showSystem || !a.system) && (!q || a.package.toLowerCase().includes(q))
    );
  }, [apps, appFilter, showSystem]);

  const openPreview = useCallback((f: MediaFile) => {
    setPreview(f);
    setInspect(null);
  }, []);

  /** Reads the actual bytes: real format detection + SHA-256 proof. */
  const doInspect = async () => {
    if (!preview || inspecting) return;
    setInspecting(true);
    try {
      setInspect(await api.inspectMedia(preview.path));
    } catch (e) {
      setError(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setInspecting(false);
    }
  };

  const doSaveMedia = () =>
    run(async () => {
      const dest = await save({ title: "Save recovered media to folder", defaultPath: "WhatsApp-Recovered" });
      if (!dest) return;
      return await api.saveMedia(serial, dest as string);
    });

  const doOpenMediaDir = () =>
    run(async () => {
      const d = await api.mediaDir(serial);
      await openPath(d);
    });

  const doExport = (format: string, ext: string) =>
    run(async () => {
      const dest = await save({
        title: "Export recovery result",
        defaultPath: `recoverkit-export.${ext}`,
        filters: [{ name: format.toUpperCase(), extensions: [ext] }],
      });
      if (!dest) return;
      const path = await api.exportResult(format, dest as string);
      setExportedPath(path);
      return path;
    }, "Export complete");

  // ----- Derived ----------------------------------------------------------

  /** Identity resolution: jid → contact/chat display name. */
  const nameMap = useMemo(() => new Map(result?.names ?? []), [result]);
  const nameOf = (jid: string | null): string =>
    jid == null ? "" : nameMap.get(jid) ?? "";

  const filtered = useMemo(() => {
    if (!result) return [];
    const q = filter.trim().toLowerCase();
    return result.messages.filter((m) => {
      if (onlyDeleted && m.source === "live") return false;
      if (!q) return true;
      return (
        (m.text ?? "").toLowerCase().includes(q) ||
        (m.chat_jid ?? "").toLowerCase().includes(q) ||
        nameOf(m.chat_jid).toLowerCase().includes(q) ||
        (m.sender ?? "").toLowerCase().includes(q)
      );
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [result, filter, onlyDeleted]);

  const dbArtifacts = report?.artifacts.filter((a) => a.kind === "database") ?? [];
  const cryptArtifacts = report?.artifacts.filter((a) => a.kind === "backup") ?? [];

  // ----- Render -----------------------------------------------------------

  return (
    <div className="app">
      <header>
        <div className="brand">
          <span className="logo">🔶</span>
          <div>
            <h1>RecoverKit</h1>
            <p>read-only mobile forensics</p>
          </div>
        </div>
        <div className="device-chip">
          <span className={serial ? "dot" : "dot off"} />
          {devices.length > 0 ? (
            <select 
              className="dev-sel" 
              value={activeSerial} 
              onChange={(e) => {
                const s = e.target.value;
                if (s && !sessions.has(s)) {
                  const device = devices.find(d => d.serial === s);
                  openDevice(s, device?.model ?? device?.product ?? "");
                } else {
                  setActiveSerial(s);
                }
              }}
            >
              <option value="">Select device...</option>
              {devices.map((d) => (
                <option key={d.serial} value={d.serial}>
                  {d.model ?? d.product ?? d.serial} ({d.serial})
                </option>
              ))}
            </select>
          ) : (
            <span>no device connected</span>
          )}
          <button className="mini" onClick={refreshDevices} disabled={busy} title="Refresh devices">
            ⟳
          </button>
        </div>
        {busy && <span className="pill busy-pill">working…</span>}
        {serial && (
          <span
            className={`mirror-pill${mirrorBusy ? " busy" : ""}${mirrorErr ? " err" : ""}`}
            title={mirrorErr ? mirrorErr : "Live mirror status"}
          >
            <i>Mirror</i>
            {mirrorErr
              ? "⚠ capture error"
              : mirrorBusy
              ? "capturing…"
              : shotTs
              ? new Date(shotTs).toLocaleTimeString()
              : "no snapshot"}
          </span>
        )}
        {mirror && (
          <div className="header-telemetry">
            <span title="Device model"><i>Model</i> {mirror.model || "—"}</span>
            <span title="Android version"><i>Android</i> {mirror.android || "—"}</span>
            <span title="Battery level"><i>Battery</i> {mirror.battery}</span>
            <span title="Free / total storage"><i>Storage</i> {mirror.storage}</span>
            <span title="Screen resolution"><i>Screen</i> {mirror.resolution}</span>
            <span title="Time since boot"><i>Uptime</i> {mirror.uptime}</span>
            <span title="Installed WhatsApp version"><i>WhatsApp</i> {mirror.whatsapp}</span>
            <button
              className="mini"
              disabled={!serial || mirrorBusy}
              onClick={() => serial && loadMirror(serial)}
              title="Refresh telemetry"
            >
              ⟳
            </button>
          </div>
        )}
      </header>
      
      {/* Device Tab Bar for Multi-Device Support */}
      {sessions.size > 0 && (
        <div className="device-tabs">
          {Array.from(sessions.values()).map((session) => (
            <button
              key={session.serial}
              className={`device-tab ${session.serial === activeSerial ? 'active' : ''}`}
              onClick={() => setActiveSerial(session.serial)}
            >
              📱 {session.model || session.serial.slice(0, 12)}
              {session.liveStream && <span className="live-dot" title="Live streaming">●</span>}
              {session.report && <span className="data-dot" title="Data acquired">📊</span>}
              <span 
                className="close-tab" 
                onClick={(e) => {
                  e.stopPropagation();
                  closeDevice(session.serial);
                }}
                title="Close device"
              >×</span>
            </button>
          ))}
        </div>
      )}

      <div className="body">
        <aside className="sidebar">
          {STEPS.map((s) => (
            <button
              key={s.id}
              className={step === s.id ? "nav active" : "nav"}
              onClick={() => setStep(s.id)}
              title={s.label}
            >
              <span className="nav-icon">{s.icon}</span>
              <span className="nav-label">{s.label}</span>
            </button>
          ))}
        </aside>

        <main>
        {step === "connect" && (
          <section className="workbench wb-connect">
            {/* LEFT — System data acquisition */}
            <div className="panel" style={{ gridArea: "left" }}>
              <div className="panel-bar">📞 System Data</div>
              <div className="panel-body">
                <button className="primary" disabled={!serial || directBusy || busy} onClick={doDirectAcquisition}>
                  {directBusy ? "Acquiring…" : "📥 Acquire Call Logs, SMS & Contacts"}
                </button>
                <p className="hint small">
                  Reads call logs, SMS/MMS, and contacts directly from Android content providers.
                  No root required.
                </p>
                {directData && (
                  <>
                    <hr className="pdiv" />
                    <div className="stats">
                      <span className="pill ok">{directData.call_logs.length} calls</span>
                      <span className="pill ok">{directData.sms_messages.length} SMS</span>
                      <span className="pill ok">{directData.contacts.length} contacts</span>
                      <span className="pill ok">{directData.call_recordings.length} recordings</span>
                      <span className="pill ok">{directData.whatsapp_media.length} WA media</span>
                      <span className="pill ok">{directData.calendar_events.length} calendar</span>
                    </div>
                    {directData.notes.length > 0 && (
                      <div className="hint" style={{ marginTop: 8 }}>
                        {directData.notes.map((n, i) => (
                          <p key={i} className="err">⚠ {n}</p>
                        ))}
                      </div>
                    )}
                    <button 
                      className="primary" 
                      style={{ marginTop: 12, width: "100%" }}
                      onClick={() => setDirectView("full")}
                    >
                      🔍 View All Data
                    </button>
                  </>
                )}
              </div>
            </div>

            {/* WhatsApp Deep Extraction Panel */}
            <div className="panel" style={{ gridArea: "system" }}>
              <div className="panel-bar">🔐 WhatsApp Deep Extraction</div>
              <div className="panel-body">
                <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
                  <button className="primary" disabled={!serial || waDeepBusy || busy} onClick={doWhatsappDeep} style={{ flex: 1 }}>
                    {waDeepBusy ? "Extracting…" : "🔓 Auto-Extract Chats"}
                  </button>
                  <button className="mini" disabled={!serial || rootBusy} onClick={doCheckRoot} title="Check if device has root access">
                    {rootBusy ? "…" : "🔍 Root"}
                  </button>
                </div>
                
                {/* Root Check Results */}
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
                    
                    {/* Guided Instructions */}
                    {!rootCheck.has_root && (
                      <div style={{ marginTop: 10, padding: 8, background: "rgba(255,200,0,0.1)", borderRadius: 4, fontSize: 12 }}>
                        <strong>⚠️ Root Required for Full Extraction</strong>
                        <p style={{ margin: "6px 0 4px" }}>The WhatsApp encryption key is protected. Options:</p>
                        <ol style={{ margin: 0, paddingLeft: 18, lineHeight: 1.6 }}>
                          <li><strong>Root device</strong> — Install Magisk (unlock bootloader → flash Magisk)</li>
                          <li><strong>Export chats</strong> — In WhatsApp: Chat → ⋮ → More → Export chat → import .txt here</li>
                          <li><strong>Use rooted device</strong> — Login WhatsApp on a rooted phone, extract key</li>
                        </ol>
                        <button 
                          className="mini" 
                          style={{ marginTop: 8 }}
                          onClick={doImportTxt}
                        >
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
                  Auto-extracts: key → decrypt → parse → chats. Root needed for full access.
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
                    
                    {/* Provide Key button when key not obtained but crypt file exists */}
                    {!waDeepResult.key_obtained && waDeepResult.crypt_file_path && (
                      <div style={{ marginTop: 12, padding: 10, background: "rgba(100,150,255,0.1)", borderRadius: 6 }}>
                        <p style={{ margin: "0 0 8px", fontSize: 12 }}>
                          <strong>💡 Have the key file?</strong> If you obtained the WhatsApp key from another source
                          (rooted device, backup extraction, etc.), you can provide it now to decrypt the database.
                        </p>
                        <button className="mini" onClick={doProvideKey} disabled={waDeepBusy}>
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
                        <div style={{ maxHeight: 200, overflowY: "auto", border: "1px solid var(--border)", borderRadius: 6, padding: 8 }}>
                          {waDeepResult.chats.slice(0, 20).map((chat, i) => (
                            <div key={i} style={{ padding: "4px 0", borderBottom: i < 19 ? "1px solid var(--border)" : "none" }}>
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

            {/* Call Monitor Panel */}
            <div className="panel" style={{ gridArea: "main" }}>
              <div className="panel-bar">
                📞 Call Monitor
                {callMonitorActive && <span className="pill ok" style={{ marginLeft: 8 }}>● Live</span>}
              </div>
              <div className="panel-body">
                <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
                  <button 
                    className={callMonitorActive ? "mini" : "primary"} 
                    disabled={!serial}
                    onClick={callMonitorActive ? stopCallMonitor : startCallMonitor}
                    style={{ flex: 1 }}
                  >
                    {callMonitorActive ? "⏹ Stop Monitoring" : "▶ Start Call Monitor"}
                  </button>
                </div>
                <p className="hint small">
                  Monitors incoming/outgoing calls in real-time. Notifications appear when calls are initiated.
                </p>
                
                {/* Active Call Notification */}
                {currentCallNotification && (
                  <div style={{ 
                    marginTop: 12, 
                    padding: 12, 
                    background: "rgba(0,200,100,0.15)", 
                    borderRadius: 8,
                    border: "2px solid rgba(0,200,100,0.5)",
                    animation: "pulse 1s infinite"
                  }}>
                    <p style={{ margin: 0, fontSize: 16, fontWeight: 600 }}>
                      {currentCallNotification}
                    </p>
                  </div>
                )}
                
                {/* Call Event Log */}
                {callEvents.length > 0 && (
                  <div style={{ marginTop: 12 }}>
                    <p style={{ fontWeight: 500, marginBottom: 8, fontSize: 12 }}>Call Log ({callEvents.length}):</p>
                    <div style={{ maxHeight: 150, overflowY: "auto", border: "1px solid var(--border)", borderRadius: 6, padding: 8 }}>
                      {callEvents.slice().reverse().map((event, i) => (
                        <div key={i} style={{ 
                          padding: "4px 0", 
                          borderBottom: i < callEvents.length - 1 ? "1px solid var(--border)" : "none",
                          fontSize: 12
                        }}>
                          <span style={{ marginRight: 8 }}>
                            {event.event_type === "Incoming" ? "📞" : 
                             event.event_type === "Outgoing" ? "📱" : 
                             event.event_type === "Missed" ? "❌" : "📞"}
                          </span>
                          <strong>{event.event_type}</strong>
                          {event.phone_number && (
                            <span style={{ marginLeft: 8, color: "var(--muted)" }}>
                              {event.phone_number}
                            </span>
                          )}
                          {event.contact_name && (
                            <span style={{ marginLeft: 8, color: "var(--accent)" }}>
                              {event.contact_name}
                            </span>
                          )}
                          <span style={{ marginLeft: "auto", fontSize: 10, color: "var(--muted)" }}>
                            {new Date(event.timestamp).toLocaleTimeString()}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
                
                {!callMonitorActive && callEvents.length === 0 && (
                  <p className="hint small" style={{ marginTop: 12, textAlign: "center" }}>
                    Click "Start Call Monitor" to begin monitoring calls
                  </p>
                )}
              </div>
            </div>

            {/* CENTER — live screen mirror */}
            <div className="panel" style={{ gridArea: "main" }}>
              <div className="panel-bar">
                📱 {liveStream ? "Live stream" : "Screen mirror"}
                {liveStream && <span className="hint" style={{ marginLeft: 8 }}>● {streamFps} fps</span>}
              </div>
              <div className="panel-body mirror-screen">
                {shot ? (
                  isVideoFrame ? (
                    <video
                      src={`${convertFileSrc(shot)}?t=${shotTs}`}
                      autoPlay
                      muted
                      loop
                      style={{ maxWidth: '100%', maxHeight: '100%' }}
                    />
                  ) : (
                    <img src={`${convertFileSrc(shot)}?t=${shotTs}`} alt="phone screen" />
                  )
                ) : (
                  <div className="empty">{mirrorBusy ? "capturing screen…" : serial ? "no snapshot yet" : "select a device"}</div>
                )}
                {mirrorErr && (
                  <p className="hint lock-warn err">
                    ⚠ Screen capture failed: {mirrorErr}
                  </p>
                )}
                {shotDark && (
                  <p className="hint lock-warn">
                    🔒 Dark frame — the phone is locked. Android hides screen contents behind the lock
                    for security, so the mirror can only show the lock screen itself. Unlock the phone
                    once and the live mirror continues automatically.
                  </p>
                )}
                <div className="row">
                  <button disabled={!serial || mirrorBusy || liveStream} onClick={() => serial && capture(serial)}>
                    📸 Snapshot now
                  </button>
                  <label className="check">
                    <input type="checkbox" checked={autoRefresh} disabled={liveStream} onChange={(e) => setAutoRefresh(e.target.checked)} />
                    auto-refresh (5s)
                  </label>
                </div>
                <div className="row" style={{ marginTop: 8 }}>
                  <button
                    className={liveStream ? "" : "primary"}
                    disabled={!serial || liveStream}
                    onClick={() => serial && startStream(serial)}
                  >
                    ▶ Start live stream
                  </button>
                  <button
                    disabled={!liveStream}
                    onClick={() => serial && stopStream(serial)}
                  >
                    ⏹ Stop stream
                  </button>
                  <label className="check">
                    FPS:
                    <select
                      value={streamFps}
                      onChange={(e) => setStreamFps(Number(e.target.value))}
                      disabled={liveStream}
                      style={{ marginLeft: 4 }}
                    >
                      <option value={5}>5</option>
                      <option value={10}>10</option>
                      <option value={15}>15</option>
                      <option value={20}>20</option>
                      <option value={30}>30</option>
                      <option value={60}>60</option>
                    </select>
                  </label>
                  <label className="check">
                    Mode:
                    <select
                      value={streamMode}
                      onChange={(e) => setStreamMode(e.target.value)}
                      disabled={liveStream}
                      style={{ marginLeft: 4 }}
                    >
                      <option value="normal">Normal (stable)</option>
                      <option value="fast">Fast (experimental)</option>
                    </select>
                  </label>
                </div>
              </div>
            </div>

            {/* TOP — status strip */}
            <div className="panel timeline" style={{ gridArea: "strip" }}>
              <div className="panel-body tl">
                <span className="hint nowrap">
                  {devices.length === 0
                    ? "No device detected — enable USB debugging, plug in via USB-C, tap Allow, then hit ⟳ in the header."
                    : `${devices.length} device${devices.length === 1 ? "" : "s"} · ${serial || "none selected"} · read-only`}
                </span>
                {devices.some((d) => d.serial === serial && d.state === "device") && (
                  <button className="primary next" onClick={() => setStep("scan")}>
                    Continue to Scan →
                  </button>
                )}
              </div>
            </div>

            {/* FULL SCREEN Direct Data Viewer */}
            {directView === "full" && directData && (
              <DirectDataViewer
                data={directData}
                serial={serial}
                onClose={() => setDirectView("summary")}
                onStatus={setStatus}
              />
            )}          </section>
        )}

        {step === "apps" && (
          <section className="workbench wb-apps">
            {/* Messaging Apps Detection Panel */}
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
                  onDetect={detectMessagingApps}
                  onExtract={(appId) => {
                    setStatus(`Extracting data from ${appId}...`);
                    // TODO: Implement per-app extraction
                  }}
                  loading={appInventoryBusy}
                />
              </div>
            </div>

            {/* TOP — strip */}
            <div className="panel timeline" style={{ gridArea: "strip" }}>
              <div className="panel-body tl">
                <span className="hint nowrap">
                  {apps
                    ? `${apps.length} apps · ${apps.filter((a) => !a.system).length} user-installed · ${apps.filter((a) => a.system).length} system`
                    : appsBusy
                    ? "reading the package list…"
                    : "app inventory not loaded yet"}
                </span>
                <button className="mini" disabled={!serial || appsBusy} onClick={loadApps} title="Reload the installed app inventory">
                  ⟳
                </button>
                {apps && (
                  <button className="primary next" onClick={() => setStep("scan")}>
                    Continue to Scan →
                  </button>
                )}
              </div>
            </div>

            {/* LEFT — filters */}
            <div className="panel" style={{ gridArea: "left" }}>
              <div className="panel-bar">🎛 Filters</div>
              <div className="panel-body">
                <button className="primary" disabled={!serial || appsBusy} onClick={loadApps}>
                  {appsBusy ? "Reading device…" : "📥 Load apps from device"}
                </button>
                <input
                  type="text"
                  placeholder="search package…"
                  value={appFilter}
                  onChange={(e) => setAppFilter(e.target.value)}
                />
                <label className="check">
                  <input type="checkbox" checked={showSystem} onChange={(e) => setShowSystem(e.target.checked)} />
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

            {/* CENTER — installed apps table */}
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
                  <AppTable apps={shownApps} serial={serial} sel={appSel?.package ?? ""} onOpen={openApp} />
                )}
              </div>
            </div>

            {/* RIGHT — configuration inspector */}
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
                              onClick={() => readWhatsApp().finally(() => setWaStage(""))}
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
        )}

        {step === "scan" && (
          <section className="workbench wb-scan">
            {/* LEFT — scan actions */}
            <div className="panel" style={{ gridArea: "left" }}>
              <div className="panel-bar">⚙ Actions</div>
              <div className="panel-body">
                <button className="primary" disabled={!serial || busy} onClick={() => doScanDevice(false)}>
                  🔍 Scan device
                </button>
                <button className="primary" disabled={!serial || busy} onClick={() => doScanDevice(true)}>
                  ⬇ Scan & pull
                </button>
                <label className="check">
                  <input type="checkbox" checked={includeMedia} onChange={(e) => setIncludeMedia(e.target.checked)} />
                  include media
                </label>
                <hr className="pdiv" />
                <button disabled={busy} onClick={doScanFolder}>
                  📂 Import folder…
                </button>
                <p className="hint">
                  Scan enumerates WhatsApp databases, encrypted backups & media without copying. Scan &
                  pull also copies them locally for recovery.
                </p>
              </div>
            </div>

            {/* CENTER — artifact table */}
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
                      <thead>
                        <tr>
                          <th>Kind</th>
                          <th>Path</th>
                          <th>Size</th>
                        </tr>
                      </thead>
                      <tbody>
                        {report.artifacts.map((a) => (
                          <tr key={a.path} className={a.path === dbPath || a.path === cryptPath ? "sel" : ""}>
                            <td>
                              <span className={`pill kind-${a.kind.toLowerCase()}`}>{a.kind}</span>
                            </td>
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
                  <div className="empty">Nothing scanned yet — use the actions on the left.</div>
                )}
              </div>
            </div>

            {/* RIGHT — summary & notes */}
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

            {/* TOP — strip */}
            <div className="panel timeline" style={{ gridArea: "strip" }}>
              <div className="panel-body tl">
                <span className="hint nowrap">
                  {report
                    ? `${report.artifacts.length} artifacts · ${report.artifacts.filter((a) => a.kind === "database").length} databases · ${report.artifacts.filter((a) => a.kind === "backup").length} backups`
                    : "no scan yet"}
                </span>
                {report && (
                  <button className="primary next" disabled={report.artifacts.length === 0} onClick={() => setStep("recover")}>
                    Continue to Recover →
                  </button>
                )}
              </div>
            </div>
          </section>
        )}

        {step === "media" && (
          <section className="workbench">
            {/* LEFT — acquisition panel */}
            <div className="panel" style={{ gridArea: "acq" }}>
              <div className="panel-bar">📥 Acquire</div>
              <div className="panel-body">
                {!serial && <div className="empty">Select a device in ① Connect first.</div>}
                <button className="primary" disabled={!serial || busy || mediaBusy} onClick={() => doRecoverMedia("whatsapp")}>
                  💬 WhatsApp media
                </button>
                <button className="primary" disabled={!serial || busy || mediaBusy} onClick={() => doRecoverMedia("gallery")}>
                  🖼 Gallery (any app)
                </button>
                <button disabled={!serial || busy || mediaBusy} onClick={() => doRecoverMedia("downloads")}>
                  📁 Downloads
                </button>
                {media && !mediaBusy && media.files.length > 0 && (
                  <>
                    <hr className="pdiv" />
                    <button className="primary" disabled={busy} onClick={doSaveMedia}>
                      💾 Save all…
                    </button>
                    <button disabled={busy} onClick={doOpenMediaDir}>
                      📂 Open folder
                    </button>
                    <button disabled={!serial || busy || presenceBusy} onClick={verifyOnDevice}>
                      {presenceBusy ? "📡 Checking the phone…" : "📡 Verify on device"}
                    </button>
                    <p className="hint small">
                      Re-reads the phone (read-only) and flags every recovered file that is lost from
                      the device — its local copy is then the only evidence.
                    </p>
                  </>
                )}
                <p className="hint">
                  Read-only — nothing on the phone is touched. Already-copied files are reused on re-runs.
                </p>
              </div>
            </div>

            {/* CENTER — gallery monitor */}
            <div className="panel" style={{ gridArea: "gallery" }}>
              <div className="panel-bar">
                🖼 Gallery
                <span className="sep" />
                <label className="check">
                  <input type="checkbox" checked={showImages} onChange={(e) => setShowImages(e.target.checked)} />
                  images
                </label>
                <label className="check">
                  <input type="checkbox" checked={showVideos} onChange={(e) => setShowVideos(e.target.checked)} />
                  videos
                </label>
              </div>
              <div className="panel-body">
                {viewMedia && viewMedia.files.length > 0 ? (
                  <MediaGrid files={gridFiles} selectedPath={preview?.path ?? ""} onOpen={openPreview} lost={lostSet} />
                ) : (
                  <div className="empty">
                    {mediaBusy
                      ? "Copying media from the phone…"
                      : "No media yet — pick an acquisition source in the Acquire panel."}
                  </div>
                )}
              </div>
            </div>

            {/* RIGHT — inspector panel */}
            <div className="panel" style={{ gridArea: "inspector" }}>
              <div className="panel-bar">🔍 Inspector</div>
              <div className="panel-body">
                {preview ? (
                  <>
                    {preview.kind === "image" ? (
                      <img className="insp-img" src={convertFileSrc(preview.path)} alt="" />
                    ) : (
                      <video className="insp-img" src={convertFileSrc(preview.path)} controls />
                    )}
                    <span className="mono small">{preview.path.split(/[\\/]/).pop()}</span>
                    <div className="stats">
                      <span className="pill">{fmtSize(preview.size)}</span>
                      {inspect && (
                        <span className={`pill ${inspect.valid ? "ok" : "warn"}`}>
                          {inspect.valid ? `✔ genuine ${inspect.format}` : "⚠ invalid media"}
                        </span>
                      )}
                    </div>
                    <button className="primary" disabled={inspecting} onClick={doInspect}>
                      {inspecting ? "Reading bytes…" : inspect ? "✔ Re-verify file" : "✔ Verify real file"}
                    </button>
                    <button onClick={() => openPath(preview.path)}>🔓 Open in viewer</button>
                    {inspect && (
                      <p className={`hint ${inspect.valid ? "" : "err"}`}>
                        {inspect.valid
                          ? `Genuine ${inspect.format} — ${fmtSize(inspect.size)} of real bytes from the phone. SHA-256: `
                          : "⚠ Not valid image/video bytes — corrupted or incomplete. "}
                        <span className="mono small">{inspect.sha256}</span>
                      </p>
                    )}
                  </>
                ) : (
                  <p className="hint">Click any thumbnail in the gallery to inspect & verify it here.</p>
                )}
              </div>
            </div>

            {/* TOP — timeline / status strip */}
            <div className="panel timeline" style={{ gridArea: "timeline" }}>
              <div className="panel-body tl">
                {serial && mediaBusy && mediaProgress && (
                  <div className="tl-progress">
                    <div className="progress-track">
                      <div
                        className="progress-fill"
                        style={{
                          width: mediaProgress.total
                            ? `${Math.round((mediaProgress.index / mediaProgress.total) * 100)}%`
                            : "6%",
                        }}
                      />
                    </div>
                    <span className="hint nowrap">
                      ⬇ <b>{mediaProgress.folder}</b>
                      {mediaProgress.total > 0 && <> · {mediaProgress.index}/{mediaProgress.total}</>}
                      {viewMedia && <> · {viewMedia.files.length} files</>}
                    </span>
                  </div>
                )}
                {viewMedia && (
                  <div className="stats">
                    <span className="pill ok">{viewMedia.files.filter((f) => f.kind === "image").length} images</span>
                    <span className="pill ok">{viewMedia.files.filter((f) => f.kind === "video").length} videos</span>
                    {lostSet && (
                      <>
                        <span className="pill ok">{viewMedia.files.length - lostSet.size} on device</span>
                        <span className={lostSet.size ? "pill kind-media" : "pill ok"}>
                          {lostSet.size} lost from device
                        </span>
                        <span className="hint nowrap">checked {new Date(presenceAt).toLocaleTimeString()}</span>
                      </>
                    )}
                    <span className="pill">{fmtSize(viewMedia.files.reduce((a, f) => a + f.size, 0))}</span>
                    {mediaBusy && <span className="pill">recovering…</span>}
                    {!mediaBusy && viewMedia.failed_dirs > 0 && (
                      <span className="pill warn">{viewMedia.failed_dirs} folder(s) failed</span>
                    )}
                  </div>
                )}
                {!viewMedia && !mediaBusy && <span className="hint">Ready — choose an acquisition source.</span>}
              </div>
            </div>
          </section>
        )}

        {step === "recover" && (
          <section className="workbench wb-recover">
            {/* LEFT — evidence sources */}
            <div className="panel" style={{ gridArea: "left" }}>
              <div className="panel-bar">🗄 Sources</div>
              <div className="panel-body">

            <div className="field">
              <label>Database (msgstore.db)</label>
              <div className="row">
                <input className="mono" value={dbPath} onChange={(e) => setDbPath(e.target.value)} placeholder="path to msgstore.db" />
                <select value={dbPath} onChange={(e) => setDbPath(e.target.value)} disabled={dbArtifacts.length === 0}>
                  <option value="">— found in scan —</option>
                  {dbArtifacts.map((a) => (
                    <option key={a.path} value={a.path}>{a.path.split(/[\\/]/).pop()}</option>
                  ))}
                </select>
                <button onClick={async () => {
                  const f = await open({ filters: [{ name: "SQLite", extensions: ["db"] }] });
                  if (f) setDbPath(f as string);
                }}>📄…</button>
              </div>
              {!dbPath && (
                <p className="hint">
                  👉 No database selected yet — go to <b>② Scan</b> and use <b>📂 Import folder…</b> to load a
                  WhatsApp backup folder (the path auto-fills), or click <b>📄…</b> to browse a msgstore.db manually.
                  Then press <b>▶ Run recovery</b> below.
                </p>
              )}
            </div>

            <div className="field sub">
              <label>💬 No root? Import evidence files instead — works for every app that can export</label>
              <div className="row">
                <button className="primary" disabled={busy} onClick={doImportTxt}>
                  💬 WhatsApp chat (.txt) …
                </button>
                <button className="primary" disabled={busy} onClick={doImportSms}>
                  📨 SMS / MMS (.xml) …
                </button>
                <button className="primary" disabled={busy} onClick={doImportVcf}>
                  📇 Contacts (.vcf) …
                </button>
              </div>
              <p className="hint">
                <b>WhatsApp:</b> chat → ⋯ → More → Export chat → Without media · <b>SMS:</b> install the free
                “SMS Backup &amp; Restore” app → Back up → copy the .xml here · <b>Contacts:</b> Contacts app →
                ⋯ → Export → .vcf. All real data, <b>no key and no root needed</b>. (Deleted items are not part of exports.)
              </p>
            </div>

            {(cryptArtifacts.length > 0 || cryptPath) && (
              <div className="field sub">
                <label>🔒 Encrypted backup (crypt12/14/15) + device key</label>
                <div className="row">
                  <select value={cryptPath} onChange={(e) => setCryptPath(e.target.value)}>
                    <option value="">— encrypted backup —</option>
                    {cryptArtifacts.map((a) => (
                      <option key={a.path} value={a.path}>{a.path.split(/[\\/]/).pop()}</option>
                    ))}
                  </select>
                  <input className="mono" value={keyPath} onChange={(e) => setKeyPath(e.target.value)} placeholder="key file path — paste it here, or use Browse key" />
                  <button onClick={async () => {
                    const f = await open({ title: "Choose WhatsApp key file (usually a 158-byte file named 'key')" });
                    if (f) setKeyPath(f as string);
                  }}>🗝 Browse key…</button>
                  <button
                    disabled={!serial || busy}
                    title="Requires a rooted phone: reads /data/data/com.whatsapp/files/key via su"
                    onClick={() => run(async () => {
                      const p = await api.fetchKey(serial);
                      setKeyPath(p);
                      return p;
                    }, "Key fetched from phone ✓")}
                  >
                    📥 Fetch key (root)
                  </button>
                  <button className="primary" disabled={!cryptPath || !keyPath || busy} onClick={doDecrypt}>
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
                <input type="checkbox" checked={deep} onChange={(e) => setDeep(e.target.checked)} />
                deep recovery — carve deleted records (freeblocks, freelist, WAL)
              </label>
              <button className="primary" disabled={!dbPath || busy} onClick={doRecover}>
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
                      <button
                        onClick={() => {
                          setResult(null); setReport(null); setDbPath(""); setCryptPath("");
                          setKeyPath(""); setDecryptNote(""); setFilter(""); setOnlyDeleted(false);
                          setExportedPath(""); setStep("scan");
                        }}
                      >
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
                    onChange={(e) => setFilter(e.target.value)}
                  />
                  <label className="check">
                    <input type="checkbox" checked={onlyDeleted} onChange={(e) => setOnlyDeleted(e.target.checked)} />
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
                  <button className="primary next" onClick={() => setStep("export")}>
                    Continue to Export →
                  </button>
                )}
              </div>
            </div>
          </section>
        )}

        {step === "export" && (
          <section className="workbench wb-export">
            {/* LEFT — export formats */}
            <div className="panel" style={{ gridArea: "left" }}>
              <div className="panel-bar">💾 Formats</div>
              <div className="panel-body">
                <button className="primary" disabled={!result || busy} onClick={() => doExport("json", "json")}>
                  {"{ }"} JSON
                </button>
                <button className="primary" disabled={!result || busy} onClick={() => doExport("csv", "csv")}>
                  ▦ CSV
                </button>
                <button className="primary" disabled={!result || busy} onClick={() => doExport("html", "html")}>
                  🌐 HTML report
                </button>
                <p className="hint">🛡 Every export embeds the source SHA-256 fingerprint for evidence integrity.</p>
              </div>
            </div>

            {/* CENTER — case summary */}
            <div className="panel" style={{ gridArea: "main" }}>
              <div className="panel-bar">📄 Case summary</div>
              <div className="panel-body">
                {!result ? (
                  <div className="empty">Nothing recovered yet — run step ③ first.</div>
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
                      Source fingerprint:{" "}
                      <span className="mono small" title={result.sha256}>{result.sha256 ?? "n/a"}</span>
                    </p>
                  </>
                )}
              </div>
            </div>

            {/* RIGHT — output */}
            <div className="panel" style={{ gridArea: "right" }}>
              <div className="panel-bar">📁 Output</div>
              <div className="panel-body">
                {exportedPath ? (
                  <>
                    <span className="pill ok">saved ✓</span>
                    <span className="mono small">{exportedPath}</span>
                    <button onClick={() => revealItemInDir(exportedPath)}>Show in Explorer</button>
                  </>
                ) : (
                  <p className="hint">No export written yet — choose a format on the left.</p>
                )}
              </div>
            </div>

            {/* TOP — strip */}
            <div className="panel timeline" style={{ gridArea: "strip" }}>
              <div className="panel-body tl">
                <span className="hint nowrap">
                  {result
                    ? `ready — ${result.total_messages} messages from ${result.db_path.split(/[\\/]/).pop()}`
                    : "waiting for step ③"}
                </span>
              </div>
            </div>
          </section>
        )}
        </main>
      </div>

      <footer>
        {status && !busy && <span className="pill ok">{status}</span>}
        {error && <span className="pill err" title={error}>{error.length > 110 ? error.slice(0, 110) + "…" : error}</span>}
        <span className="foot-right">read-only by design · recovery-core v0.1.0</span>
      </footer>
    </div>
  );
}
