import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Device, DeviceMirror, DeviceFullInfo, IconInfo, ScanReport, DecryptOutcome, MediaInspect, MediaRecovery, ScreenshotInfo, UiRecovery, DirectAcquisition, AppInventory, InstagramData, MessengerData, TelegramData, ForensicExtraction, WhatsAppDeepResult, RootCheckResult, CallEvent, CallRecording, ScreenCapture, SimSnapshot, SimSwapReport, Case, CaseSummary, EvidenceItem, AuditEntry, ChainStatus, TimelinePage, WifiNetwork, BluetoothDevice, DeviceAccount, PowerEvent, NotificationEntry, DictionaryWord, DeletedSmsResult, ThumbnailRecoveryResult, CarveResult, BrowserForensicsResult, LocationForensicsResult, NetworkForensicsResult, ApkAnalysisResult, UnifiedEvent, SourceReport, CameraCaptureResult, MicCaptureResult, FastbootDevice, PartitionInfo, RemoteAgentState } from "./types";

export const listDevices = () => invoke<Device[]>("list_devices");

// ── Fastboot (bootloader mode — no USB debugging required) ──
export const listFastbootDevices = () => invoke<FastbootDevice[]>("list_fastboot_devices");
export const fastbootGetvarAll = (serial: string) => invoke<[string, string][]>("fastboot_getvar_all", { serial });
export const fastbootListPartitions = (serial: string) => invoke<PartitionInfo[]>("fastboot_list_partitions", { serial });
export const fastbootReadPartition = (serial: string, partition: string) => invoke<string>("fastboot_read_partition", { serial, partition });
export const fastbootBootImage = (serial: string, imagePath: string) => invoke<string>("fastboot_boot_image", { serial, imagePath });
export const fastbootReboot = (serial: string, mode: string) => invoke<string>("fastboot_reboot", { serial, mode });
export const rebootToBootloader = (serial: string) => invoke<string>("reboot_to_bootloader", { serial });

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

export const deviceFullInfo = (serial: string) => invoke<DeviceFullInfo>("device_full_info", { serial });

export const deviceScreenshot = (serial: string) => invoke<ScreenshotInfo>("device_screenshot", { serial });

export const playDeviceRingtone = (serial: string) => invoke<void>("play_ringtone", { serial });
export const startRingtoneLoop = (serial: string) => invoke<void>("start_ringtone_loop", { serial });
export const stopRingtone = (serial: string) => invoke<void>("stop_ringtone", { serial });

export const lockdownDevice = (serial: string) => invoke<void>("lockdown_device", { serial });
export const releaseDevice = (serial: string) => invoke<void>("release_device", { serial });

export const stopCharging = (serial: string) => invoke<string>("stop_charging", { serial });
export const startCharging = (serial: string) => invoke<string>("start_charging", { serial });
export const getBatteryStatus = (serial: string) => invoke<string>("get_battery_status", { serial });

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

export const extractWhatsappDowngrade = (serial: string) =>
  invoke<WhatsAppDeepResult>("extract_whatsapp_downgrade", { serial });

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

// ── Deep Forensics ──

export const collectWifi = (serial: string) => invoke<WifiNetwork[]>("collect_wifi", { serial });

export const collectBluetooth = (serial: string) => invoke<BluetoothDevice[]>("collect_bluetooth", { serial });

export const collectAccounts = (serial: string) => invoke<DeviceAccount[]>("collect_accounts", { serial });

export const collectPowerEvents = (serial: string) => invoke<PowerEvent[]>("collect_power_events", { serial });

export const collectNotifications = (serial: string) => invoke<NotificationEntry[]>("collect_notifications", { serial });

export const collectDictionary = (serial: string) => invoke<DictionaryWord[]>("collect_dictionary", { serial });

// ── Phase 2: Deleted Data Recovery ──

export const recoverDeletedSms = (serial: string) => invoke<DeletedSmsResult>("recover_deleted_sms", { serial });

export const recoverThumbnails = (serial: string) => invoke<ThumbnailRecoveryResult>("recover_thumbnails", { serial });

export const carveDeviceDb = (serial: string, dbPath: string, tables: string[]) => invoke<CarveResult>("carve_device_db", { serial, dbPath, tables });

// ── Phase 3: Browser Forensics ──

export const collectBrowserData = (serial: string) => invoke<BrowserForensicsResult>("collect_browser_data", { serial });

// ── Phase 4: Location & Network ──

export const collectLocationData = (serial: string) => invoke<LocationForensicsResult>("collect_location_data", { serial });

export const collectNetworkArtifacts = (serial: string) => invoke<NetworkForensicsResult>("collect_network_artifacts", { serial });

export const analyzeApks = (serial: string) => invoke<ApkAnalysisResult>("analyze_installed_apks", { serial });

// ── Phase 5: Unified Timeline & Report ──

export const buildUnifiedTimeline = (
  calls: [number, string, string][],
  sms: [number, string, string, boolean][],
  whatsapp: [number, string, string, boolean][],
  power: [number, string, number][],
  notifications: [number, string, string][],
  browser: [number, string, string][],
  location: [number, number, number, string][],
  serial?: string,
) => invoke<UnifiedEvent[]>("build_timeline", {
  calls, sms, whatsapp, power, notifications, browser, location, serial: serial ?? null,
});

export const generateForensicReport = (
  serial: string,
  model: string | null,
  sources: SourceReport[],
  events: UnifiedEvent[],
) => invoke<string>("generate_report", { serial, model, sources, events });

export const captureDevicePhoto = (serial: string) =>
  invoke<CameraCaptureResult>("capture_device_photo", { serial });

export const captureDeviceMic = (serial: string, duration: number = 5) =>
  invoke<MicCaptureResult>("capture_device_mic", { serial, duration });

/** Launch an app on the connected device via ADB monkey. */
export const launchApp = (serial: string, pkg: string) =>
  invoke<string>("launch_app", { serial, package: pkg });

// ── Remote Control ──
export const screenTap = (serial: string, x: number, y: number) =>
  invoke<string>("screen_tap", { serial, x, y });

export const screenSwipe = (serial: string, x1: number, y1: number, x2: number, y2: number, durationMs: number = 300) =>
  invoke<string>("screen_swipe", { serial, x1, y1, x2, y2, durationMs });

export const screenText = (serial: string, text: string) =>
  invoke<string>("screen_text", { serial, text });

export const screenKey = (serial: string, keycode: number) =>
  invoke<string>("screen_key", { serial, keycode });

/** Get the currently running foreground app package on the device. */
export const getForegroundApp = (serial: string) =>
  invoke<string>("get_foreground_app", { serial });

// ── Headless Mode ──
/** Start headless mode: acquire wake lock, turn screen off, keep CPU awake. */
export const headlessStart = (serial: string) =>
  invoke<string>("headless_start", { serial });

/** Stop headless mode: wake screen, release wake lock. */
export const headlessStop = (serial: string) =>
  invoke<string>("headless_stop", { serial });

/** Check if the device screen is currently on or off. */
export const getScreenState = (serial: string) =>
  invoke<string>("get_screen_state", { serial });

/** Rotate the physical device display (0=portrait, 1=90°, 2=180°, 3=270°). */
export const deviceRotate = (serial: string, rotation: number) =>
  invoke<string>("device_rotate", { serial, rotation });

/** Restore auto-rotation (accelerometer controls orientation). */
export const deviceAutoRotate = (serial: string) =>
  invoke<string>("device_auto_rotate", { serial });

/** Freeze device: disable ALL touch input. Phone becomes fully read-only. */
export const deviceFreeze = (serial: string) =>
  invoke<string>("device_freeze", { serial });

/** Unfreeze device: restore normal touch input and navigation. */
export const deviceUnfreeze = (serial: string) =>
  invoke<string>("device_unfreeze", { serial });

/** Change device display density (zoom level). Higher DPI = bigger UI elements. */
export const deviceZoom = (serial: string, density: number) =>
  invoke<string>("device_zoom", { serial, density });

/** Reset device density to factory default. */
export const deviceZoomReset = (serial: string) =>
  invoke<string>("device_zoom_reset", { serial });

/** Get the device's current display density (DPI). */
export const deviceGetDensity = (serial: string) =>
  invoke<string>("device_get_density", { serial });

// ── Device Tools ──
export const getClipboard = (serial: string) =>
  invoke<string>("get_clipboard", { serial });
export const getNotifications = (serial: string) =>
  invoke<string>("get_notifications", { serial });
export const getWifiCredentials = (serial: string) =>
  invoke<string>("get_wifi_credentials", { serial });
export const createDeviceBackup = (serial: string, outputDir: string) =>
  invoke<string>("create_device_backup", { serial, outputDir });
export const exportDataJson = (data: string, filename: string, outputDir: string) =>
  invoke<string>("export_data_json", { data, filename, outputDir });
export const pullAppData = (serial: string, package_: string, outputDir: string) =>
  invoke<string>("pull_app_data", { serial, package: package_, outputDir });
export const searchDatabase = (dbPath: string, query: string) =>
  invoke<string>("search_database", { dbPath: dbPath, query });

// ── Advanced Tools ──
export const listDeviceFiles = (serial: string, path: string) =>
  invoke<string>("list_device_files", { serial, path });
export const getRunningProcesses = (serial: string) =>
  invoke<string>("get_running_processes", { serial });
export const extractApk = (serial: string, package_: string, outputDir: string) =>
  invoke<string>("extract_apk", { serial, package: package_, outputDir });
export const getBatteryStats = (serial: string) =>
  invoke<string>("get_battery_stats", { serial });
export const fullDeviceExtract = (serial: string, outputDir: string) =>
  invoke<string>("full_device_extract", { serial, outputDir });
export const pullDeviceFile = (serial: string, remotePath: string, outputDir: string) =>
  invoke<string>("pull_device_file", { serial, remotePath, outputDir });
export const scanThumbnails = (serial: string, outputDir: string) =>
  invoke<string>("scan_thumbnails", { serial, outputDir });
export const getDeviceTemperatures = (serial: string) =>
  invoke<string>("get_device_temperatures", { serial });

// ── Thermal Stress Test ──
export const runThermalStressTest = (serial: string, durationSecs: number) =>
  invoke<string>("run_thermal_stress_test", { serial, durationSecs });

// ── Device Cooling ──
export const setCpuGovernor = (serial: string, governor: string) =>
  invoke<string>("set_cpu_governor", { serial, governor });
export const killHeavyProcesses = (serial: string) =>
  invoke<string>("kill_heavy_processes", { serial });
export const setScreenBrightness = (serial: string, level: number) =>
  invoke<string>("set_screen_brightness", { serial, level });
export const setAutoSync = (serial: string, enabled: boolean) =>
  invoke<string>("set_auto_sync", { serial, enabled });
export const setBatterySaver = (serial: string, enabled: boolean) =>
  invoke<string>("set_battery_saver", { serial, enabled });
export const applyCoolingMode = (serial: string) =>
  invoke<string>("apply_cooling_mode", { serial });

// ── Forensic Evidence ──
export const hashEvidence = (filePath: string) =>
  invoke<string>("hash_evidence", { filePath });
export const hashDirectory = (dirPath: string) =>
  invoke<string>("hash_directory", { dirPath });
export const startLogcat = (serial: string) =>
  invoke<string>("start_logcat", { serial });
export const getLogcatSnapshot = (serial: string, lines?: number) =>
  invoke<string>("get_logcat_snapshot", { serial, lines });
export const globalKeywordSearch = (dataDir: string, keyword: string) =>
  invoke<string>("global_keyword_search", { dataDir, keyword });
export const getUsageTimeline = (serial: string) =>
  invoke<string>("get_usage_timeline", { serial });
export const generateEvidenceReport = (caseId: string, deviceSerial: string, dataDir: string, outputDir: string) =>
  invoke<string>("generate_forensic_report", { caseId, deviceSerial, dataDir, outputDir });
export const auditAppPermissions = (serial: string) =>
  invoke<string>("audit_app_permissions", { serial });

// ── Security & Forensics ──
export const detectSpyware = (serial: string) =>
  invoke<string>("detect_spyware", { serial });
export const detectLinkedDevices = (serial: string) =>
  invoke<string>("detect_linked_devices", { serial });
export const getBluetoothForensics = (serial: string) =>
  invoke<string>("get_bluetooth_forensics", { serial });
export const getNetworkConnections = (serial: string) =>
  invoke<string>("get_network_connections", { serial });
export const getClipboardHistory = (serial: string, count?: number) =>
  invoke<string>("get_clipboard_history", { serial, count });

// ── Image Forensics ──
/** Run Error Level Analysis on an image to detect manipulation. */
export const imageEla = (imagePath: string, outputDir: string, quality: number = 90, scale: number = 15) =>
  invoke<string>("image_ela", { imagePath, outputDir, quality, scale });

/** Extract comprehensive image metadata (dimensions, format, EXIF, GPS). */
export const imageMetadata = (imagePath: string) =>
  invoke<string>("image_metadata", { imagePath });

// ── Device Identity & Forensics ──
/** Collect device identity: IMEI, ESN, MEID, serial, brand, model. */
export const collectDeviceIdentity = (serial: string) =>
  invoke<string>("collect_device_identity", { serial });

/** Extract lock screen info: type, status, security settings. */
export const extractLockInfo = (serial: string) =>
  invoke<string>("extract_lock_info", { serial });

/** Recover deleted contacts from contacts2.db. */
export const recoverDeletedContacts = (serial: string, outputDir: string) =>
  invoke<string>("recover_deleted_contacts", { serial, outputDir });

/** Collect cell tower connection history. */
export const collectCellTowerHistory = (serial: string) =>
  invoke<string>("collect_cell_tower_history", { serial });

/** Detect anti-forensics: wipers, cache cleaners, tampering evidence. */
export const detectAntiForensics = (serial: string) =>
  invoke<string>("detect_anti_forensics", { serial });

/** Collect dual SIM information for both slots. */
export const collectDualSimInfo = (serial: string) =>
  invoke<string>("collect_dual_sim_info", { serial });

/** Collect timezone fingerprint: detect manual time changes. */
export const collectTimezoneFingerprint = (serial: string) =>
  invoke<string>("collect_timezone_fingerprint", { serial });

/** Analyze device storage: visual breakdown of space usage. */
export const analyzeStorage = (serial: string) =>
  invoke<string>("analyze_storage", { serial });

/** Recover deleted voice memos/recordings from the device. */
export const recoverVoiceMemos = (serial: string, outputDir: string) =>
  invoke<string>("recover_voice_memos", { serial, outputDir });

/** Extract saved passwords metadata from browsers. */
export const extractSavedPasswords = (serial: string) =>
  invoke<string>("extract_saved_passwords", { serial });

// ── GPS / Location Timeline ──
/** Collect GPS/location history from the device. */
export const collectGpsTimeline = (serial: string) =>
  invoke<string>("collect_gps_timeline", { serial });

// ── WhatsApp Chat Viewer ──
/** Extract WhatsApp chat messages from the device. */
export const extractWhatsappChats = (serial: string, outputDir: string) =>
  invoke<string>("extract_whatsapp_chats", { serial, outputDir });

// ── Email Extraction ──
/** Extract email account metadata from Gmail, Outlook, etc. */
export const extractEmailAccounts = (serial: string) =>
  invoke<string>("extract_email_accounts", { serial });

// ── Browser History Viewer ──
/** Extract browser history, bookmarks, and tabs. */
export const extractBrowserHistory = (serial: string) =>
  invoke<string>("extract_browser_history", { serial });

// ── Video Metadata ──
/** Extract metadata from video files on the device. */
export const extractVideoMetadata = (serial: string) =>
  invoke<string>("extract_video_metadata", { serial });

// ── App Timeline Correlation ──
/** Build a correlated timeline of app usage events. */
export const extractAppTimeline = (serial: string) =>
  invoke<string>("extract_app_timeline", { serial });

// ── Cloud Account Extraction ──
/** Extract cloud account info: Google, Samsung, Xiaomi, etc. */
export const extractCloudAccounts = (serial: string) =>
  invoke<string>("extract_cloud_accounts", { serial });

// ── Encrypted Backup Analysis ──
/** Analyze encrypted backups found on the device. */
export const analyzeEncryptedBackups = (serial: string) =>
  invoke<string>("analyze_encrypted_backups", { serial });

// ── Automated PDF Report ──
/** Generate a comprehensive forensic report. */
export const generatePdfReport = (serial: string, outputDir: string, caseId: string) =>
  invoke<string>("generate_pdf_report", { serial, outputDir, caseId });

// ═══════════════════════════════════════════════════════════════════
// ADVANCED FORENSIC FEATURES (#21-#38)
// ═══════════════════════════════════════════════════════════════════

// ── #21 Deleted SMS/MMS Recovery ──
export const recoverDeletedSmsAdvanced = (serial: string, outputDir: string) =>
  invoke<string>("recover_deleted_sms_advanced", { serial, outputDir });

// ── #22 Call Log Forensics ──
export const extractCallLogs = (serial: string) =>
  invoke<string>("extract_call_logs", { serial });

// ── #23 WiFi Connection History ──
export const extractWifiHistory = (serial: string) =>
  invoke<string>("extract_wifi_history", { serial });

// ── #24 Bluetooth Device Forensics ──
export const extractBluetoothDevices = (serial: string) =>
  invoke<string>("extract_bluetooth_devices", { serial });

// ── #25 Social Media Extraction ──
export const extractSocialMedia = (serial: string, outputDir: string) =>
  invoke<string>("extract_social_media", { serial, outputDir });

// ── #26 Deleted Photo Recovery ──
export const recoverDeletedPhotos = (serial: string, outputDir: string) =>
  invoke<string>("recover_deleted_photos", { serial, outputDir });

// ── #27 Keyboard Dictionary Recovery ──
export const extractKeyboardDictionary = (serial: string) =>
  invoke<string>("extract_keyboard_dictionary", { serial });

// ── #28 Clipboard Forensics ──
export const extractClipboardForensics = (serial: string) =>
  invoke<string>("extract_clipboard_forensics", { serial });

// ── #29 USB Connection Forensics ──
export const extractUsbConnections = (serial: string) =>
  invoke<string>("extract_usb_connections", { serial });

// ── #30 Power/Battery Forensics ──
export const extractPowerForensics = (serial: string) =>
  invoke<string>("extract_power_forensics", { serial });

// ── #31 Notification History ──
export const extractNotificationHistory = (serial: string) =>
  invoke<string>("extract_notification_history", { serial });

// ── #32 Screen Time / Digital Wellbeing ──
export const extractScreenTime = (serial: string) =>
  invoke<string>("extract_screen_time", { serial });

// ── #33 SQLite WAL/Journal Carving ──
export const carveSqliteWal = (serial: string, outputDir: string) =>
  invoke<string>("carve_sqlite_wal", { serial, outputDir });

// ── #34 App Sandbox Extraction ──
export const extractAppSandbox = (serial: string) =>
  invoke<string>("extract_app_sandbox", { serial });

// ── #35 Hidden Partition Scanner ──
export const scanHiddenPartitions = (serial: string) =>
  invoke<string>("scan_hidden_partitions", { serial });

// ── #36 Network Traffic Snapshot ──
export const extractNetworkTraffic = (serial: string) =>
  invoke<string>("extract_network_traffic", { serial });

// ── #37 Sensor Data Analysis ──
export const extractSensorData = (serial: string) =>
  invoke<string>("extract_sensor_data", { serial });

// ── #38 Full File System Manifest ──
export const generateFileManifest = (serial: string, outputDir: string) =>
  invoke<string>("generate_file_manifest", { serial, outputDir });

// ═══════════════════════════════════════════════════════════════════
// BUG BOUNTY HUNTER FEATURES (#39-#46)
// ═══════════════════════════════════════════════════════════════════

// ── #39 Permission Auditor Pro ──
export const auditPermissionsPro = (serial: string) =>
  invoke<string>("audit_permissions_pro", { serial });

// ── #40 Intent Fuzzer ──
export const fuzzIntents = (serial: string) =>
  invoke<string>("fuzz_intents", { serial });

// ── #41 SELinux Policy Analyzer ──
export const analyzeSelinux = (serial: string) =>
  invoke<string>("analyze_selinux", { serial });

// ── #42 Content Provider Enumerator ──
export const enumerateContentProviders = (serial: string) =>
  invoke<string>("enumerate_content_providers", { serial });

// ── #43 Privilege Escalation Tester ──
export const testPrivilegeEscalation = (serial: string) =>
  invoke<string>("test_privilege_escalation", { serial });

// ── #44 Secure Element Probe ──
export const probeSecureElement = (serial: string) =>
  invoke<string>("probe_secure_element", { serial });

// ── #45 Bootloader / Verified Boot Checker ──
export const checkVerifiedBoot = (serial: string) =>
  invoke<string>("check_verified_boot", { serial });

// ── #46 Kernel Exploit Detector ──
export const detectKernelExploits = (serial: string) =>
  invoke<string>("detect_kernel_exploits", { serial });

// ── Remote Agent Receiver ──
export const remoteAgentStart = (port?: number, storageDir?: string) =>
  invoke<RemoteAgentState>("remote_agent_start", { port: port ?? null, storageDir: storageDir ?? null });

export const remoteAgentStop = () =>
  invoke<RemoteAgentState>("remote_agent_stop");

export const remoteAgentStatus = (storageDir?: string) =>
  invoke<RemoteAgentState>("remote_agent_status", { storageDir: storageDir ?? null });

export const remoteAgentExport = (packagePath: string, workspaceDir?: string) =>
  invoke<string>("remote_agent_export", { packagePath, workspaceDir: workspaceDir ?? null });
