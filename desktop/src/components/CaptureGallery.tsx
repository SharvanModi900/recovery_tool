import { useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import type { ScreenCapture } from "../types";
import { fmtSize } from "../types";

interface CaptureGalleryProps {
  serial: string | null;
  captures: ScreenCapture[];
  captureBusy: boolean;
  autoCaptureActive: boolean;
  captureInterval: number;
  onCapture: () => void;
  onStartAuto: () => void;
  onStopAuto: () => void;
  onIntervalChange: (v: number) => void;
  onRefresh: () => void;
  onDelete: (id: number) => void;
}

export function CaptureGallery({
  serial, captures, captureBusy, autoCaptureActive, captureInterval,
  onCapture, onStartAuto, onStopAuto, onIntervalChange, onRefresh, onDelete,
}: CaptureGalleryProps) {
  const [selected, setSelected] = useState<ScreenCapture | null>(null);

  return (
    <>
      <div className="mirror-gallery-col">
        <div className="mirror-gal-head">
          <span>Captures</span>
          <span className="pill" style={{ fontSize: 8.5, padding: "0 5px" }} title="Only distinct, meaningful screens are saved">✨ Smart</span>
          {autoCaptureActive && <span className="pill ok" style={{ fontSize: 8.5, padding: "0 5px" }}>● Live</span>}
          <span className="sep" />
          <button className="primary" disabled={!serial || captureBusy || autoCaptureActive} onClick={onCapture} title="Capture now" style={{ padding: "2px 6px", fontSize: 10.5 }}>
            {captureBusy ? "⏳" : "📸"}
          </button>
          {autoCaptureActive ? (
            <button className="mini" onClick={onStopAuto} title="Stop auto-capture" style={{ padding: "1px 5px", fontSize: 10 }}>⏹</button>
          ) : (
            <button className="mini" disabled={!serial} onClick={onStartAuto} title="Auto-capture" style={{ padding: "1px 5px", fontSize: 10 }}>▶</button>
          )}
          <select className="mh-interval" value={captureInterval} onChange={(e) => onIntervalChange(Number(e.target.value))} disabled={autoCaptureActive} title="Interval" style={{ width: 38, padding: "1px 2px", fontSize: 10 }}>
            <option value={5}>5s</option><option value={10}>10s</option><option value={30}>30s</option><option value={60}>60s</option>
          </select>
          <button className="mini" onClick={onRefresh} title="Refresh" style={{ padding: "1px 5px", fontSize: 10 }}>⟳</button>
        </div>
        <div className="mirror-gal-list">
          {captures.length === 0 ? (
            <div className="empty" style={{ padding: 6, fontSize: 10 }}>No captures yet</div>
          ) : (
            captures.map((cap) => (
              <div key={cap.id} className="mh-item" onClick={() => setSelected(cap)}>
                <img src={convertFileSrc(cap.file_path)} alt="capture" />
                <div className="mh-item-info">
                  <span className="mh-time">{new Date(cap.timestamp).toLocaleTimeString()}</span>
                  <span className="mh-meta" title={cap.note || undefined}>
                    {cap.note ? cap.note.replace("auto · ", "📱 ") : (cap.device_model || cap.device_serial)}
                    {cap.dark_frame ? " · 🔒" : ""} · {fmtSize(cap.file_size)}
                  </span>
                </div>
                <button className="mh-del" onClick={(e) => { e.stopPropagation(); onDelete(cap.id); }} title="Delete">×</button>
              </div>
            ))
          )}
        </div>
      </div>

      {/* Viewer modal */}
      {selected && (
        <div style={{
          position: "fixed", top: 0, left: 0, right: 0, bottom: 0,
          background: "rgba(0,0,0,0.7)", zIndex: 9999,
          display: "flex", alignItems: "center", justifyContent: "center",
          padding: 40
        }} onClick={() => setSelected(null)}>
          <div style={{
            background: "#ffffff", borderRadius: 12, padding: 20,
            maxWidth: "90vw", maxHeight: "90vh", overflow: "auto",
            border: "2px solid #f4a300"
          }} onClick={(e) => e.stopPropagation()}>
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
              <h3 style={{ margin: 0, color: "#c77700", fontSize: 16 }}>📸 Capture Details</h3>
              <button onClick={() => setSelected(null)} style={{ background: "none", border: "none", fontSize: 20, cursor: "pointer" }}>×</button>
            </div>
            <img src={convertFileSrc(selected.file_path)} alt="capture" style={{ maxWidth: "100%", maxHeight: "60vh", borderRadius: 8, border: "1px solid #ddd" }} />
            <div style={{ marginTop: 12, fontSize: 12, color: "#3a2e1f" }}>
              <p style={{ margin: "4px 0" }}><b>Time:</b> {new Date(selected.timestamp).toLocaleString()}</p>
              <p style={{ margin: "4px 0" }}><b>Device:</b> {selected.device_model || selected.device_serial}</p>
              <p style={{ margin: "4px 0" }}><b>Size:</b> {fmtSize(selected.file_size)}</p>
              <p style={{ margin: "4px 0" }}><b>Note:</b> {selected.note || "—"}</p>
              {selected.dark_frame && <p style={{ margin: "4px 0", color: "#c77700" }}>🔒 Dark frame — device was locked</p>}
              <p style={{ margin: "4px 0", fontSize: 10, color: "#8a7a60" }}>Path: {selected.file_path}</p>
            </div>
            <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
              <button onClick={() => { openPath(selected.file_path); setSelected(null); }}>📂 Open File</button>
              <button onClick={() => { onDelete(selected.id); setSelected(null); }}>🗑 Delete</button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
