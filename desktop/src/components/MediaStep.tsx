import { convertFileSrc } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import type { MediaFile, MediaInspect, MediaRecovery } from "../types";
import { fmtSize } from "../types";
import { MediaGrid } from "./HelperComponents";

interface MediaStepProps {
  serial: string;
  busy: boolean;
  mediaBusy: boolean;
  media: MediaRecovery | null;
  mediaProgress: { folder: string; index: number; total: number } | null;
  viewMedia: MediaRecovery | null;
  gridFiles: MediaFile[];
  preview: MediaFile | null;
  inspect: MediaInspect | null;
  inspecting: boolean;
  showImages: boolean;
  showVideos: boolean;
  lostSet: Set<string> | null;
  presenceBusy: boolean;
  presenceAt: number;
  onRecoverMedia: (source: string) => void;
  onSaveMedia: () => void;
  onOpenMediaDir: () => void;
  onVerifyOnDevice: () => void;
  onOpenPreview: (f: MediaFile) => void;
  onInspect: () => void;
  onShowImagesChange: (v: boolean) => void;
  onShowVideosChange: (v: boolean) => void;
  onGoTo: (step: "connect") => void;
}

export function MediaStep({
  serial, busy, mediaBusy, media, mediaProgress, viewMedia, gridFiles,
  preview, inspect, inspecting, showImages, showVideos,
  lostSet, presenceBusy, presenceAt,
  onRecoverMedia, onSaveMedia, onOpenMediaDir, onVerifyOnDevice,
  onOpenPreview, onInspect, onShowImagesChange, onShowVideosChange, onGoTo,
}: MediaStepProps) {
  return (
    <section className="workbench">
      <div className="panel" style={{ gridArea: "acq" }}>
        <div className="panel-bar">📥 Acquire</div>
        <div className="panel-body">
          {!serial && (
            <div className="empty" style={{ textAlign: "center", padding: "12px 0" }}>
              <p className="hint small" style={{ margin: "0 0 6px" }}>Connect a device first.</p>
              <button className="mini primary" onClick={() => onGoTo("connect")}>Go to ① Connect</button>
            </div>
          )}
          <button className="primary" disabled={!serial || busy || mediaBusy} onClick={() => onRecoverMedia("whatsapp")}>💬 WhatsApp media</button>
          <button className="primary" disabled={!serial || busy || mediaBusy} onClick={() => onRecoverMedia("gallery")}>🖼 Gallery (any app)</button>
          <button disabled={!serial || busy || mediaBusy} onClick={() => onRecoverMedia("downloads")}>📁 Downloads</button>
          {media && !mediaBusy && media.files.length > 0 && (
            <>
              <hr className="pdiv" />
              <button className="primary" disabled={busy} onClick={onSaveMedia}>💾 Save all…</button>
              <button disabled={busy} onClick={onOpenMediaDir}>📂 Open folder</button>
              <button disabled={!serial || busy || presenceBusy} onClick={onVerifyOnDevice}>
                {presenceBusy ? "📡 Checking the phone…" : "📡 Verify on device"}
              </button>
              <p className="hint small">Re-reads the phone (read-only) and flags every recovered file that is lost from the device — its local copy is then the only evidence.</p>
            </>
          )}
          <p className="hint">Read-only — nothing on the phone is touched. Already-copied files are reused on re-runs.</p>
        </div>
      </div>

      <div className="panel" style={{ gridArea: "gallery" }}>
        <div className="panel-bar">
          🖼 Gallery
          <span className="sep" />
          <label className="check"><input type="checkbox" checked={showImages} onChange={(e) => onShowImagesChange(e.target.checked)} /> images</label>
          <label className="check"><input type="checkbox" checked={showVideos} onChange={(e) => onShowVideosChange(e.target.checked)} /> videos</label>
        </div>
        <div className="panel-body">
          {viewMedia && viewMedia.files.length > 0 ? (
            <MediaGrid files={gridFiles} selectedPath={preview?.path ?? ""} onOpen={onOpenPreview} lost={lostSet} />
          ) : (
            <div className="empty">{mediaBusy ? "Copying media from the phone…" : "No media yet — pick an acquisition source in the Acquire panel."}</div>
          )}
        </div>
      </div>

      <div className="panel" style={{ gridArea: "inspector" }}>
        <div className="panel-bar">🔍 Inspector</div>
        <div className="panel-body">
          {preview ? (
            <>
              {preview.kind === "image" ? (
                <img className="insp-img" src={convertFileSrc(preview.path)} alt="" />
              ) : (
                <video className="insp-img" src={convertFileSrc(preview.path)} controls />
              )}
              <span className="mono small">{preview.path.split(/[\\/]/).pop()}</span>
              <div className="stats">
                <span className="pill">{fmtSize(preview.size)}</span>
                {inspect && (
                  <span className={`pill ${inspect.valid ? "ok" : "warn"}`}>
                    {inspect.valid ? `✔ genuine ${inspect.format}` : "⚠ invalid media"}
                  </span>
                )}
              </div>
              <button className="primary" disabled={inspecting} onClick={onInspect}>
                {inspecting ? "Reading bytes…" : inspect ? "✔ Re-verify file" : "✔ Verify real file"}
              </button>
              <button onClick={() => openPath(preview.path)}>🔓 Open in viewer</button>
              {inspect && (
                <p className={`hint ${inspect.valid ? "" : "err"}`}>
                  {inspect.valid
                    ? `Genuine ${inspect.format} — ${fmtSize(inspect.size)} of real bytes from the phone. SHA-256: `
                    : "⚠ Not valid image/video bytes — corrupted or incomplete. "}
                  <span className="mono small">{inspect.sha256}</span>
                </p>
              )}
            </>
          ) : (
            <p className="hint">Click any thumbnail in the gallery to inspect & verify it here.</p>
          )}
        </div>
      </div>

      <div className="panel timeline" style={{ gridArea: "timeline" }}>
        <div className="panel-body tl">
          {serial && mediaBusy && mediaProgress && (
            <div className="tl-progress">
              <div className="progress-track">
                <div className="progress-fill" style={{ width: mediaProgress.total ? `${Math.round((mediaProgress.index / mediaProgress.total) * 100)}%` : "6%" }} />
              </div>
              <span className="hint nowrap">⬇ <b>{mediaProgress.folder}</b>{mediaProgress.total > 0 && <> · {mediaProgress.index}/{mediaProgress.total}</>}{viewMedia && <> · {viewMedia.files.length} files</>}</span>
            </div>
          )}
          {viewMedia && (
            <div className="stats">
              <span className="pill ok">{viewMedia.files.filter((f) => f.kind === "image").length} images</span>
              <span className="pill ok">{viewMedia.files.filter((f) => f.kind === "video").length} videos</span>
              {lostSet && (
                <>
                  <span className="pill ok">{viewMedia.files.length - lostSet.size} on device</span>
                  <span className={lostSet.size ? "pill kind-media" : "pill ok"}>{lostSet.size} lost from device</span>
                  <span className="hint nowrap">checked {new Date(presenceAt).toLocaleTimeString()}</span>
                </>
              )}
              <span className="pill">{fmtSize(viewMedia.files.reduce((a, f) => a + f.size, 0))}</span>
              {mediaBusy && <span className="pill">recovering…</span>}
              {!mediaBusy && viewMedia.failed_dirs > 0 && <span className="pill warn">{viewMedia.failed_dirs} folder(s) failed</span>}
            </div>
          )}
          {!viewMedia && !mediaBusy && <span className="hint">Ready — choose an acquisition source.</span>}
        </div>
      </div>
    </section>
  );
}
