import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import type { AppInfo, Device, DeviceMirror, MediaFile, MediaInspect, MediaRecovery, ScanReport, UiRecovery, DirectAcquisition, AppInventory, WhatsAppDeepResult, RootCheckResult } from "./types";
import { fmtSize } from "./types";
import { DirectDataViewer, ExportStep, ScanStep, MediaStep, AppsStep, CaseStep, RecoverStep, ConnectStep, CallNotificationPanel, DeepForensicsStep, DeletedDataStep, BrowserForensicsStep, NetworkLocationStep, DebugConsole, DeviceDetailsPanel, StandaloneTimeline } from "./components";
import { useDeviceMirror } from "./hooks/useDeviceMirror";
import { useCallMonitor } from "./hooks/useCallMonitor";
import { useSimForensics } from "./hooks/useSimForensics";
import { useCaseManagement } from "./hooks/useCaseManagement";

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

type Step = "connect" | "apps" | "scan" | "media" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

const STEPS: { id: Step; label: string; icon: string }[] = [
  { id: "connect", label: "Connect", icon: "📱" },
  { id: "apps", label: "Apps", icon: "📦" },
  { id: "scan", label: "Scan", icon: "🔎" },
  { id: "media", label: "Media", icon: "🖼" },
  { id: "deep", label: "Forensics", icon: "🔬" },
  { id: "timeline", label: "Timeline", icon: "🕐" },
  { id: "recover", label: "Recover", icon: "⚙" },
  { id: "export", label: "Export", icon: "📤" },
  { id: "case", label: "Case", icon: "🗂" },
];

// Workflow phases — groups related steps for visual navigation
const PHASES: { label: string; icon: string; steps: Step[] }[] = [
  { label: "Collect", icon: "📥", steps: ["connect", "apps", "scan", "media"] },
  { label: "Analyze", icon: "🔬", steps: ["deep", "timeline", "recover"] },
  { label: "Report", icon: "📊", steps: ["export", "case"] },
];

// Sub-steps hidden from sidebar but still accessible
const FORENSICS_SUB: { id: Step; label: string; icon: string }[] = [
  { id: "deep", label: "Deep Scan", icon: "🔍" },
  { id: "deleted", label: "Deleted Data", icon: "🗑" },
  { id: "browser", label: "Browser", icon: "🌐" },
  { id: "network", label: "Network", icon: "📍" },
];

export default function App() {
  const [step, setStep] = useState<Step>("connect");
  const [debugOpen, setDebugOpen] = useState(false);
  const [infoBarOpen, setInfoBarOpen] = useState(false);
  const [deviceDetailsOpen, setDeviceDetailsOpen] = useState(false);

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
  const [cameraBusy, setCameraBusy] = useState(false);
  const [cameraStatus, setCameraStatus] = useState<string>("");
  const [micBusy, setMicBusy] = useState(false);
  const [micStatus, setMicStatus] = useState<string>("");
  const [micDuration, setMicDuration] = useState<number>(5);
  const [directView, setDirectView] = useState<"summary" | "full">("summary");
  const [connectTab, setConnectTab] = useState<"overview" | "data" | "whatsapp" | "calls" | "sim">("overview");
  const [chargingStopped, setChargingStopped] = useState(false);
  const [waDeepResult, setWaDeepResult] = useState<WhatsAppDeepResult | null>(null);
  const [waDeepBusy, setWaDeepBusy] = useState(false);
  const [rootCheck, setRootCheck] = useState<RootCheckResult | null>(null);
  const [rootBusy, setRootBusy] = useState(false);
  



  




  // Global
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [status, setStatus] = useState("");
  const [exportedPath, setExportedPath] = useState("");

  // ── Extracted hooks ──────────────────────────────────────────────────
  const {
    mirror, shot, shotTs, shotDark, mirrorErr,
    autoRefresh, setAutoRefresh,
    mirrorBusy, liveStream, streamFps, setStreamFps, streamMode, setStreamMode, isVideoFrame,
    loadMirror, capture, startStream, stopStream, resetMirror,
    captures, autoCaptureActive, captureBusy, captureInterval, setCaptureInterval,
    loadCaptures, takeScreenshot, startAutoCaptureHandler, stopAutoCaptureHandler, deleteCapture,
  } = useDeviceMirror({ serial, step, setStatus, setError });

  const {
    callMonitorActive, callEvents, setCallEvents, currentCallNotification,
    callRecordings, playingRec, timeline,
    startCallMonitor, stopCallMonitor, loadCallRecordings,
    playRecording, setPlayingRec, playBellSound,
  } = useCallMonitor({ serial, setStatus, setError });

  const {
    simSnapshot, simBaseline, simHistorical, simVerdict, simHistory, simBusy,
    loadSimHistory, collectSim, runSimSwapCheck, applySimBaseline, deleteSimSnap,
  } = useSimForensics({ serial, setStatus, setError, playBellSound });

  const {
    cases, activeCaseId, caseSummary, caseEvidence, caseAudit,
    caseBusy, caseMsg, caseForm, caseEvidenceFilter,
    custodyHighlight, custodyRef,
    selectCase, handleCreateCase, handleConsolidate, handleVerifyChain,
    handleExportCase, handleCloseCase, handleDeleteCase,
    setCaseForm, setCaseEvidenceFilter, loadCases,
  } = useCaseManagement({ serial, deviceModel: activeSession?.model, step, setStep, setStatus, setError });

  // ── Step completion tracking (drives nav badges + progress bar) ────────
  const stepDone = useMemo(() => ({
    connect: !!serial && directData != null,
    apps: appInventory != null,
    scan: report != null,
    media: liveMedia != null || captures.length > 0,
    deep: false,
    deleted: false,
    browser: false,
    network: false,
    recover: result != null,
    export: !!exportedPath,
    case: caseSummary != null,
    timeline: false,
  }), [serial, directData, appInventory, report, liveMedia, captures.length, result, exportedPath, caseSummary]);

  const phaseDone = useMemo(() =>
    Object.fromEntries(PHASES.map(p => [p.label, p.steps.every(s => stepDone[s])])),
    [stepDone]
  );

  const progress = useMemo(() => {
    const done = Object.values(stepDone).filter(Boolean).length;
    return { done, total: STEPS.length, pct: Math.round((done / STEPS.length) * 100) };
  }, [stepDone]);

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

  const doCapturePhoto = () => {
    setCameraBusy(true);
    setCameraStatus("Opening camera app on device...");
    api.captureDevicePhoto(serial)
      .then((result) => {
        if (result.success && result.local_path) {
          setCameraStatus(`📸 Photo captured! File: ${result.filename}, Size: ${fmtSize(result.size)}`);
          setStatus(`Photo captured — ${result.filename} (${fmtSize(result.size)}) saved to ${result.local_path}`);
        } else {
          setCameraStatus(`❌ No photo captured. ${result.notes.join(" | ")}`);
        }
      })
      .catch((err) => {
        setCameraStatus(`❌ Error: ${err}`);
      })
      .finally(() => {
        setCameraBusy(false);
        setTimeout(() => setCameraStatus(""), 10000);
      });
  };

  const doCaptureMic = () => {
    setMicBusy(true);
    setMicStatus(`🎤 Recording ${micDuration} seconds from device mic...`);
    api.captureDeviceMic(serial, micDuration)
      .then((result) => {
        if (result.success && result.local_path) {
          setMicStatus(`🎤 Audio captured! File: ${result.filename}, Size: ${fmtSize(result.size)}, Duration: ${result.duration}s`);
          setStatus(`Audio captured — ${result.filename} (${fmtSize(result.size)}, ${result.duration}s) saved to ${result.local_path}`);
        } else {
          setMicStatus(`❌ No audio captured. ${result.notes.join(" | ")}`);
        }
      })
      .catch((err) => {
        setMicStatus(`❌ Error: ${err}`);
      })
      .finally(() => {
        setMicBusy(false);
        setTimeout(() => setMicStatus(""), 10000);
      });
  };

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

  const doWhatsappDowngrade = () =>
    run(async () => {
      setWaDeepBusy(true);
      try {
        const data = await api.extractWhatsappDowngrade(serial);
        setWaDeepResult(data);
        const status = data.database_decrypted
          ? `WhatsApp downgrade extraction complete — ${data.total_messages} messages, ${data.total_contacts} chats`
          : `Downgrade: ${data.key_obtained ? "Key obtained" : "No key"}, ${data.notes.slice(-1)[0] || "check notes"}`;
        setStatus(status);
        return data;
      } finally {
        setWaDeepBusy(false);
      }
    }, "WhatsApp downgrade extraction complete");







  // Wrapper: the hook returns the path; we also need to track it for the Export step.
  const doExportCase = async (format: "json" | "html") => {
    const path = await handleExportCase(format);
    if (path) setExportedPath(path);
  };







  // Load SIM history + take one live snapshot when a device connects
  useEffect(() => {
    if (!serial) return;
    loadSimHistory();
    const t = setTimeout(() => { collectSim(); }, 1200);
    return () => clearTimeout(t);
  }, [serial]); // eslint-disable-line react-hooks/exhaustive-deps

  const doProvideKey = async () => {
    const f = await open({
      title: "Select WhatsApp key file",
      filters: [{ name: "Key files", extensions: ["key", "*"] }],
    });
    if (!f) return;
    
    const userKeyPath = f as string;
    setWaDeepBusy(true);
    try {
      if (!waDeepResult?.crypt_file_path) {
        setStatus("No encrypted database found. Run Auto-Extract first to locate the database.");
        return;
      }
      
      // Step 1: Decrypt the database with the user-provided key
      const outcome = await api.decryptBackup(
        waDeepResult.crypt_file_path,
        userKeyPath,
        "D:\\RecoverKit_Temp\\msgstore_decrypted.db"
      );
      setStatus(`Decrypted! Format: ${outcome.format}, key@${outcome.key_offset}, iv@${outcome.iv_offset}`);
      
      // Step 2: Parse the decrypted database to get messages
      const res = await api.recoverDb("D:\\RecoverKit_Temp\\msgstore_decrypted.db", true, serial || undefined);
      setResult(res);
      
      // Step 3: Update waDeepResult to reflect successful decryption
      setWaDeepResult(prev => prev ? {
        ...prev,
        key_obtained: true,
        key_method: "user-provided file",
        database_decrypted: true,
        decrypted_db_path: "D:\\RecoverKit_Temp\\msgstore_decrypted.db",
        total_messages: res.total_messages,
        total_contacts: res.chats,
        notes: [...prev.notes, `Decrypted with user-provided key: ${outcome.format}`],
      } : null);
    } catch (e: any) {
      setStatus(`Key provision failed: ${e.message || e}`);
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



  useEffect(() => {
    // A different device means a different inventory
    setApps(null);
    setAppSel(null);
    setLostSet(null);
    setPresenceAt(0);
    if (!serial) {
      resetMirror();
      return;
    }
    loadMirror(serial);
    capture(serial);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial]);



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
        {serial && mirror && (
          <button
            className="device-info-toggle"
            onClick={() => setInfoBarOpen(!infoBarOpen)}
            title="Device info"
          >
            {mirror.model || "Device"} · {mirror.android || "—"} · 🔋{mirror.battery}
            <span style={{ marginLeft: 4, fontSize: 8 }}>{infoBarOpen ? "▲" : "▼"}</span>
          </button>
        )}
        <div style={{ flex: 1 }} />
        <CallNotificationPanel callEvents={callEvents} onClear={() => setCallEvents([])} />
      </header>

      {/* Collapsible device info bar */}
      {infoBarOpen && serial && mirror && (
        <div className="device-info-bar">
          <span><i>Model</i> {mirror.model || "—"}</span>
          <span><i>Android</i> {mirror.android || "—"}</span>
          <span><i>Battery</i> {mirror.battery}</span>
          <span><i>Storage</i> {mirror.storage}</span>
          <span><i>Screen</i> {mirror.resolution}</span>
          <span><i>Uptime</i> {mirror.uptime}</span>
          <span><i>WhatsApp</i> {mirror.whatsapp}</span>
          <button className="mini" disabled={mirrorBusy} onClick={() => loadMirror(serial)} title="Refresh">⟳</button>
          <button className="mini" onClick={() => setDeviceDetailsOpen(true)} style={{ background: "var(--saffron)", color: "#fff", border: "1px solid var(--saffron-deep)" }}>
            📋 Full Details
          </button>
        </div>
      )}
      
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
          {/* Progress indicator */}
          <div className="sidebar-progress" title={`${progress.done} of ${progress.total} steps complete`}>
            <div className="progress-bar">
              <div className="progress-fill" style={{ width: `${progress.pct}%` }} />
            </div>
            <span className="progress-label">{progress.done}/{progress.total} complete</span>
          </div>

          {PHASES.map((phase) => (
            <div key={phase.label} className="nav-phase-group">
              <div className="phase-header">
                <span>{phase.icon}</span>
                <span>{phase.label}</span>
                {phaseDone[phase.label] && <span className="phase-check">✓</span>}
              </div>
              {phase.steps.map((stepId) => {
                const s = STEPS.find(st => st.id === stepId)!;
                const done = stepDone[stepId];
                const isActive = step === stepId;
                const isForensics = stepId === "deep";
                const showSub = isForensics && ["deep", "deleted", "browser", "network"].includes(step);
                return (
                  <div key={s.id}>
                    <button
                      className={`nav${isActive || showSub ? " active" : ""}${done ? " done" : ""}`}
                      onClick={() => setStep(s.id)}
                      title={s.label}
                    >
                      <span className="nav-icon">{s.icon}</span>
                      <span className="nav-label">{s.label}</span>
                      {done && <span className="nav-check">✓</span>}
                    </button>
                    {/* Forensics sub-tabs — shown only when inside forensic steps */}
                    {showSub && (
                      <div className="nav-sub">
                        {FORENSICS_SUB.map((sub) => (
                          <button
                            key={sub.id}
                            className={`nav-sub-item${step === sub.id ? " active" : ""}`}
                            onClick={() => setStep(sub.id)}
                          >
                            <span>{sub.icon}</span> {sub.label}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          ))}
        </aside>

        <main>
        {step === "connect" && (
          <>
          <ConnectStep
            serial={serial} busy={busy} devices={devices}
            connectTab={connectTab} setConnectTab={setConnectTab}
            directData={directData} directBusy={directBusy}
            onAcquire={doDirectAcquisition} onViewFull={() => setDirectView("full")}
            cameraBusy={cameraBusy} cameraStatus={cameraStatus} onCapturePhoto={doCapturePhoto}
            micBusy={micBusy} micStatus={micStatus} micDuration={micDuration} onMicDurationChange={setMicDuration} onCaptureMic={doCaptureMic}
            waDeepResult={waDeepResult} waDeepBusy={waDeepBusy}
            rootCheck={rootCheck} rootBusy={rootBusy}
            onWhatsappDeep={doWhatsappDeep} onCheckRoot={doCheckRoot}
            onWhatsappDowngrade={doWhatsappDowngrade}
            onImportTxt={doImportTxt} onProvideKey={doProvideKey}
            callMonitorActive={callMonitorActive} callEvents={callEvents}
            currentCallNotification={currentCallNotification}
            callRecordings={callRecordings} playingRec={playingRec}
            timeline={timeline}
            onStartMonitor={startCallMonitor} onStopMonitor={stopCallMonitor}
            onLoadRecordings={loadCallRecordings}
            onPlayRecording={playRecording} onStopPlaying={() => setPlayingRec(null)}
            simSnapshot={simSnapshot} simBaseline={simBaseline}
            simHistorical={simHistorical} simVerdict={simVerdict}
            simHistory={simHistory} simBusy={simBusy}
            onCollectSim={collectSim} onSimBaseline={applySimBaseline}
            onSimSwapCheck={runSimSwapCheck} onDeleteSimSnap={deleteSimSnap}
            shot={shot} shotTs={shotTs} shotDark={shotDark}
            mirrorBusy={mirrorBusy} mirrorErr={mirrorErr} isVideoFrame={isVideoFrame}
            liveStream={liveStream} streamFps={streamFps} streamMode={streamMode}
            autoRefresh={autoRefresh}
            captures={captures} captureBusy={captureBusy}
            autoCaptureActive={autoCaptureActive} captureInterval={captureInterval}
            onCapture={capture} onStartStream={startStream} onStopStream={stopStream}
            onTakeScreenshot={takeScreenshot}
            onStartAuto={startAutoCaptureHandler} onStopAuto={stopAutoCaptureHandler}
            onIntervalChange={setCaptureInterval}
            onRefreshCaptures={loadCaptures} onDeleteCapture={deleteCapture}
            onAutoRefreshChange={setAutoRefresh}
            onStreamFpsChange={setStreamFps} onStreamModeChange={setStreamMode}
            mirror={mirror}
            onRingDevice={() => {
              if (!serial) return;
              setStatus("Ringing device...");
              api.playDeviceRingtone(serial).then(() => {
                setStatus("Device is ringing!");
              }).catch((e: unknown) => setStatus(`Ring failed: ${e}`));
            }}
            onLockDevice={() => {
              if (!serial) return;
              setStatus("Locking device...");
              api.lockdownDevice(serial).then(() => {
                setStatus("Device locked!");
              }).catch((e: unknown) => setStatus(`Lock failed: ${e}`));
            }}
            onToggleCharging={() => {
              if (!serial) return;
              if (chargingStopped) {
                setStatus("Resuming charging...");
                api.startCharging(serial).then(() => {
                  setChargingStopped(false);
                  setStatus("Charging resumed");
                }).catch((e: unknown) => setStatus(`Failed: ${e}`));
              } else {
                setStatus("Stopping charging...");
                api.stopCharging(serial).then(() => {
                  setChargingStopped(true);
                  setStatus("Charging stopped");
                }).catch((e: unknown) => setStatus(`Failed: ${e}`));
              }
            }}
            chargingStopped={chargingStopped}
            onGoTo={setStep}
          />
          {directView === "full" && directData && (
            <DirectDataViewer
              data={directData}
              serial={serial}
              onClose={() => setDirectView("summary")}
              onStatus={setStatus}
            />
          )}
          </>
        )}

        {step === "apps" && (
          <AppsStep
            serial={serial} busy={busy}
            appInventory={appInventory} appInventoryBusy={appInventoryBusy}
            apps={apps ?? null} appsBusy={appsBusy}
            appFilter={appFilter} showSystem={showSystem}
            shownApps={shownApps} appSel={appSel} waStage={waStage}
            result={result}
            onDetectApps={detectMessagingApps} onLoadApps={loadApps}
            onOpenApp={openApp}
            onFilterChange={setAppFilter} onShowSystemChange={setShowSystem}
            onReadWhatsApp={readWhatsApp}
            onGoTo={setStep}
          />
        )}

        {step === "scan" && (
          <ScanStep
            serial={serial} busy={busy} includeMedia={includeMedia}
            report={report} dbPath={dbPath} cryptPath={cryptPath}
            onScanDevice={doScanDevice} onScanFolder={doScanFolder}
            onIncludeMediaChange={setIncludeMedia} onGoTo={setStep}
          />
        )}

        {step === "media" && (
          <MediaStep
            serial={serial} busy={busy} mediaBusy={mediaBusy} media={media}
            mediaProgress={mediaProgress} viewMedia={viewMedia} gridFiles={gridFiles}
            preview={preview} inspect={inspect} inspecting={inspecting}
            showImages={showImages} showVideos={showVideos}
            lostSet={lostSet} presenceBusy={presenceBusy} presenceAt={presenceAt}
            onRecoverMedia={doRecoverMedia} onSaveMedia={doSaveMedia}
            onOpenMediaDir={doOpenMediaDir} onVerifyOnDevice={verifyOnDevice}
            onOpenPreview={openPreview} onInspect={doInspect}
            onShowImagesChange={setShowImages} onShowVideosChange={setShowVideos}
            onGoTo={setStep}
          />
        )}

        {step === "deep" && (
          <DeepForensicsStep serial={serial} onGoTo={setStep} />
        )}

        {step === "deleted" && (
          <DeletedDataStep serial={serial} onGoTo={setStep} />
        )}

        {step === "browser" && (
          <BrowserForensicsStep serial={serial} onGoTo={setStep} />
        )}

        {step === "network" && (
          <NetworkLocationStep serial={serial} onGoTo={setStep} />
        )}

        {step === "recover" && (
          <RecoverStep
            serial={serial} busy={busy}
            dbPath={dbPath} cryptPath={cryptPath} keyPath={keyPath}
            deep={deep} result={result} filter={filter} onlyDeleted={onlyDeleted}
            decryptNote={decryptNote}
            filtered={filtered}
            dbArtifacts={dbArtifacts} cryptArtifacts={cryptArtifacts}
            nameOf={nameOf}
            onDbPathChange={setDbPath} onCryptPathChange={setCryptPath}
            onKeyPathChange={setKeyPath} onDeepChange={setDeep}
            onFilterChange={setFilter} onOnlyDeletedChange={setOnlyDeleted}
            onRecover={doRecover} onDecrypt={doDecrypt}
            onImportTxt={doImportTxt} onImportSms={doImportSms} onImportVcf={doImportVcf}
            onClearDemo={() => {
              setResult(null); setReport(null); setDbPath(""); setCryptPath("");
              setKeyPath(""); setDecryptNote(""); setFilter(""); setOnlyDeleted(false);
              setExportedPath(""); setStep("scan");
            }}
            onGoTo={setStep}
            onFetchKey={() => run(async () => {
              const p = await api.fetchKey(serial);
              setKeyPath(p);
              return p;
            }, "Key fetched from phone ✓")}
          />
        )}

        {step === "export" && (
          <ExportStep
            result={result} busy={busy} exportedPath={exportedPath}
            onExport={doExport} onGoTo={setStep}
          />
        )}

        {step === "case" && (
          <CaseStep
            serial={serial}
            cases={cases} activeCaseId={activeCaseId}
            caseSummary={caseSummary} caseEvidence={caseEvidence}
            caseAudit={caseAudit} caseBusy={caseBusy} caseMsg={caseMsg}
            caseForm={caseForm} caseEvidenceFilter={caseEvidenceFilter}
            custodyHighlight={custodyHighlight} custodyRef={custodyRef}
            exportedPath={exportedPath}
            onSelectCase={selectCase} onCreateCase={handleCreateCase}
            onConsolidate={handleConsolidate} onVerifyChain={handleVerifyChain}
            onExportCase={doExportCase} onCloseCase={handleCloseCase}
            onDeleteCase={handleDeleteCase}
            onCaseFormChange={(field, value) => setCaseForm(prev => ({ ...prev, [field]: value }))}
            onEvidenceFilterChange={setCaseEvidenceFilter}
            onLoadCases={loadCases}
            onGoToTimeline={() => setStep("timeline")}
          />
        )}

        {step === "timeline" && (
          <StandaloneTimeline serial={serial} />
        )}
        </main>
      </div>

      <footer>
        {status && !busy && <span className="pill ok">{status}</span>}
        {error && <span className="pill err" title={error}>{error.length > 110 ? error.slice(0, 110) + "…" : error}</span>}
        <span className="foot-right">
          <button
            className="mini"
            onClick={() => setDebugOpen(!debugOpen)}
            title="Toggle debug console"
            style={{
              background: debugOpen ? "#3498db" : "transparent",
              color: debugOpen ? "#fff" : "var(--muted)",
              border: debugOpen ? "1px solid #2980b9" : "1px solid var(--line)",
              marginRight: 8,
            }}
          >
            🐛 Debug
          </button>
          read-only by design · v0.1.0
        </span>
      </footer>

      {/* Debug Console */}
      <DebugConsole
        serial={serial}
        isOpen={debugOpen}
        onClose={() => setDebugOpen(false)}
      />

      {/* Device Details Overlay */}
      {deviceDetailsOpen && (
        <div className="device-details-overlay">
          <div className="device-details-header">
            <h2>📱 Device Details</h2>
            <button className="mini" onClick={() => setDeviceDetailsOpen(false)}>✕ Close</button>
          </div>
          <div className="device-details-body">
            <DeviceDetailsPanel serial={serial} />
          </div>
        </div>
      )}
    </div>
  );
}
