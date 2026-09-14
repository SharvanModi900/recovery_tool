import { useState, useEffect, useCallback } from "react";
import * as api from "../api";

interface DeletedPhotoGalleryProps {
  serial: string | null;
  outputDir: string;
  onClose: () => void;
}

interface ThumbEntry {
  name: string;
  path: string;
  size: number;
}

export function DeletedPhotoGallery({ serial, outputDir, onClose }: DeletedPhotoGalleryProps) {
  const [thumbs, setThumbs] = useState<ThumbEntry[]>([]);
  const [scanning, setScanning] = useState(false);
  const [status, setStatus] = useState("Click Scan to recover deleted thumbnails");
  const [selected, setSelected] = useState<string | null>(null);

  const scan = useCallback(async () => {
    if (!serial) return;
    setScanning(true);
    setStatus("Scanning for thumbnails...");
    try {
      const result = await api.scanThumbnails(serial, outputDir);
      setStatus(result);
      // List the recovered files
      const thumbDir = `${outputDir}\\thumbnails`;
      // Try to read thumbnail directory
      try {
        const localFiles = await api.listDeviceFiles(serial, "/sdcard/Android/data/com.whatsapp/cache/Thumbs");
        const entries: ThumbEntry[] = localFiles.split("\n").filter(Boolean)
          .filter(l => l.startsWith("F|"))
          .map(l => {
            const parts = l.split("|");
            return { name: parts[2] || "unknown", path: thumbDir, size: parseInt(parts[1]) || 0 };
          });
        setThumbs(entries);
      } catch {
        setThumbs([]);
      }
    } catch (e) {
      setStatus(typeof e === "string" ? e : (e as Error).message);
    } finally {
      setScanning(false);
    }
  }, [serial, outputDir]);

  useEffect(() => {
    // Auto-scan on mount
    scan();
  }, [scan]);

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 999, background: "rgba(0,0,0,0.7)", display: "flex", alignItems: "center", justifyContent: "center" }} onClick={onClose}>
      <div onClick={e => e.stopPropagation()} style={{ background: "var(--bg, #0f0f1a)", border: "1px solid var(--border, #333)", borderRadius: 12, width: 680, maxHeight: "85vh", display: "flex", flexDirection: "column", boxShadow: "0 8px 40px rgba(0,0,0,0.5)" }}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "12px 16px", borderBottom: "1px solid var(--border, #333)" }}>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <span style={{ fontSize: 16 }}>🖼</span>
            <span style={{ fontSize: 13, fontWeight: 600, color: "var(--text, #eee)" }}>Deleted Photo Recovery</span>
            <span style={{ fontSize: 10, color: "var(--text-secondary, #888)" }}>{thumbs.length} thumbnails</span>
          </div>
          <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
            <button onClick={scan} disabled={scanning || !serial} style={{ padding: "4px 12px", fontSize: 11, borderRadius: 6, border: "1px solid var(--border, #444)", background: "var(--info, #3b82f6)", color: "#fff", cursor: "pointer" }}>
              {scanning ? "⏳ Scanning..." : "🔄 Re-scan"}
            </button>
            <button onClick={onClose} style={{ background: "none", border: "none", color: "var(--text-secondary, #aaa)", cursor: "pointer", fontSize: 16 }}>✕</button>
          </div>
        </div>

        {/* Status */}
        <div style={{ padding: "6px 16px", fontSize: 10, color: "var(--text-secondary, #888)", borderBottom: "1px solid var(--border, #222)" }}>
          {status}
        </div>

        {/* Gallery Grid */}
        <div style={{ flex: 1, overflow: "auto", padding: 12 }}>
          {thumbs.length === 0 ? (
            <div style={{ textAlign: "center", padding: 40, color: "var(--text-secondary, #666)" }}>
              <div style={{ fontSize: 40, marginBottom: 12 }}>🖼</div>
              <div style={{ fontSize: 12 }}>No thumbnails recovered yet</div>
              <div style={{ fontSize: 10, marginTop: 6 }}>Thumbnails are cached images from WhatsApp, Instagram, etc.</div>
              <div style={{ fontSize: 10 }}>Click Re-scan to try again</div>
            </div>
          ) : (
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(100px, 1fr))", gap: 8 }}>
              {thumbs.map((t, i) => (
                <div key={i} onClick={() => setSelected(t.name)}
                  style={{ border: selected === t.name ? "2px solid var(--info, #3b82f6)" : "1px solid var(--border, #333)", borderRadius: 8, overflow: "hidden", cursor: "pointer", background: "var(--panel-alt, #111)" }}>
                  <div style={{ height: 80, background: "var(--panel-alt, #1a1a2e)", display: "flex", alignItems: "center", justifyContent: "center", fontSize: 24 }}>
                    🖼
                  </div>
                  <div style={{ padding: "4px 6px", fontSize: 9, color: "var(--text-secondary, #888)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {t.name}
                  </div>
                  <div style={{ padding: "0 6px 4px", fontSize: 8, color: "var(--text-secondary, #666)" }}>
                    {t.size > 0 ? `${(t.size / 1024).toFixed(0)}KB` : ""}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div style={{ padding: "8px 16px", borderTop: "1px solid var(--border, #222)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #555)" }}>
            Output: {outputDir}\thumbnails
          </span>
          <span style={{ fontSize: 9, color: "var(--text-secondary, #555)" }}>
            {thumbs.length} files recovered
          </span>
        </div>
      </div>
    </div>
  );
}
