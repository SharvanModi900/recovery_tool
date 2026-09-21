import { useCallback, useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openPath } from "@tauri-apps/plugin-opener";
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import type { AppInfo, Device, DeviceMirror, MediaFile, MediaInspect, MediaRecovery, ScanReport, UiRecovery, DirectAcquisition, AppInventory, WhatsAppDeepResult, RootCheckResult, InstagramData, MessengerData, TelegramData, ForensicExtraction } from "./types";
import { fmtSize } from "./types";
import { DirectDataViewer, ExportStep, ScanStep, MediaStep, AppsStep, CaseStep, RecoverStep, ConnectStep, ForensicsStep, DeviceToolsStep, MessagesStep, CallsStep, ContactsStep, SimForensicsStep, DebugConsole, StandaloneTimeline, ScreenMirrorPanel, DeletedPhotoGallery } from "./components";
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

type Step = "connect" | "apps" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "forensics" | "device_tools" | "recover" | "export" | "case" | "timeline";

export default function App() {
  const [step, setStep] = useState<Step>("connect");
  const [debugOpen, setDebugOpen] = useState(false);
  const [galleryOpen, setGalleryOpen] = useState(false);

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

  // Per-app extraction results
  const [appExtractResults, setAppExtractResults] = useState<Map<string, InstagramData | MessengerData | TelegramData | ForensicExtraction>>(new Map());
  const [appExtractBusy, setAppExtractBusy] = useState(false);

  // Direct acquisition (call logs, SMS, contacts)
  const [directData, setDirectData] = useState<DirectAcquisition | null>(null);
  const [directBusy, setDirectBusy] = useState(false);
  const [cameraBusy, setCameraBusy] = useState(false);
  const [cameraStatus, setCameraStatus] = useState<string>("");
  const [directView, setDirectView] = useState<"summary" | "full">("summary");
  const [connectTab, setConnectTab] = useState<"overview" | "data" | "whatsapp" | "calls" | "remote">("overview");
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
    controlMode, setControlMode,
    mirrorBusy, liveStream, streamFps, setStreamFps, streamMode, setStreamMode, isVideoFrame,
    paused, setPaused,
    headlessActive, headlessBusy, toggleHeadless,
    loadMirror, capture, startStream, stopStream, resetMirror,
    captures, autoCaptureActive, captureBusy, captureInterval, setCaptureInterval,
    loadCaptures, takeScreenshot, startAutoCaptureHandler, stopAutoCaptureHandler, deleteCapture,
  } = useDeviceMirror({ serial, step, setStatus, setError });

  const {
    callMonitorActive, callEvents, currentCallNotification,
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
   * One-click WhatsApp evidence read.
   * Checks for local crypt files first, tries quick key extraction, decrypts, parses.
   */
  const readWhatsApp = () =>
    run(async () => {
      setError("");
      setDecryptNote("");

      // ── Step 1: Find the encrypted/plain database ──
      // Check local evidence folders first before scanning the device
      setWaStage("Looking for WhatsApp database…");
      let localCrypt = "";

      // Check known local directories for crypt files
      const localDirs = [
        "D:\\recovery-tool\\evidence",
        "D:\\RecoverKit_Temp",
      ];
      for (const d of localDirs) {
        try {
          const stat = await api.scanFolder(d);
          const found = stat.artifacts?.find(a =>
            (a.kind === "backup" || a.kind === "database") && /\.crypt\d+$/i.test(a.path)
          );
          if (found) {
            localCrypt = found.path;
            break;
          }
        } catch { /* not found, continue */ }
      }

      // If no local crypt found, scan the device
      let db = "";
      let crypt = localCrypt;
      if (!crypt) {
        setWaStage("No local database found — scanning phone…");
        const [rep, pulled] = await api.scanDevice(serial, true, false);
        setReport(rep);
        db = pulled.find((p) => /msgstore\.db$/i.test(p)) ?? "";
        crypt =
          pulled.find((p) => /\.crypt\d+$/i.test(p) && !p.includes("increment")) ??
          pulled.find((p) => /\.crypt\d+$/i.test(p)) ??
          "";
      }
      if (db) setDbPath(db);
      if (crypt) setCryptPath(crypt);

      // ── Step 2: If we have a plain DB, parse it directly ──
      let finalDb = db;

      // ── Step 3: If we have a crypt file, try to get the key and decrypt ──
      if (!finalDb && crypt) {
        if (keyPath) {
          // Key already loaded — decrypt directly
          setWaStage("Decrypting the encrypted backup…");
          const out = await api.decryptBackup(crypt, keyPath, undefined, serial || undefined);
          setDbPath(out.output);
          setDecryptNote(
            out.format === "plain"
              ? "Input was already a plain SQLite database."
              : `Decrypted ${out.format} · key@${out.key_offset} iv@${out.iv_offset} · ${fmtSize(out.plaintext_size)}`
          );
          finalDb = out.output;
        } else {
          // No key loaded — try quick extraction methods (run-as, root, accessible paths, local files)
          setWaStage("Trying to extract WhatsApp key from device…");
          try {
            const deepResult = await api.extractWhatsappDeep(serial);
            setWaDeepResult(deepResult);
            if (deepResult.database_decrypted && deepResult.decrypted_db_path) {
              setWaStage("WhatsApp key extracted — database decrypted!");
              setDecryptNote(`Auto-extracted key via ${deepResult.key_method ?? "unknown"} — ${deepResult.total_messages} messages recovered`);
              finalDb = deepResult.decrypted_db_path;
            } else if (deepResult.key_obtained) {
              const extractedKeyPath = "D:\\RecoverKit_Temp\\whatsapp.key";
              setKeyPath(extractedKeyPath);
              setWaStage("Decrypting the encrypted backup…");
              const out = await api.decryptBackup(crypt, extractedKeyPath, undefined, serial || undefined);
              setDbPath(out.output);
              setDecryptNote(
                out.format === "plain"
                  ? "Input was already a plain SQLite database."
                  : `Decrypted ${out.format} · key@${out.key_offset} iv@${out.iv_offset} · ${fmtSize(out.plaintext_size)}`
              );
              finalDb = out.output;
            } else {
              throw new Error(
                "The encrypted database (crypt14) requires WhatsApp's 158-byte encryption key to decrypt. " +
                "This key is stored inside Android's protected app sandbox and cannot be accessed on a non-rooted device.\n\n" +
                "To proceed, you need ONE of:\n" +
                "• A rooted phone — use 📥 Fetch key in ③ Recover\n" +
                "• A previously extracted key file — use 🗝 Browse key in ③ Recover\n" +
                "• A WhatsApp chat export (.txt) — use 📄 Import in ③ Recover"
              );
            }
          } catch (deepErr: any) {
            if (deepErr?.message?.includes("requires WhatsApp")) throw deepErr;
            throw new Error(
              "Only an encrypted backup (.crypt) is accessible and no key file is loaded. " +
              "Provide the keyfile in ③ Recover, or import a .txt chat export."
            );
          }
        }
      }
      if (!finalDb) {
        throw new Error("No WhatsApp database could be located.");
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

  // Per-app forensic extraction (Instagram / Messenger / Telegram / generic)
  const extractApp = async (appId: string) => {
    if (!serial || appExtractBusy) return;
    setAppExtractBusy(true);
    setError("");
    try {
      let data: InstagramData | MessengerData | TelegramData | ForensicExtraction;
      switch (appId) {
        case "instagram":
          data = await api.extractInstagram(serial);
          break;
        case "messenger":
          data = await api.extractMessenger(serial);
          break;
        case "telegram":
          data = await api.extractTelegram(serial);
          break;
        default:
          data = await api.extractAppForensic(serial, appId);
          break;
      }
      setAppExtractResults(prev => {
        const next = new Map(prev);
        next.set(appId, data);
        return next;
      });
      const appName = appId.charAt(0).toUpperCase() + appId.slice(1);
      setStatus(`${appName} extraction complete`);
    } catch (e) {
      setError(`Extract ${appId} failed: ${typeof e === "string" ? e : (e as Error)?.message ?? String(e)}`);
    } finally {
      setAppExtractBusy(false);
    }
  };

  // Launch an app on the connected device via ADB monkey
  const [foregroundApp, setForegroundApp] = useState<string | null>(null);
  const launchAppOnDevice = async (pkg: string) => {
    if (!serial) return;
    try {
      setStatus(`Launching ${pkg} on device…`);
      await api.launchApp(serial, pkg);
      setStatus(`${pkg} launched on device`);
      // Check foreground app after launch
      setTimeout(async () => {
        try {
          const fg = await api.getForegroundApp(serial);
          setForegroundApp(fg || null);
        } catch {}
      }, 1000);
    } catch (e) {
      setError(`Launch failed: ${typeof e === "string" ? e : (e as Error)?.message ?? String(e)}`);
    }
  };

  // Poll foreground app periodically when on apps step
  useEffect(() => {
    if (step !== "apps" || !serial) return;
    const check = async () => {
      try {
        const fg = await api.getForegroundApp(serial);
        setForegroundApp(fg || null);
      } catch {}
    };
    check();
    const interval = setInterval(check, 3000);
    return () => clearInterval(interval);
  }, [step, serial]);

  // Read the inventory once on the first visit of the Apps step
  useEffect(() => {
    if (step === "apps" && serial && !apps && !appsBusy) loadApps();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [step, serial]);

  const openApp = async (a: AppInfo) => {
    setAppSel(a); // list row shows immediately, details stream in
    if (!serial) return;
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
      {/* Top Header Bar */}
      <header className="top-header">
        <div className="header-left">
          <div className="brand">
            <div className="logo-icon">
              <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <path d="M12 2L2 7l10 5 10-5-10-5z"/>
                <path d="M2 17l10 5 10-5"/>
                <path d="M2 12l10 5 10-5"/>
              </svg>
            </div>
            <div>
              <h1>RecoverKit</h1>
              <p>Read-only mobile forensics</p>
            </div>
          </div>
        </div>
        <div className="header-center">
          <div className="case-info">
            <svg className="case-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
            </svg>
            <span className="case-label">Case #RK-2026-00931</span>
            <span className="case-status active">Active</span>
          </div>
          {serial && mirror && (
            <div className="device-quick-info">
              <span>{mirror.model || "Device"}</span>
              <span className="dot-sep">·</span>
              <span>{serial.slice(0, 12)}</span>
            </div>
          )}
        </div>
        <div className="header-right">
          <button className="icon-btn" onClick={refreshDevices} disabled={busy} title="Refresh devices">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M23 4v6h-6"/>
              <path d="M1 20v-6h6"/>
              <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
            </svg>
          </button>
          <button className="icon-btn" onClick={() => setGalleryOpen(true)} title="Deleted Photo Recovery Gallery">
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
              <circle cx="8.5" cy="8.5" r="1.5"/>
              <polyline points="21 15 16 10 5 21"/>
            </svg>
          </button>
          <div className="user-avatar">SK</div>
        </div>
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
          {/* CASE Section */}
          <div className="nav-section">
            <div className="section-header">CASE</div>
            <button className={`nav-item ${step === "connect" ? "active" : ""}`} onClick={() => setStep("connect")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M3 9l9-7 9 7v11a2 2 0 01-2 2H5a2 2 0 01-2-2z"/>
                  <polyline points="9 22 9 12 15 12 15 22"/>
                </svg>
              </span>
              <span>Overview</span>
            </button>
            <button className={`nav-item ${step === "case" ? "active" : ""}`} onClick={() => setStep("case")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z"/>
                </svg>
              </span>
              <span>Evidence</span>
            </button>
            <button className={`nav-item ${step === "timeline" ? "active" : ""}`} onClick={() => setStep("timeline")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="12" cy="12" r="10"/>
                  <polyline points="12 6 12 12 16 14"/>
                </svg>
              </span>
              <span>Timeline</span>
            </button>
          </div>
          
          {/* DEVICE Section */}
          <div className="nav-section">
            <div className="section-header">DEVICE</div>

            <button className={`nav-item ${step === "apps" ? "active" : ""}`} onClick={() => setStep("apps")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="3" y="3" width="7" height="7"/>
                  <rect x="14" y="3" width="7" height="7"/>
                  <rect x="14" y="14" width="7" height="7"/>
                  <rect x="3" y="14" width="7" height="7"/>
                </svg>
              </span>
              <span>Apps</span>
            </button>
            <button className={`nav-item ${step === "messages" ? "active" : ""}`} onClick={() => setStep("messages")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M21 15a2 2 0 01-2 2H7l-4 4V5a2 2 0 012-2h14a2 2 0 012 2z"/>
                </svg>
              </span>
              <span>Messages</span>
            </button>
            <button className={`nav-item ${step === "calls" ? "active" : ""}`} onClick={() => setStep("calls")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M22 16.92v3a2 2 0 01-2.18 2 19.79 19.79 0 01-8.63-3.07 19.5 19.5 0 01-6-6 19.79 19.79 0 01-3.07-8.67A2 2 0 014.11 2h3a2 2 0 012 1.72 12.84 12.84 0 00.7 2.81 2 2 0 01-.45 2.11L8.09 9.91a16 16 0 006 6l1.27-1.27a2 2 0 012.11-.45 12.84 12.84 0 002.81.7A2 2 0 0122 16.92z"/>
                </svg>
              </span>
              <span>Calls</span>
            </button>
            <button className={`nav-item ${step === "contacts" ? "active" : ""}`} onClick={() => setStep("contacts")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M17 21v-2a4 4 0 00-4-4H5a4 4 0 00-4 4v2"/>
                  <circle cx="9" cy="7" r="4"/>
                  <path d="M23 21v-2a4 4 0 00-3-3.87"/>
                  <path d="M16 3.13a4 4 0 010 7.75"/>
                </svg>
              </span>
              <span>Contacts</span>
            </button>
            <button className={`nav-item ${step === "media" ? "active" : ""}`} onClick={() => setStep("media")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
                  <circle cx="8.5" cy="8.5" r="1.5"/>
                  <polyline points="21 15 16 10 5 21"/>
                </svg>
              </span>
              <span>Media</span>
            </button>
            <button className={`nav-item ${step === "sim_forensics" ? "active" : ""}`} onClick={() => setStep("sim_forensics")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M5 12.55a11 11 0 0114.08 0"/>
                  <path d="M1.42 9a16 16 0 0121.16 0"/>
                  <path d="M8.53 16.11a6 6 0 016.95 0"/>
                  <line x1="12" y1="20" x2="12.01" y2="20"/>
                </svg>
              </span>
              <span>SIM</span>
            </button>
          </div>
          
          {/* ANALYSIS Section */}
          <div className="nav-section">
            <div className="section-header">ANALYSIS</div>
            <button className={`nav-item ${step === "forensics" ? "active" : ""}`} onClick={() => setStep("forensics")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="11" cy="11" r="8"/>
                  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                </svg>
              </span>
              <span>Forensics</span>
            </button>
            <button className={`nav-item ${step === "device_tools" ? "active" : ""}`} onClick={() => setStep("device_tools")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M14.7 6.3a1 1 0 000 1.4l1.6 1.6a1 1 0 001.4 0l3.77-3.77a6 6 0 01-7.94 7.94l-6.91 6.91a2.12 2.12 0 01-3-3l6.91-6.91a6 6 0 017.94-7.94l-3.76 3.76z"/>
                </svg>
              </span>
              <span>Device Tools</span>
            </button>
            <button className={`nav-item ${step === "scan" ? "active" : ""}`} onClick={() => setStep("scan")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <circle cx="11" cy="11" r="8"/>
                  <line x1="21" y1="21" x2="16.65" y2="16.65"/>
                  <line x1="11" y1="8" x2="11" y2="14"/>
                  <line x1="8" y1="11" x2="14" y2="11"/>
                </svg>
              </span>
              <span>Search</span>
            </button>
            <button className={`nav-item ${step === "recover" ? "active" : ""}`} onClick={() => setStep("recover")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M23 4v6h-6"/>
                  <path d="M1 20v-6h6"/>
                  <path d="M3.51 9a9 9 0 0114.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0020.49 15"/>
                </svg>
              </span>
              <span>Recovery</span>
            </button>
          </div>
          
          {/* REPORT Section */}
          <div className="nav-section">
            <div className="section-header">REPORT</div>
            <button className={`nav-item ${step === "export" ? "active" : ""}`} onClick={() => setStep("export")}>
              <span className="nav-icon">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/>
                  <polyline points="17 8 12 3 7 8"/>
                  <line x1="12" y1="3" x2="12" y2="15"/>
                </svg>
              </span>
              <span>Export</span>
            </button>
          </div>

          {/* Device Status Footer */}
          {serial && (
            <div className="device-status-footer">
              <span className="status-dot connected"></span>
              <div className="status-text">
                <div>Device Connected</div>
                <div className="status-sub">Read-only mode</div>
              </div>
            </div>
          )}
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
            controlMode={controlMode} onControlModeChange={setControlMode}
            headlessActive={headlessActive} headlessBusy={headlessBusy} onToggleHeadless={toggleHeadless}
            paused={paused} onPausedChange={setPaused}
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
            onGoTo={(s: string) => setStep(s as Step)}
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
            appExtractResults={appExtractResults} appExtractBusy={appExtractBusy}
            foregroundApp={foregroundApp}
            screenMirror={
              <ScreenMirrorPanel
                serial={serial}
                shot={shot} shotTs={shotTs} shotDark={shotDark}
                mirrorBusy={mirrorBusy} mirrorErr={mirrorErr} isVideoFrame={isVideoFrame}
                liveStream={liveStream} streamFps={streamFps} streamMode={streamMode}
                autoRefresh={autoRefresh}
                mirror={mirror}
                controlMode={controlMode}
                onControlModeChange={setControlMode}
                headlessActive={headlessActive}
                headlessBusy={headlessBusy}
                onToggleHeadless={toggleHeadless}
                paused={paused}
                onPausedChange={setPaused}
                captures={captures} captureBusy={captureBusy}
                autoCaptureActive={autoCaptureActive} captureInterval={captureInterval}
                onCapture={capture}
                onStartStream={startStream}
                onStopStream={stopStream}
                onTakeScreenshot={takeScreenshot}
                onStartAuto={startAutoCaptureHandler}
                onStopAuto={stopAutoCaptureHandler}
                onIntervalChange={setCaptureInterval}
                onRefreshCaptures={loadCaptures}
                onDeleteCapture={deleteCapture}
                onAutoRefreshChange={setAutoRefresh}
                onStreamFpsChange={setStreamFps}
                onStreamModeChange={setStreamMode}
              />
            }
            onDetectApps={detectMessagingApps} onLoadApps={loadApps}
            onOpenApp={openApp}
            onExtractApp={extractApp}
            onLaunchApp={launchAppOnDevice}
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

        {step === "messages" && (
          <MessagesStep serial={serial} waDeepResult={waDeepResult} recovery={result} directData={directData} onGoTo={setStep} />
        )}

        {step === "calls" && (
          <CallsStep serial={serial} callEvents={callEvents} callRecordings={callRecordings} directData={directData} callMonitorActive={callMonitorActive} onGoTo={setStep} />
        )}

        {step === "contacts" && (
          <ContactsStep serial={serial} recovery={result} directData={directData} onGoTo={setStep} />
        )}

        {step === "sim_forensics" && (
          <SimForensicsStep serial={serial}
            simSnapshot={simSnapshot} simBaseline={simBaseline} simHistorical={simHistorical} simVerdict={simVerdict} simHistory={simHistory} simBusy={simBusy}
            onCollectSim={collectSim} onSimBaseline={applySimBaseline} onSimSwapCheck={runSimSwapCheck} onDeleteSimSnap={deleteSimSnap}
            onGoTo={setStep} />
        )}

        {step === "forensics" && (
          <ForensicsStep serial={serial} onGoTo={setStep} />
        )}

        {step === "device_tools" && (
          <DeviceToolsStep serial={serial} onGoTo={setStep} />
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

      {galleryOpen && (
        <DeletedPhotoGallery serial={serial} outputDir="D:\\recovery-tool\\workspace" onClose={() => setGalleryOpen(false)} />
      )}
    </div>
  );
}
