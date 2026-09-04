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
}

/** Browser history entry from Android content provider. */
export interface BrowserHistory {
  id: number;
  url: string;
  title: string | null;
  timestamp: number;
  source: string;
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
  browser_history: BrowserHistory[];
  calendar_events: CalendarEvent[];
  notes: string[];
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
