import { memo, useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../api";
import type { AppInfo, MediaFile, UiRecovery } from "../types";
import { fmtSize } from "../types";

// ── Media grid (memoized for 2000+ tiles) ────────────────────────────

export const MediaTile = memo(function MediaTile({
  f, onOpen, selected, presence,
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

export const MediaGrid = memo(function MediaGrid({
  files, selectedPath, onOpen, lost,
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
          key={f.path} f={f}
          selected={selectedPath === f.path}
          onOpen={onOpen}
          presence={lost ? (lost.has(f.path) ? "lost" : "device") : undefined}
        />
      ))}
    </div>
  );
});

// ── App icon with lazy extraction ────────────────────────────────────

const iconCache = new Map<string, string>();

function iconColor(pkg: string): string {
  let h = 0;
  for (let i = 0; i < pkg.length; i++) h = (h * 31 + pkg.charCodeAt(i)) >>> 0;
  return `hsl(${h % 360} 45% 48%)`;
}

export function AppIcon({ pkg, serial, big }: { pkg: string; serial: string; big?: boolean }) {
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
        api.appIcon(serial, pkg).then((r: { path: string }) => {
          const u = convertFileSrc(r.path);
          iconCache.set(pkg, u);
          setUrl(u);
        }).catch(() => {
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

// ── Forensic relevance tag ───────────────────────────────────────────

export function appTag(pkg: string): string {
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

// ── WhatsApp evidence viewer ─────────────────────────────────────────

export function WaEvidence({ res }: { res: UiRecovery }) {
  const [tab, setTab] = useState<"contacts" | "chats">("chats");
  const nameOf = (jid: string) =>
    res.names.find(([j]) => j === jid)?.[1] || res.names.find(([j]) => j === jid.split("@")[0])?.[1] || jid.split("@")[0];
  const topChats = res.analysis.top_chats;
  return (
    <>
      <div className="grid-3">
        <div className="stat"><b>{res.chats}</b><span>chats</span></div>
        <div className="stat"><b>{res.contacts}</b><span>contacts</span></div>
        <div className="stat"><b>{res.total_messages.toLocaleString()}</b><span>messages</span></div>
      </div>
      {res.recovered_deleted > 0 && (
        <p className="hint lock-warn" style={{ marginTop: 6 }}>
          ⚠ {res.recovered_deleted} deleted message{res.recovered_deleted === 1 ? "" : "s"} carved from the database.
        </p>
      )}
      <div className="legend" style={{ marginTop: 8 }}>
        <span className={`lg-tab${tab === "chats" ? " active" : ""}`} onClick={() => setTab("chats")}>Chats</span>
        <span className={`lg-tab${tab === "contacts" ? " active" : ""}`} onClick={() => setTab("contacts")}>Contacts</span>
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
                <span className="pill kind-media" key={jid}>{name || jid.split("@")[0]} · {n}</span>
              ))}
            </div>
          )}
          <div className="wa-list">
            {res.messages.slice(0, 600).map((m) => (
              <div className="wa-item" key={m.row_id ?? `${m.chat_jid}-${m.timestamp}`}>
                <div className="wa-item-head">
                  <b>{m.from_me ? "→ " : ""}{m.chat_jid ? nameOf(m.chat_jid) : m.sender ?? "unknown"}</b>
                  <span className="wa-ts">{m.timestamp ? new Date(m.timestamp).toLocaleString() : "?"}</span>
                </div>
                <span className="wa-body">{m.text || (m.source !== "live" ? `(carved · ${m.source})` : "")}</span>
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

// ── Virtualized app inventory table ──────────────────────────────────

const ROW_H = 26;

export function AppTable({
  apps, serial, sel, onOpen, runningPkg,
}: {
  apps: AppInfo[];
  serial: string;
  sel: string;
  onOpen: (a: AppInfo) => void;
  runningPkg?: string | null;
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
            <th></th><th>Package</th><th>Version</th><th>Installed</th><th>Updated</th><th>Type</th><th>Evidence</th>
          </tr>
        </thead>
        {start > 0 && (
          <tbody><tr style={{ height: start * ROW_H }}><td colSpan={7} /></tr></tbody>
        )}
        <tbody>
          {apps.slice(start, end).map((a) => {
            const isRunning = runningPkg && a.package === runningPkg;
            const rowClass = sel === a.package ? "sel" : "";
            return (
              <tr
                key={a.package}
                style={{
                  height: ROW_H,
                  background: isRunning ? "rgba(34,197,94,0.15)" : undefined,
                  borderLeft: isRunning ? "3px solid var(--ok, #22c55e)" : undefined,
                }}
                className={rowClass}
                onClick={() => onOpen(a)}
              >
                <td>
                  {isRunning && <span style={{ color: "var(--ok, #22c55e)", fontSize: 8, marginRight: 2 }}>●</span>}
                  <AppIcon pkg={a.package} serial={serial} />
                </td>
              <td className="mono small">{a.package}</td>
              <td className="small nowrap">{a.version_name ?? "…"}</td>
              <td className="small nowrap">{a.first_install || "…"}</td>
              <td className="small nowrap">{a.last_update || "…"}</td>
              <td className="small">{a.system ? "system" : "user"}</td>
              <td>{appTag(a.package) && <span className="pill kind-media">{appTag(a.package)}</span>}</td>
            </tr>
            );
          })}
        </tbody>
        {end < apps.length && (
          <tbody><tr style={{ height: (apps.length - end) * ROW_H }}><td colSpan={7} /></tr></tbody>
        )}
      </table>
    </div>
  );
}
