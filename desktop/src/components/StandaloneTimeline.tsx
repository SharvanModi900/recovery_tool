import { useState, useCallback } from "react";
import * as api from "../api";
import type { UnifiedEvent, DirectAcquisition } from "../types";

interface StandaloneTimelineProps {
  serial: string;
}

type SourceFilter = "all" | "call" | "sms" | "whatsapp" | "power" | "notification" | "browser" | "location";

const SOURCE_COLORS: Record<string, string> = {
  call: "#e05e00",
  sms: "#2f9e44",
  whatsapp: "#25D366",
  power: "#f4a300",
  notification: "#7c3aed",
  browser: "#3b82f6",
  location: "#ef4444",
  calendar: "#06b6d4",
};

const SOURCE_ICONS: Record<string, string> = {
  call: "📞",
  sms: "💬",
  whatsapp: "🟢",
  power: "🔋",
  notification: "🔔",
  browser: "🌐",
  location: "📍",
  calendar: "📅",
};

export function StandaloneTimeline({ serial }: StandaloneTimelineProps) {
  const [events, setEvents] = useState<UnifiedEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [collected, setCollected] = useState(false);
  const [query, setQuery] = useState("");
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>("all");
  const [showDeleted, setShowDeleted] = useState(true);
  const [totalSources, setTotalSources] = useState<Record<string, number>>({});

  const collectAll = useCallback(async () => {
    if (!serial) return;
    setBusy(true);
    setCollected(false);

    try {
      // Collect direct acquisition data (calls, SMS, contacts, calendar)
      const direct: DirectAcquisition = await api.directAcquisition(serial);

      // Collect deep forensics data
      const [power, notifications, browser, location] = await Promise.allSettled([
        api.collectPowerEvents(serial),
        api.collectNotifications(serial),
        api.collectBrowserData(serial),
        api.collectLocationData(serial),
      ]);

      // Format calls
      const calls = (direct.call_logs || []).map((c) => [
        c.timestamp || 0,
        c.name || c.number || "Unknown",
        c.number || "",
      ] as [number, string, string]);

      // Format SMS
      const sms = (direct.sms_messages || []).map((s) => [
        s.timestamp || 0,
        s.address || "Unknown",
        s.body || "",
        s.msg_type === "inbox" ? true : false,
      ] as [number, string, string, boolean]);

      // Format WhatsApp (from direct acquisition)
      const whatsapp: [number, string, string, boolean][] = [];

      // Format power events
      const powerArr = (power.status === "fulfilled" ? power.value : []).map((p) => [
        p.timestamp_ms,
        p.event_type,
        p.battery_level,
      ] as [number, string, number]);

      // Format notifications
      const notifArr = (notifications.status === "fulfilled" ? notifications.value : []).map((n) => [
        n.timestamp || 0,
        n.package,
        n.title || n.text || "",
      ] as [number, string, string]);

      // Format browser history
      const browserArr = (browser.status === "fulfilled" ? browser.value?.history || [] : []).map((h) => [
        h.last_visit || 0,
        h.url,
        h.title || "",
      ] as [number, string, string]);

      // Format location
      const locationArr = (location.status === "fulfilled" ? location.value?.points || [] : []).map((p) => [
        p.timestamp || 0,
        p.latitude,
        p.longitude,
        p.source || "gps",
      ] as [number, number, number, string]);

      // Build unified timeline
      const unified = await api.buildUnifiedTimeline(
        calls, sms, whatsapp, powerArr, notifArr, browserArr, locationArr, serial
      );

      setEvents(unified);

      // Count sources
      const counts: Record<string, number> = {};
      unified.forEach((e: UnifiedEvent) => {
        counts[e.category] = (counts[e.category] || 0) + 1;
      });
      setTotalSources(counts);
      setCollected(true);
    } catch (err) {
      console.error("Timeline collection failed:", err);
    }

    setBusy(false);
  }, [serial]);

  // Filter events
  const filtered = events.filter((e) => {
    if (sourceFilter !== "all" && e.category !== sourceFilter) return false;
    if (!showDeleted && e.is_deleted) return false;
    if (query) {
      const q = query.toLowerCase();
      return (
        e.title.toLowerCase().includes(q) ||
        (e.detail || "").toLowerCase().includes(q) ||
        e.category.toLowerCase().includes(q) ||
        (e.source_app || "").toLowerCase().includes(q)
      );
    }
    return true;
  });

  // Sort by timestamp descending (newest first)
  const sorted = [...filtered].sort((a, b) => b.timestamp - a.timestamp);

  // Group by date
  const grouped: Record<string, UnifiedEvent[]> = {};
  sorted.forEach((e) => {
    const date = new Date(e.timestamp).toLocaleDateString();
    if (!grouped[date]) grouped[date] = [];
    grouped[date].push(e);
  });

  const sourceKeys = Object.keys(totalSources);

  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel" style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
        <div className="panel-bar" style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
          <span>🕐 Unified Timeline</span>
          <span className="hint small">{events.length} events from {sourceKeys.length} sources</span>
        </div>

        {!collected && !busy && (
          <div className="fx-empty" style={{ flex: 1 }}>
            <span className="fx-empty-icon">🕐</span>
            <span className="fx-empty-text">
              Collect all device data into a unified chronological timeline.<br />
              Includes calls, SMS, WhatsApp, power events, notifications, browser history, and location.
            </span>
            <button className="primary" disabled={!serial} onClick={collectAll}>
              🕐 Build Timeline
            </button>
          </div>
        )}

        {busy && (
          <div className="fx-empty" style={{ flex: 1 }}>
            <span className="spin" />
            <span className="fx-empty-text">Collecting data from device and building timeline…</span>
          </div>
        )}

        {collected && (
          <>
            {/* Toolbar */}
            <div className="fx-toolbar" style={{ display: "flex", gap: 8, alignItems: "center", flexWrap: "wrap" }}>
              <input
                className="fx-search"
                type="text"
                placeholder="Search events…"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                style={{ flex: 1, minWidth: 150 }}
              />
              <label style={{ fontSize: 10, display: "flex", alignItems: "center", gap: 3 }}>
                <input type="checkbox" checked={showDeleted} onChange={(e) => setShowDeleted(e.target.checked)} />
                Deleted
              </label>
              <button className="mini" onClick={collectAll} disabled={busy}>⟳ Refresh</button>
            </div>

            {/* Source filter chips */}
            <div style={{ display: "flex", gap: 4, padding: "4px 12px", flexWrap: "wrap", borderBottom: "1px solid var(--line)" }}>
              <button
                className={`fx-tab${sourceFilter === "all" ? " active" : ""}`}
                onClick={() => setSourceFilter("all")}
                style={{ fontSize: 10 }}
              >
                All ({events.length})
              </button>
              {sourceKeys.map((k) => (
                <button
                  key={k}
                  className={`fx-tab${sourceFilter === k ? " active" : ""}`}
                  onClick={() => setSourceFilter(k as SourceFilter)}
                  style={{ fontSize: 10 }}
                >
                  {SOURCE_ICONS[k] || "•"} {k} ({totalSources[k] || 0})
                </button>
              ))}
            </div>

            {/* Timeline content */}
            <div className="panel-body" style={{ flex: 1, overflow: "auto" }}>
              {sorted.length === 0 ? (
                <div className="fx-empty">
                  <span className="fx-empty-icon">🔍</span>
                  <span className="fx-empty-text">No events match your filters</span>
                </div>
              ) : (
                <div className="timeline-groups">
                  {Object.entries(grouped).map(([date, evts]) => (
                    <div key={date} className="timeline-date-group">
                      <div className="timeline-date-header">{date}</div>
                      {evts.map((e, i) => (
                        <div key={i} className={`timeline-event${e.is_deleted ? " deleted" : ""}`}>
                          <div className="timeline-event-time">
                            {new Date(e.timestamp).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}
                          </div>
                          <div className="timeline-event-icon" style={{ color: SOURCE_COLORS[e.category] || "#888" }}>
                            {SOURCE_ICONS[e.category] || "•"}
                          </div>
                          <div className="timeline-event-body">
                            <div className="timeline-event-title">
                              <span className="timeline-event-category" style={{ background: SOURCE_COLORS[e.category] || "#888", color: "#fff", padding: "1px 6px", borderRadius: 4, fontSize: 9, marginRight: 6 }}>
                                {e.category}
                              </span>
                              {e.title}
                              {e.is_deleted && <span className="timeline-deleted-badge">DELETED</span>}
                            </div>
                            {e.detail && <div className="timeline-event-detail">{e.detail}</div>}
                            {e.source_app && <div className="timeline-event-source">via {e.source_app}</div>}
                          </div>
                        </div>
                      ))}
                    </div>
                  ))}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </section>
  );
}
