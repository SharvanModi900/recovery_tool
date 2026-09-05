import { useEffect, useState } from "react";
import { openPath } from "@tauri-apps/plugin-opener";
import { convertFileSrc } from "@tauri-apps/api/core";
import * as api from "../api";
import { fmtTime } from "../types";
import { AudioPlayer } from "./AudioPlayer";
import { meta, KIND_TO_CATEGORY } from "./timelineMeta";
import type { TimelineEvent } from "../types";

/**
 * Detail/inspector panel for one selected timeline event.
 *
 * Extracted from `TimelineView` to keep components small and reusable:
 * - recordings: pull the on-device file (path carried in `event.source`) and
 *   play it inline via the shared `AudioPlayer`;
 * - captures: preview the local PNG and open it;
 * - any event: jump to its Evidence bundle or the chain-of-custody log.
 */
export interface TimelineInspectorProps {
  event: TimelineEvent | null;
  /** Currently connected device serial — needed to pull a recording. */
  serial: string;
  onJump?: (target: "evidence" | "custody", category?: string) => void;
  onStatus?: (msg: string) => void;
}

export function TimelineInspector({ event, serial, onJump, onStatus }: TimelineInspectorProps) {
  const [localPath, setLocalPath] = useState<string | null>(null);
  const [pulling, setPulling] = useState(false);
  const [err, setErr] = useState("");

  // Reset playback/error whenever a different event is selected.
  useEffect(() => {
    setLocalPath(null);
    setErr("");
    setPulling(false);
  }, [event?.id]);

  if (!event) return <div className="empty">Select an event to inspect it.</div>;

  const m = meta(event.kind);
  const isRecording = event.kind === "recording";
  const isCapture = event.kind === "capture";
  const remotePath = event.source; // device path for recordings

  const pullAndPlay = async () => {
    if (!serial) {
      setErr("No device connected — cannot pull the recording.");
      return;
    }
    if (!remotePath || !remotePath.startsWith("/")) {
      setErr("No device path on this recording — re-bundle the case (Acquire & bundle all sources).");
      return;
    }
    setPulling(true);
    setErr("");
    setLocalPath(null);
    try {
      const p = await api.pullRecording(serial, remotePath);
      setLocalPath(p);
      onStatus?.("Pulled recording for playback");
    } catch (e) {
      setErr("Pull failed: " + String(e));
    } finally {
      setPulling(false);
    }
  };

  return (
    <>
      <div className="stats">
        <span className="pill">
          {m.icon} {m.label}
        </span>
      </div>
      <div style={{ fontWeight: 700, fontSize: 13, wordBreak: "break-word" }}>{event.title}</div>
      <div className="small" style={{ color: "var(--muted)" }}>{fmtTime(event.timestamp)}</div>

      {isCapture && event.local_path && (
        <img
          src={convertFileSrc(event.local_path)}
          alt="capture"
          style={{ width: "100%", borderRadius: 6, border: "1px solid var(--line)" }}
        />
      )}

      {event.detail && (
        <div style={{ whiteSpace: "pre-wrap", wordBreak: "break-word", fontSize: 12 }}>{event.detail}</div>
      )}

      {isRecording && (
        <div>
          {localPath ? (
            <AudioPlayer filename={event.title} localPath={localPath} />
          ) : (
            <button className="primary mini" disabled={pulling} onClick={pullAndPlay}>
              {pulling ? "Pulling…" : "▶ Pull & play recording"}
            </button>
          )}
          {err && <p className="hint small" style={{ color: "#8c2f22" }}>{err}</p>}
        </div>
      )}

      <hr className="pdiv" />
      <div className="small">
        <b>Source:</b>{" "}
        <span className="mono" style={{ wordBreak: "break-all" }}>{event.source || "—"}</span>
      </div>
      {event.local_path && (
        <button className="mini" onClick={() => openPath(event.local_path as string)}>
          📂 Open file
        </button>
      )}

      <hr className="pdiv" />
      <div className="row">
        <button className="mini" onClick={() => onJump?.("evidence", KIND_TO_CATEGORY[event.kind])}>
          📄 Show in Evidence
        </button>
        <button className="mini" onClick={() => onJump?.("custody")}>
          🔏 Show in Custody
        </button>
      </div>
    </>
  );
}
