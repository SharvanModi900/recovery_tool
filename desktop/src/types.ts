// TS mirrors of recovery_core's serde types.

export interface Device {
  serial: string;
  state: string;
  model: string | null;
  product: string | null;
  android_version: string | null;
}

export type ArtifactKind = "database" | "backup" | "media" | "other";

export interface Artifact {
  path: string;
  size: number;
  kind: ArtifactKind;
}

export interface ScanReport {
  source: string;
  whatsapp_found: boolean;
  artifacts: Artifact[];
  notes: string[];
}

export interface DecryptOutcome {
  output: string;
  format: string;
  key_offset: number;
  iv_offset: number;
  plaintext_size: number;
}

export interface MediaFile {
  path: string;
  size: number;
  kind: "image" | "video";
}

export interface MediaRecovery {
  files: MediaFile[];
  pulled_dirs: number;
  failed_dirs: number;
}

/** Forensic inspection of a single recovered file. */
export interface MediaInspect {
  size: number;
  sha256: string;
  format: string;
  valid: boolean;
}

/** Result of a screen snapshot: local PNG path + black/locked-frame flag. */
export interface ScreenshotInfo {
  path: string;
  dark: boolean;
}

/** One installed app + its configuration (read-only pm/dumpsys data). */
export interface AppInfo {
  package: string;
  apk_path: string;
  system: boolean;
  version_name: string | null;
  version_code: string | null;
  target_sdk: string | null;
  first_install: string;
  last_update: string;
  data_dir: string | null;
  signer: string | null;
  perms: number;
}

/** Local path of an extracted launcher icon. */
export interface IconInfo {
  path: string;
}

/** Live device telemetry shown in the mirror panel. */
export interface DeviceMirror {
  model: string;
  android: string;
  battery: string;
  storage: string;
  resolution: string;
  uptime: string;
  whatsapp: string;
}

export type Source = "live" | "wal" | "freelist";

export interface Message {
  row_id: number | null;
  chat_jid: string | null;
  from_me: boolean;
  sender: string | null;
  text: string | null;
  timestamp: number | null;
  source: Source;
}

export interface Analysis {
  messages: number;
  deleted: number;
  chats_count: number;
  first_ts: number | null;
  last_ts: number | null;
  from_me_ratio: number;
  top_chats: [string, string, number][];
  per_day: [string, number][];
}

export interface UiRecovery {
  db_path: string;
  schema: string;
  sha256: string;
  total_messages: number;
  recovered_deleted: number;
  truncated: boolean;
  chats: number;
  contacts: number;
  media: number;
  messages: Message[];
  messages_hidden: number;
  /** jid → display name pairs (identity resolution). */
  names: [string, string][];
  /** Contact book preview: (name, number) pairs. */
  contacts_list: [string, string][];
  analysis: Analysis;
}

/** Call log entry from Android content provider. */
export interface CallLog {
  id: number;
  number: string;
  name: string | null;
  call_type: string;
  duration: number;
  timestamp: number;
  source: string;
}

/** SMS/MMS message from Android content provider. */
export interface SmsMessage {
  id: number;
  address: string;
  body: string | null;
  msg_type: string;
  timestamp: number;
  read: boolean;
  source: string;
}

/** Contact entry from Android content provider. */
export interface ContactEntry {
  id: number;
  display_name: string | null;
  phone: string | null;
  email: string | null;
  source: string;
}

/** Call recording file from device storage. */
export interface CallRecording {
  path: string;
  filename: string;
  size: number;
  timestamp: number;
  source: string;
  matched_number: string | null;
  matched_name: string | null;
  matched_call_type: string | null;
  matched_timestamp: number | null;
}

/** WhatsApp media file from device storage. */
export interface WhatsAppMedia {
  path: string;
  filename: string;
  size: number;
  media_type: string;
  source_folder: string;
  contact?: string;
  source: string;
}

/** WhatsApp chat entry. */
export interface WhatsAppChat {
  jid: string;
  name: string | null;
  message_count: number;
  last_message: string | null;
  last_timestamp: number;
  source: string;
}

/** WhatsApp call record. */
export interface WhatsAppCall {
  jid: string;
  name: string | null;
  call_type: string;
  duration: number;
  timestamp: number;
  source: string;
}

/** WhatsApp deep extraction result - automatic key + decrypt + parse. */
export interface WhatsAppDeepResult {
  key_obtained: boolean;
  key_method: string | null;
  database_decrypted: boolean;
  decrypted_db_path: string | null;
  crypt_file_path: string | null;
  crypt_format: string | null;
  chats: WhatsAppChat[];
  media_with_contacts: WhatsAppMedia[];
  total_messages: number;
  total_contacts: number;
  notes: string[];
}

/** Root access check result. */
export interface RootCheckResult {
  has_root: boolean;
  has_magisk: boolean;
  root_method: string | null;
  can_read_whatsapp_key: boolean;
  android_sdk: number | null;
  details: string[];
}

/** Call event from device monitoring. */
export interface CallEvent {
  timestamp: number;
  event_type: "Incoming" | "Outgoing" | "Missed" | "Ended" | "Rejected" | "Active" | "Idle" | "Unknown";
  phone_number: string | null;
  contact_name: string | null;
  duration: number | null;
  sim_slot: number | null;
}

/** Screen capture entry from local DB. */
export interface ScreenCapture {
  id: number;
  timestamp: number;
  file_path: string;
  device_serial: string;
  device_model: string | null;
  note: string | null;
  file_size: number;
  dark_frame: boolean;
}

/** SIM / network identity snapshot (read-only device forensics). */
export interface SimSnapshot {
  id: number;
  timestamp: number;
  device_serial: string;
  iccid: string | null;
  msisdn: string | null;
  imsi: string | null;
  carrier_name: string | null;
  operator_numeric: string | null;
  mcc: string | null;
  mnc: string | null;
  country_iso: string | null;
  sim_state: string | null;
  slot_index: number | null;
  network_operator: string | null;
  network_type: string | null;
  roaming: string | null;
  baseband: string | null;
  imei: string | null;
  is_baseline: boolean;
  fingerprint: string;
}

/** A SIM record seen by the device (active or historical, from dumpsys isub / siminfo). */
export interface HistoricalSim {
  iccid: string | null;
  carrier: string | null;
  msisdn: string | null;
  slot: number | null;
  mcc: string | null;
  mnc: string | null;
  active: boolean;
}

/** SIM-swap detection verdict. */
export interface SimSwapVerdict {
  risk: "NONE" | "LOW" | "HIGH";
  changed: boolean;
  reasons: string[];
}

/** Full SIM / Network panel payload. */
export interface SimSwapReport {
  snapshot: SimSnapshot;
  baseline: SimSnapshot | null;
  historical: HistoricalSim[];
  verdict: SimSwapVerdict;
}

/** Calendar event from Android content provider. */
export interface CalendarEvent {
  id: number;
  title: string | null;
  description: string | null;
  location: string | null;
  start_time: number;
  end_time: number;
  source: string;
}

/** Direct acquisition result from Android content providers. */
export interface DirectAcquisition {
  call_logs: CallLog[];
  call_recordings: CallRecording[];
  sms_messages: SmsMessage[];
  contacts: ContactEntry[];
  whatsapp_media: WhatsAppMedia[];
  whatsapp_chats: WhatsAppChat[];
  whatsapp_calls: WhatsAppCall[];
  calendar_events: CalendarEvent[];
  notes: string[];
}

/** Multi-app detection and extraction types. */

export interface AppDetection {
  id: string;
  name: string;
  icon: string;
  package: string;
  is_installed: boolean;
  is_accessible: boolean;
  requires_root: boolean;
  accessible_paths_found: string[];
  root_paths_found: string[];
  databases_found: string[];
  media_count: number;
  has_data: boolean;
}

export interface AppInventory {
  apps: AppDetection[];
  total_installed: number;
  total_with_data: number;
  device_has_root: boolean;
}

/** Instagram extraction types. */

export interface InstagramMedia {
  path: string;
  filename: string;
  size: number;
  media_type: string;
  source: string;
}

export interface InstagramData {
  media: InstagramMedia[];
  has_database: boolean;
  database_accessible: boolean;
}

/** Messenger extraction types. */

export interface MessengerMedia {
  path: string;
  filename: string;
  size: number;
  media_type: string;
  source: string;
}

export interface MessengerData {
  media: MessengerMedia[];
  has_database: boolean;
  database_accessible: boolean;
}

/** Telegram extraction types. */

export interface TelegramMedia {
  path: string;
  filename: string;
  size: number;
  media_type: string;
  source: string;
}

export interface TelegramData {
  media: TelegramMedia[];
  has_database: boolean;
  database_accessible: boolean;
}

/** Forensic extraction types. */

export interface ExtractedMedia {
  path: string;
  filename: string;
  size: number;
  media_type: string;
  source: string;
  local_path?: string;
}

export interface CarvedMessage {
  row_id: number | null;
  chat_jid: string | null;
  from_me: boolean;
  sender: string | null;
  text: string | null;
  timestamp: number | null;
  source: string;
}

export interface ForensicExtraction {
  app_id: string;
  app_name: string;
  media_files: ExtractedMedia[];
  carved_messages: CarvedMessage[];
  databases_found: string[];
  databases_pulled: string[];
  extraction_notes: string[];
}

/** A forensic case grouping evidence from one or more devices. */
export interface Case {
  id: number;
  case_number: string;
  title: string;
  examiner: string;
  subject: string;
  device_serial: string | null;
  device_model: string | null;
  created_at: number;
  closed_at: number | null;
  notes: string | null;
}

/** One piece of evidence bundled into a case. */
export interface EvidenceItem {
  id: number;
  case_id: number;
  timestamp: number;
  category: string;
  source: string;
  description: string;
  item_count: number;
  size_bytes: number;
  sha256: string;
  local_path: string | null;
}

/** A chain-of-custody audit log entry (tamper-evident hash chain). */
export interface AuditEntry {
  id: number;
  case_id: number;
  timestamp: number;
  action: string;
  detail: string;
  actor: string;
  hash_chain: string;
}

/** Per-category roll-up for the case dashboard. */
export interface CategoryCount {
  category: string;
  evidence_items: number;
  records: number;
}

/** Case dashboard summary payload. */
export interface CaseSummary {
  case_info: Case;
  evidence_count: number;
  total_records: number;
  audit_count: number;
  by_category: CategoryCount[];
  earliest: number | null;
  latest: number | null;
  chain_valid: boolean;
}

/** Chain-of-custody verification result. */
export interface ChainStatus {
  valid: boolean;
  entries: number;
  broken_at: number | null;
  message: string;
}

// ── Unified cross-source timeline ────────────────────────────────────────
export interface TimelineEvent {
  id: number;
  case_id: number;
  timestamp: number; // event time, Unix ms
  kind: string; // call | sms | calendar | whatsapp_chat | whatsapp_call | whatsapp_message | recording | capture | sim
  title: string;
  detail: string;
  source: string;
  local_path: string | null;
}

export interface KindCount {
  kind: string;
  count: number;
}

export interface TimelinePage {
  events: TimelineEvent[];
  total: number;
  kinds: KindCount[];
  returned: number;
}

export function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

export function fmtTime(ms: number | null): string {
  if (ms == null) return "—";
  const d = new Date(ms);
  if (isNaN(d.getTime())) return String(ms);
  return d.toLocaleString();
}
