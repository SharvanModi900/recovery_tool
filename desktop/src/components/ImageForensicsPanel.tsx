import { useState, useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { imageEla, imageMetadata } from "../api";

interface ImageForensicsPanelProps {
  onClose: () => void;
}

const OUT = "D:\\recovery-tool\\workspace";

export function ImageForensicsPanel({ onClose }: ImageForensicsPanelProps) {
  const [selectedImage, setSelectedImage] = useState<string | null>(null);
  const [elaResult, setElaResult] = useState<string | null>(null);
  const [elaPath, setElaPath] = useState<string | null>(null);
  const [metadataResult, setMetadataResult] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [quality, setQuality] = useState(90);
  const [scale, setScale] = useState(15);
  const [viewMode, setViewMode] = useState<"original" | "ela" | "side">("side");

  const pickImage = useCallback(async () => {
    const path = await open({
      filters: [{ name: "Images", extensions: ["jpg", "jpeg", "png", "bmp", "gif", "webp"] }],
      multiple: false,
    });
    if (path) {
      setSelectedImage(path);
      setElaResult(null);
      setElaPath(null);
      setMetadataResult(null);
    }
  }, []);

  const runEla = useCallback(async () => {
    if (!selectedImage) return;
    setBusy(true);
    setElaResult(null);
    setElaPath(null);
    try {
      const result = await imageEla(selectedImage, OUT, quality, scale);
      setElaResult(result);
      // Extract the ELA output path from the report
      const m = result.match(/ELA output: (.+)/);
      if (m) setElaPath(m[1]);
    } catch (e) {
      setElaResult(`⚠ Error: ${e}`);
    } finally {
      setBusy(false);
    }
  }, [selectedImage, quality, scale]);

  const runMetadata = useCallback(async () => {
    if (!selectedImage) return;
    setBusy(true);
    setMetadataResult(null);
    try {
      const result = await imageMetadata(selectedImage);
      setMetadataResult(result);
    } catch (e) {
      setMetadataResult(`⚠ Error: ${e}`);
    } finally {
      setBusy(false);
    }
  }, [selectedImage]);

  // Parse verdict from ELA result
  const verdict = elaResult?.match(/Verdict: (.+)/)?.[1] || null;
  const verdictColor = verdict?.includes("HIGH") ? "#ef4444" : verdict?.includes("MODERATE") ? "#f59e0b" : "#22c55e";

  return (
    <div style={{ position: "fixed", inset: 0, zIndex: 999, background: "rgba(0,0,0,0.7)", display: "flex", alignItems: "center", justifyContent: "center" }} onClick={onClose}>
      <div onClick={e => e.stopPropagation()} style={{ background: "var(--bg, #0f0f1a)", border: "1px solid var(--border, #333)", borderRadius: 12, width: 900, maxHeight: "90vh", display: "flex", flexDirection: "column", boxShadow: "0 8px 40px rgba(0,0,0,0.5)" }}>
        {/* Header */}
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "10px 16px", borderBottom: "1px solid var(--border, #333)" }}>
          <span style={{ fontSize: 14, fontWeight: 700, color: "var(--text, #eee)" }}>📸 Image Forensics — Error Level Analysis</span>
          <button onClick={onClose} style={{ background: "none", border: "none", color: "var(--text-secondary, #aaa)", cursor: "pointer", fontSize: 16 }}>✕</button>
        </div>

        {/* Controls */}
        <div style={{ display: "flex", gap: 8, padding: "8px 12px", flexWrap: "wrap", alignItems: "center", borderBottom: "1px solid var(--border, #222)" }}>
          <button onClick={pickImage} style={btnStyle}>
            📂 {selectedImage ? "Change Image" : "Select Image"}
          </button>
          {selectedImage && (
            <>
              <button onClick={runEla} disabled={busy} style={{ ...btnStyle, background: busy ? "#555" : "linear-gradient(135deg, #3b82f6, #8b5cf6)", color: "#fff" }}>
                {busy ? "⏳ Analyzing…" : "🔍 Run ELA"}
              </button>
              <button onClick={runMetadata} disabled={busy} style={btnStyle}>
                📋 Metadata
              </button>
              <div style={{ display: "flex", gap: 4, alignItems: "center", marginLeft: 8 }}>
                <span style={{ fontSize: 10, color: "#888" }}>Quality:</span>
                <input type="range" min={50} max={100} value={quality} onChange={e => setQuality(+e.target.value)} style={{ width: 60, accentColor: "#3b82f6" }} />
                <span style={{ fontSize: 10, fontFamily: "monospace", color: "#aaa", width: 20 }}>{quality}</span>
              </div>
              <div style={{ display: "flex", gap: 4, alignItems: "center" }}>
                <span style={{ fontSize: 10, color: "#888" }}>Scale:</span>
                <input type="range" min={1} max={50} value={scale} onChange={e => setScale(+e.target.value)} style={{ width: 60, accentColor: "#f59e0b" }} />
                <span style={{ fontSize: 10, fontFamily: "monospace", color: "#aaa", width: 20 }}>{scale}x</span>
              </div>
              {/* View mode toggle */}
              {elaPath && (
                <div style={{ display: "flex", gap: 2, marginLeft: 8 }}>
                  {(["original", "ela", "side"] as const).map(m => (
                    <button key={m} onClick={() => setViewMode(m)} style={{
                      padding: "2px 8px", fontSize: 10, borderRadius: 4,
                      border: viewMode === m ? "1px solid #3b82f6" : "1px solid #444",
                      background: viewMode === m ? "rgba(59,130,246,0.2)" : "transparent",
                      color: viewMode === m ? "#93c5fd" : "#888", cursor: "pointer",
                    }}>
                      {m === "side" ? "⬛⬛ Side" : m === "original" ? "📷 Original" : "🔍 ELA"}
                    </button>
                  ))}
                </div>
              )}
            </>
          )}
        </div>

        {/* Selected image path */}
        {selectedImage && (
          <div style={{ padding: "4px 12px", fontSize: 10, color: "#888", fontFamily: "monospace", borderBottom: "1px solid var(--border, #222)" }}>
            {selectedImage}
          </div>
        )}

        {/* Image display area */}
        <div style={{ flex: 1, overflow: "auto", padding: "8px 12px", minHeight: 300, display: "flex", gap: 8, justifyContent: "center", alignItems: "flex-start" }}>
          {!selectedImage ? (
            <div style={{ color: "#555", fontSize: 12, textAlign: "center", padding: 40 }}>
              <div style={{ fontSize: 40, marginBottom: 12 }}>📸</div>
              Select an image to begin forensic analysis
            </div>
          ) : (
            <>
              {/* Original image */}
              {(viewMode === "original" || viewMode === "side") && (
                <div style={{ textAlign: "center" }}>
                  <div style={{ fontSize: 10, color: "#888", marginBottom: 4 }}>Original</div>
                  <img
                    src={convertFileSrc(selectedImage)}
                    alt="original"
                    style={{ maxWidth: viewMode === "side" ? 380 : 700, maxHeight: 400, borderRadius: 6, border: "1px solid #333" }}
                  />
                </div>
              )}
              {/* ELA result image */}
              {elaPath && (viewMode === "ela" || viewMode === "side") && (
                <div style={{ textAlign: "center" }}>
                  <div style={{ fontSize: 10, color: "#888", marginBottom: 4 }}>ELA Output (diff map)</div>
                  <img
                    src={`${convertFileSrc(elaPath)}?t=${Date.now()}`}
                    alt="ela"
                    style={{ maxWidth: viewMode === "side" ? 380 : 700, maxHeight: 400, borderRadius: 6, border: "1px solid #3b82f6" }}
                  />
                </div>
              )}
            </>
          )}
        </div>

        {/* Verdict banner */}
        {verdict && (
          <div style={{
            padding: "8px 16px", textAlign: "center", fontWeight: 700, fontSize: 13,
            background: `${verdictColor}15`, color: verdictColor,
            borderTop: `1px solid ${verdictColor}40`, borderBottom: `1px solid ${verdictColor}40`,
          }}>
            {verdict}
          </div>
        )}

        {/* Results panel */}
        <div style={{ maxHeight: 180, overflow: "auto", padding: "6px 12px", borderTop: "1px solid var(--border, #222)" }}>
          {metadataResult && (
            <div style={{ marginBottom: 6 }}>
              <div style={{ fontSize: 10, fontWeight: 600, color: "#60a5fa", marginBottom: 2 }}>📋 Metadata</div>
              <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 9.5, color: "#bbb", whiteSpace: "pre-wrap" }}>{metadataResult}</pre>
            </div>
          )}
          {elaResult && (
            <div>
              <div style={{ fontSize: 10, fontWeight: 600, color: "#a78bfa", marginBottom: 2 }}>🔍 ELA Analysis</div>
              <pre style={{ margin: 0, fontFamily: "monospace", fontSize: 9.5, color: "#bbb", whiteSpace: "pre-wrap" }}>{elaResult}</pre>
            </div>
          )}
          {!elaResult && !metadataResult && (
            <div style={{ color: "#444", fontSize: 10, textAlign: "center", padding: 8 }}>
              Analysis results appear here
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

const btnStyle: React.CSSProperties = {
  padding: "4px 12px", fontSize: 11, borderRadius: 6,
  border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)",
  color: "var(--text, #eee)", cursor: "pointer",
};
