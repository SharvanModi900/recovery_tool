import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Device, DeviceMirror, IconInfo, ScanReport, DecryptOutcome, MediaInspect, MediaRecovery, ScreenshotInfo, UiRecovery, DirectAcquisition, AppInventory, InstagramData, MessengerData, TelegramData, ForensicExtraction, WhatsAppDeepResult, RootCheckResult, CallEvent, CallRecording, ScreenCapture, SimSnapshot, SimSwapReport, Case, CaseSummary, EvidenceItem, AuditEntry, ChainStatus, TimelinePage } from "./types";

export const listDevices = () => invoke<Device[]>("list_devices");

export const scanFolder = (path: string) => invoke<ScanReport>("scan_folder", { path });

export const scanDevice = (serial: string, pull: boolean, includeMedia: boolean) =>
  invoke<[ScanReport, string[]]>("scan_device", { serial, pull, includeMedia });

export const decryptBackup = (input: string, keyFile: string, output?: string, serial?: string) =>
  invoke<DecryptOutcome>("decrypt_backup", { input, keyFile, output, serial });

export const fetchKey = (serial: string, remotePath?: string) =>
  invoke<string>("fetch_key", { serial, remotePath });

export const recoverDb = (dbPath: string, deep: boolean, serial?: string) =>
  invoke<UiRecovery>("recover_db", { dbPath, deep, serial });

export const importChatTxt = (path: string) =>
  invoke<UiRecovery>("import_chat_txt", { path });

export const importSmsXml = (path: string) =>
  invoke<UiRecovery>("import_sms_xml", { path });

export const importVcf = (path: string) =>
  invoke<UiRecovery>("import_vcf", { path });

export const recoverMedia = (serial: string, source?: string) =>
  invoke<MediaRecovery>("recover_media", { serial, source });

export const saveMedia = (serial: string, dest: string) =>
  invoke<string>("save_media", { serial, dest });

export const mediaDir = (serial: string) =>
  invoke<string>("media_dir", { serial });

export const inspectMedia = (path: string) => invoke<MediaInspect>("inspect_media", { path });

export const deviceMirror = (serial: string) => invoke<DeviceMirror>("device_mirror", { serial });

export const deviceScreenshot = (serial: string) => invoke<ScreenshotInfo>("device_screenshot", { serial });

export const startScreenStream = (serial: string, fps: number = 10, mode: string = "normal") =>
  invoke<void>("start_screen_stream", { serial, fps, mode });

export const stopScreenStream = (serial: string) =>
  invoke<void>("stop_screen_stream", { serial });

export const devicePresence = (serial: string) => invoke<string[]>("device_presence", { serial });

export const listApps = (serial: string) => invoke<AppInfo[]>("list_apps", { serial });

export const appDetails = (serial: string, pkg: string) =>
  invoke<AppInfo>("app_details", { serial, package: pkg });

export const appIcon = (serial: string, pkg: string) =>
  invoke<IconInfo>("app_icon", { serial, package: pkg });

export const directAcquisition = (serial: string) =>
  invoke<DirectAcquisition>("direct_acquisition", { serial });

export const pullRecording = (serial: string, remotePath: string) =>
  invoke<string>("pull_recording", { serial, remotePath });

export const detectApps = (serial: string) =>
  invoke<AppInventory>("detect_apps", { serial });

export const getAppMedia = (serial: string, appId: string) =>
  invoke<[string, number][]>("get_app_media", { serial, appId });

export const extractInstagram = (serial: string) =>
  invoke<InstagramData>("extract_instagram", { serial });

export const extractMessenger = (serial: string) =>
  invoke<MessengerData>("extract_messenger", { serial });

export const extractTelegram = (serial: string) =>
  invoke<TelegramData>("extract_telegram", { serial });

export const extractAppForensic = (serial: string, appId: string) =>
  invoke<ForensicExtraction>("extract_app_forensic", { serial, appId });

export const extractWhatsappDeep = (serial: string) =>
  invoke<WhatsAppDeepResult>("extract_whatsapp_deep", { serial });

export const checkRoot = (serial: string) =>
  invoke<RootCheckResult>("check_root", { serial });

export const pollCallState = (serial: string) =>
  invoke<CallEvent | null>("poll_call_state", { serial });

export const checkRecentCalls = (serial: string, sinceMs: number) =>
  invoke<CallEvent[]>("check_recent_calls", { serial, sinceMs });

export const getCallNotification = (serial: string) =>
  invoke<string | null>("get_call_notification", { serial });

export const pollLiveCall = (serial: string) =>
  invoke<CallEvent | null>("poll_live_call", { serial });

export const getCallRecordings = (serial: string) =>
  invoke<CallRecording[]>("get_call_recordings", { serial });

export const takeScreenshotCapture = (serial: string, note?: string) =>
  invoke<ScreenCapture>("take_screenshot_capture", { serial, note });

export const getScreenCaptures = (serial?: string, limit?: number) =>
  invoke<ScreenCapture[]>("get_screen_captures", { serial, limit });

export const deleteScreenCapture = (id: number) =>
  invoke<void>("delete_screen_capture", { id });

export const startAutoCapture = (serial: string, intervalSecs?: number) =>
  invoke<void>("start_auto_capture", { serial, intervalSecs });

export const stopAutoCapture = () =>
  invoke<void>("stop_auto_capture");

export const collectSimSnapshot = (serial: string) =>
  invoke<SimSnapshot>("collect_sim_snapshot", { serial });

export const getSimHistory = (serial?: string, limit?: number) =>
  invoke<SimSnapshot[]>("get_sim_history", { serial, limit });

export const setSimBaseline = (id: number) =>
  invoke<void>("set_sim_baseline", { id });

export const deleteSimSnapshot = (id: number) =>
  invoke<void>("delete_sim_snapshot", { id });

export const checkSimSwap = (serial: string) =>
  invoke<SimSwapReport>("check_sim_swap", { serial });

export const exportResult = (format: string, dest: string) =>
  invoke<string>("export_result", { format, dest });

// ── Consolidated case file + chain of custody ──────────────────────────

export const createCase = (fields: {
  caseNumber?: string;
  title?: string;
  examiner?: string;
  subject?: string;
  deviceSerial?: string | null;
  deviceModel?: string | null;
  notes?: string;
}) => invoke<Case>("create_case", fields);

export const listCases = () => invoke<Case[]>("list_cases");

export const getCaseSummary = (caseId: number) =>
  invoke<CaseSummary>("get_case_summary", { caseId });

export const getCaseEvidence = (caseId: number) =>
  invoke<EvidenceItem[]>("get_case_evidence", { caseId });

export const getCaseAudit = (caseId: number) =>
  invoke<AuditEntry[]>("get_case_audit", { caseId });

export const verifyCaseChain = (caseId: number) =>
  invoke<ChainStatus>("verify_case_chain", { caseId });

export const consolidateCase = (caseId: number, serial: string, actor: string) =>
  invoke<CaseSummary>("consolidate_case", { caseId, serial, actor });

export const closeCase = (caseId: number, actor?: string) =>
  invoke<void>("close_case", { caseId, actor });

export const deleteCase = (caseId: number) =>
  invoke<void>("delete_case", { caseId });

export const exportCase = (caseId: number, format: "json" | "html") =>
  invoke<string>("export_case", { caseId, format });

export const getCaseTimeline = (
  caseId: number,
  opts: { kind?: string | null; q?: string | null; from?: number | null; to?: number | null; limit?: number; offset?: number } = {},
) =>
  invoke<TimelinePage>("get_case_timeline", {
    caseId,
    kind: opts.kind ?? null,
    q: opts.q ?? null,
    from: opts.from ?? null,
    to: opts.to ?? null,
    limit: opts.limit ?? 500,
    offset: opts.offset ?? 0,
  });

export const ingestRecoveredMessages = (caseId: number, actor?: string) =>
  invoke<number>("ingest_recovered_messages", { caseId, actor: actor ?? null });
