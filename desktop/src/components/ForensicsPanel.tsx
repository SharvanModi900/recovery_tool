import { useState, useCallback } from "react";
import * as api from "../api";

interface ForensicsPanelProps {
  serial: string | null;
  onClose: () => void;
}

const OUT = "D:\\recovery-tool\\workspace";

type ForensicTab =
  | "identity" | "lock" | "contacts" | "celltower" | "antiforensics"
  | "dualsim" | "timezone" | "storage" | "voice" | "passwords"
  | "gps" | "whatsapp" | "email" | "browser" | "video"
  | "apptimeline" | "cloud" | "backups" | "report"
  | "sms" | "calllogs" | "wifi" | "bluetooth" | "social"
  | "photos" | "keyboard" | "clipboard" | "usb" | "power"
  | "notifications" | "screentime" | "walcarve" | "sandbox"
  | "partitions" | "nettraffic" | "sensors" | "manifest" | "thermal"
  | "perms" | "intents" | "selinux" | "providers" | "privesc"
  | "secelement" | "bootcheck" | "kernel";

interface TabDef {
  id: ForensicTab;
  icon: string;
  label: string;
}

const TABS: TabDef[] = [
  { id: "identity", icon: "🔑", label: "Identity" },
  { id: "lock", icon: "🔒", label: "Lock" },
  { id: "contacts", icon: "👤", label: "Contacts" },
  { id: "celltower", icon: "📡", label: "Cell Tower" },
  { id: "antiforensics", icon: "🕵", label: "Anti-Forensics" },
  { id: "dualsim", icon: "📲", label: "Dual SIM" },
  { id: "timezone", icon: "🕐", label: "Timezone" },
  { id: "storage", icon: "💾", label: "Storage" },
  { id: "voice", icon: "🎙", label: "Voice Memos" },
  { id: "passwords", icon: "🔐", label: "Passwords" },
  { id: "gps", icon: "📍", label: "GPS Timeline" },
  { id: "whatsapp", icon: "💬", label: "WhatsApp" },
  { id: "email", icon: "📧", label: "Email" },
  { id: "browser", icon: "🌐", label: "Browser" },
  { id: "video", icon: "🎬", label: "Video Meta" },
  { id: "apptimeline", icon: "📊", label: "App Timeline" },
  { id: "cloud", icon: "☁", label: "Cloud Accts" },
  { id: "backups", icon: "🔐", label: "Backups" },
  { id: "report", icon: "📄", label: "Report" },
  { id: "sms", icon: "💌", label: "SMS" },
  { id: "calllogs", icon: "📞", label: "Call Logs" },
  { id: "wifi", icon: "📶", label: "WiFi" },
  { id: "bluetooth", icon: "🔵", label: "Bluetooth" },
  { id: "social", icon: "📱", label: "Social Media" },
  { id: "photos", icon: "📸", label: "Photos" },
  { id: "keyboard", icon: "⌨", label: "Keyboard" },
  { id: "clipboard", icon: "📋", label: "Clipboard" },
  { id: "usb", icon: "🔌", label: "USB" },
  { id: "power", icon: "🔋", label: "Power" },
  { id: "notifications", icon: "🔔", label: "Notifs" },
  { id: "screentime", icon: "⏱", label: "Screen Time" },
  { id: "walcarve", icon: "🗃", label: "WAL Carve" },
  { id: "sandbox", icon: "📦", label: "Sandbox" },
  { id: "partitions", icon: "💿", label: "Partitions" },
  { id: "nettraffic", icon: "🌍", label: "Net Traffic" },
  { id: "sensors", icon: "📐", label: "Sensors" },
  { id: "manifest", icon: "📑", label: "Manifest" },
  { id: "thermal", icon: "🌡", label: "Thermal" },
  { id: "perms", icon: "🛡", label: "Permissions" },
  { id: "intents", icon: "💣", label: "Intents" },
  { id: "selinux", icon: "🔰", label: "SELinux" },
  { id: "providers", icon: "🔍", label: "Providers" },
  { id: "privesc", icon: "⚡", label: "Priv Esc" },
  { id: "secelement", icon: "🔐", label: "Secure Elem" },
  { id: "bootcheck", icon: "🔗", label: "Boot Check" },
  { id: "kernel", icon: "🧠", label: "Kernel" },
];

export function ForensicsPanel({ serial, onClose }: ForensicsPanelProps) {
  const [tab, setTab] = useState<ForensicTab>("identity");
  const [results, setResults] = useState<Partial<Record<ForensicTab, string>>>({});
  const [busy, setBusy] = useState<Set<ForensicTab>>(new Set());
  const [errors, setErrors] = useState<Partial<Record<ForensicTab, string>>>({});
  const [statusMsg, setStatusMsg] = useState<string>("");

  const run = useCallback(async (id: ForensicTab, fn: () => Promise<string>) => {
    if (!serial) {
      setStatusMsg("No device connected — please connect a device first");
      setErrors(prev => ({ ...prev, [id]: "No device connected. Serial is empty." }));
      return;
    }
    setBusy(prev => new Set(prev).add(id));
    setErrors(prev => ({ ...prev, [id]: undefined }));
    setStatusMsg(`Running ${id}...`);
    console.log(`[ForensicsPanel] Running ${id} with serial=${serial}`);
    try {
      // Longer timeout for operations that do stress tests, file scanning, or DB pulls
      const longOps: ForensicTab[] = ["thermal", "manifest", "walcarve", "social", "sandbox", "photos", "sms"];
      const timeoutMs = longOps.includes(id) ? 180000 : 60000;
      const timeoutPromise = new Promise<string>((_, reject) =>
        setTimeout(() => reject(new Error(`Command timed out after ${timeoutMs / 1000} seconds`)), timeoutMs)
      );
      const result = await Promise.race([fn(), timeoutPromise]);
      console.log(`[ForensicsPanel] ${id} completed, result length=${result.length}`);
      setResults(prev => ({ ...prev, [id]: result }));
      setStatusMsg(`${id} completed successfully`);
    } catch (e) {
      const errMsg = typeof e === "string" ? e : (e as Error)?.message ?? String(e);
      console.error(`[ForensicsPanel] ${id} failed:`, errMsg);
      setErrors(prev => ({ ...prev, [id]: errMsg }));
      setStatusMsg(`${id} failed`);
    } finally {
      setBusy(prev => { const n = new Set(prev); n.delete(id); return n; });
    }
  }, [serial]);

  const isBusy = (id: ForensicTab) => busy.has(id);
  const result = (id: ForensicTab) => results[id];
  const error = (id: ForensicTab) => errors[id];

  const btn: React.CSSProperties = {
    padding: "4px 12px", fontSize: 11, borderRadius: 6,
    border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)",
    color: "var(--text, #eee)", cursor: "pointer",
  };

  const tabBtn = (active: boolean): React.CSSProperties => ({
    padding: "4px 10px", fontSize: 10.5, borderRadius: 6,
    border: active ? "1px solid #3b82f6" : "1px solid var(--border, #444)",
    background: active ? "rgba(59,130,246,0.15)" : "var(--panel-alt, #1a1a2e)",
    color: active ? "#93c5fd" : "var(--text, #eee)",
    cursor: "pointer", fontWeight: active ? 600 : 400,
    whiteSpace: "nowrap",
  });

  const renderResult = (id: ForensicTab) => {
    if (isBusy(id)) return (
      <div style={{ textAlign: "center", padding: 40, color: "#888" }}>
        <div style={{ fontSize: 32, marginBottom: 8 }}>⏳</div>
        Analyzing... Please wait
      </div>
    );
    if (error(id)) return (
      <div style={{ padding: 16, color: "#f87171", background: "rgba(239,68,68,0.08)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.2)" }}>
        <div style={{ fontWeight: 600, marginBottom: 4 }}>Error</div>
        <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 11, whiteSpace: "pre-wrap" }}>{error(id)}</pre>
      </div>
    );
    if (result(id)) return (
      <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 11, color: "#ccc", whiteSpace: "pre-wrap", lineHeight: 1.6, background: "rgba(0,0,0,0.2)", padding: 12, borderRadius: 8, border: "1px solid #222", maxHeight: 400, overflow: "auto" }}>
        {result(id)}
      </pre>
    );
    return (
      <div style={{ textAlign: "center", padding: 40, color: "#555" }}>
        Click the button above to start
      </div>
    );
  };

  const renderContent = () => {
    switch (tab) {
      case "identity":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.collectDeviceIdentity(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #3b82f6, #8b5cf6)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Collecting…" : "🔑 Collect Device Identity"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>IMEI, MEID/ESN, Serial, Brand, Model</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "lock":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractLockInfo(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #f59e0b, #ef4444)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔒 Extract Lock Info"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Lock type, encryption status, security patch level</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "contacts":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.recoverDeletedContacts(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #22c55e, #06b6d4)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Recovering…" : "👤 Recover Deleted Contacts"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Scans contacts2.db for deleted records</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "celltower":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.collectCellTowerHistory(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #06b6d4, #3b82f6)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Collecting…" : "📡 Collect Cell Tower History"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Cell identity, SIM operator, network type, signal</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "antiforensics":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.detectAntiForensics(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ef4444, #dc2626)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Scanning…" : "🕵 Detect Anti-Forensics"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Wiper apps, cache cleaners, factory reset indicators</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "dualsim":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.collectDualSimInfo(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #8b5cf6, #ec4899)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Collecting…" : "📲 Collect Dual SIM Info"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Per-slot state, operator, MCC/MNC, serial</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "timezone":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.collectTimezoneFingerprint(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #14b8a6, #22c55e)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Collecting…" : "🕐 Collect Timezone Fingerprint"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Detect manual time changes, NTP sync status</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "storage":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.analyzeStorage(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #f97316, #eab308)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Analyzing…" : "💾 Analyze Storage"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Partition usage, top consumers, disk stats</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "voice":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.recoverVoiceMemos(serial!, OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ec4899, #f43f5e)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Recovering…" : "🎙 Recover Voice Memos"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Scans recording folders and audio files</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "passwords":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractSavedPasswords(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #6366f1, #8b5cf6)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔐 Extract Saved Passwords"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Chrome, Samsung Internet, autofill services</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "gps":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.collectGpsTimeline(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #10b981, #059669)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Collecting…" : "📍 Collect GPS Timeline"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Location history, providers, GPS status</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "whatsapp":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractWhatsappChats(serial!, OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #25d366, #128c7e)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "💬 Extract WhatsApp Chats"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Pull msgstore.db and parse messages</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "email":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractEmailAccounts(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ea4335, #4285f4)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📧 Extract Email Accounts"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Gmail, Outlook, registered accounts</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "browser":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractBrowserHistory(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #4285f4, #34a853)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🌐 Extract Browser History"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Chrome history, bookmarks, installed browsers</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "video":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractVideoMetadata(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ff6b6b, #ee5a24)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🎬 Extract Video Metadata"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Video files, resolution, duration, storage</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "apptimeline":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractAppTimeline(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #a29bfe, #6c5ce7)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Building…" : "📊 Build App Timeline"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Usage stats, activity events, install history</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "cloud":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractCloudAccounts(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #00cec9, #0984e3)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "☁ Extract Cloud Accounts"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Google, Samsung, Xiaomi, sync status</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "backups":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.analyzeEncryptedBackups(serial!))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #fd79a8, #e84393)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Analyzing…" : "🔐 Analyze Encrypted Backups"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>WhatsApp, Signal, Telegram, Android backups</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "report":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => {
                const caseId = prompt("Enter Case ID:") || `CASE-${Date.now()}`;
                run(tab, () => api.generatePdfReport(serial!, OUT, caseId));
              }}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #fdcb6e, #e17055)", color: "#fff", fontWeight: 600 }}>
                {isBusy(tab) ? "⏳ Generating…" : "📄 Generate Forensic Report"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Comprehensive text report with all findings</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      // ── Advanced Features (#21-#38) ──
      case "sms":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.recoverDeletedSmsAdvanced(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #e74c3c, #c0392b)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Recovering…" : "💌 Recover Deleted SMS"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Parse mmssms.db for deleted messages</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "calllogs":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractCallLogs(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #2ecc71, #27ae60)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📞 Extract Call Logs"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Call history with deleted detection, type counts</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "wifi":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractWifiHistory(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #3498db, #2980b9)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📶 Extract WiFi History"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Saved networks, SSIDs, credentials, current connection</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "bluetooth":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractBluetoothDevices(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #2980b9, #8e44ad)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔵 Extract Bluetooth Devices"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Paired devices, adapter info, connection history</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "social":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractSocialMedia(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #e1306c, #fd1d1d)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📱 Extract Social Media"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Instagram, Telegram, Signal, Snapchat, Messenger</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "photos":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.recoverDeletedPhotos(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #f39c12, #e67e22)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Recovering…" : "📸 Recover Deleted Photos"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Thumbnails, DCIM, recently deleted folders</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "keyboard":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractKeyboardDictionary(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #1abc9c, #16a085)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "⌨ Extract Keyboard Dictionary"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>User dictionary, Gboard personal words</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "clipboard":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractClipboardForensics(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #9b59b6, #8e44ad)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📋 Extract Clipboard Data"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Current clipboard, Clipper app, service call decode</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "usb":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractUsbConnections(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #34495e, #2c3e50)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔌 Extract USB Connections"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>USB device manager, config, OTG host devices</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "power":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractPowerForensics(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #27ae60, #2ecc71)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔋 Extract Power Forensics"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Battery stats, wake locks, charging, screen events</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "notifications":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractNotificationHistory(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #e67e22, #d35400)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🔔 Extract Notifications"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Active notifications, DND mode, listener apps</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "screentime":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractScreenTime(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #00cec9, #0984e3)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "⏱ Extract Screen Time"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>App foreground time, total screen on, unlock events</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "walcarve":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.carveSqliteWal(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #6c5ce7, #a29bfe)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Carving…" : "🗃 Carve SQLite WAL/Journal"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Find WAL and journal files for deleted data recovery</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "sandbox":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractAppSandbox(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #fd79a8, #e84393)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📦 Extract App Sandbox"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Debuggable apps, run-as SharedPreferences</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "partitions":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.scanHiddenPartitions(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #636e72, #2d3436)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Scanning…" : "💿 Scan Hidden Partitions"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Block devices, mount points, multi-user, adoptable</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "nettraffic":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractNetworkTraffic(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #00b894, #55efc4)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "🌍 Extract Network Traffic"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Per-app data usage, TCP/UDP stats, data saver</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "sensors":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.extractSensorData(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #fab1a0, #e17055)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Extracting…" : "📐 Extract Sensor Data"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Available sensors, step counter, active listeners</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "manifest":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.generateFileManifest(serial || "NONE", OUT))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #74b9ff, #0984e3)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Generating…" : "📑 Generate File Manifest"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Full listing of DCIM, Download, Documents, WhatsApp</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "thermal":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
              <button disabled={isBusy(tab)} onClick={() => {
                const durStr = prompt("Stress test duration in seconds (5-120, default 30):", "30");
                const dur = durStr ? parseInt(durStr, 10) : 30;
                const duration = isNaN(dur) ? 30 : Math.max(5, Math.min(120, dur));
                run(tab, () => api.runThermalStressTest(serial || "NONE", duration));
              }}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ff6b6b, #ee5a24)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Stress Testing…" : "🌡 Run Thermal Stress Test"}
              </button>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.getDeviceTemperatures(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #ffa502, #ff6348)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Reading…" : "🌡 Read Temperature"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>CPU stress + temp monitoring, or quick temp read</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      // ── Bug Bounty Hunter Features (#39-#46) ──
      case "perms":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.auditPermissionsPro(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #e74c3c, #c0392b)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Auditing…" : "🛡 Audit Permissions"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Find overprivileged apps, signature-level violations</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "intents":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.fuzzIntents(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #f39c12, #d35400)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Fuzzing…" : "💣 Fuzz Intents / Probe Components"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Probe exported components, detect crashes</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "selinux":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.analyzeSelinux(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #27ae60, #2ecc71)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Analyzing…" : "🔰 Analyze SELinux Policy"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Enforcement mode, permissive domains, booleans</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "providers":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.enumerateContentProviders(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #8e44ad, #9b59b6)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Enumerating…" : "🔍 Enumerate Content Providers"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Probe content:// URIs for data leaks</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "privesc":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.testPrivilegeEscalation(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #e67e22, #f1c40f)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Testing…" : "⚡ Test Privilege Escalation"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Shell UID access to restricted APIs</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "secelement":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.probeSecureElement(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #1abc9c, #16a085)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Probing…" : "🔐 Probe Secure Element"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>NFC, keystore, TEE, StrongBox isolation</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "bootcheck":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.checkVerifiedBoot(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #34495e, #2c3e50)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Checking…" : "🔗 Check Verified Boot"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Bootloader lock, dm-verity, AVB, encryption</span>
            </div>
            {renderResult(tab)}
          </div>
        );
      case "kernel":
        return (
          <div>
            <div style={{ marginBottom: 12, display: "flex", gap: 8, alignItems: "center" }}>
              <button disabled={isBusy(tab)} onClick={() => run(tab, () => api.detectKernelExploits(serial || "NONE"))}
                style={{ ...btn, background: isBusy(tab) ? "#555" : "linear-gradient(135deg, #c0392b, #e74c3c)", color: "#fff", fontWeight: 600, opacity: !serial ? 0.6 : 1 }}>
                {isBusy(tab) ? "⏳ Detecting…" : "🧠 Detect Kernel Exploits"}
              </button>
              <span style={{ fontSize: 10, color: "#888" }}>Kernel version, /proc leaks, debug interfaces, config</span>
            </div>
            {renderResult(tab)}
          </div>
        );
    }
  };

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 999, background: "rgba(0,0,0,0.7)", display: "flex", alignItems: "center", justifyContent: "center" }} onClick={onClose}>
      <div onClick={e => e.stopPropagation()} style={{ background: "var(--bg, #0f0f1a)", border: "1px solid var(--border, #333)", borderRadius: 12, width: 860, maxHeight: "90vh", display: "flex", flexDirection: "column", boxShadow: "0 8px 40px rgba(0,0,0,0.5)" }}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "10px 16px", borderBottom: "1px solid var(--border, #333)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
            <span style={{ fontSize: 14, fontWeight: 700, color: "var(--text, #eee)" }}>🔬 Forensic Analysis Suite</span>
            <span style={{ fontSize: 10, padding: "2px 8px", borderRadius: 4, background: serial ? "rgba(34,197,94,0.15)" : "rgba(239,68,68,0.15)", color: serial ? "#4ade80" : "#f87171", border: `1px solid ${serial ? "rgba(34,197,94,0.3)" : "rgba(239,68,68,0.3)"}` }}>
              {serial ? `📱 ${serial}` : "⚠ No Device"}
            </span>
          </div>
          <button onClick={onClose} style={{ background: "none", border: "none", color: "var(--text-secondary, #aaa)", cursor: "pointer", fontSize: 16 }}>✕</button>
        </div>

        {/* Status bar */}
        {statusMsg && (
          <div style={{ padding: "4px 16px", fontSize: 10, color: statusMsg.includes("failed") ? "#f87171" : statusMsg.includes("completed") ? "#4ade80" : "#60a5fa", background: "rgba(0,0,0,0.3)", borderBottom: "1px solid var(--border, #222)" }}>
            {statusMsg}
          </div>
        )}

        {/* Tab bar */}
        <div style={{ display: "flex", gap: 3, padding: "6px 12px", flexWrap: "wrap", borderBottom: "1px solid var(--border, #222)" }}>
          {TABS.map(t => (
            <button key={t.id} onClick={() => setTab(t.id)} style={tabBtn(tab === t.id)}>
              {t.icon} {t.label}
            </button>
          ))}
        </div>

        {/* Content */}
        <div style={{ flex: 1, overflow: "auto", padding: "12px 16px", minHeight: 300 }}>
          {!serial && (
            <div style={{ padding: 20, marginBottom: 12, background: "rgba(239,68,68,0.08)", borderRadius: 8, border: "1px solid rgba(239,68,68,0.2)", color: "#f87171", fontSize: 12, textAlign: "center" }}>
              ⚠ No device connected. Please connect an Android device via ADB before using forensic tools.
            </div>
          )}
          {renderContent()}
        </div>

        {/* Footer */}
        <div style={{ padding: "6px 16px", borderTop: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #444)" }}>Output: {OUT}</span>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #444)" }}>
            {TABS.findIndex(t => result(t.id)) + 1}/{TABS.length} analyses complete
          </span>
        </div>
      </div>
    </div>
  );
}
