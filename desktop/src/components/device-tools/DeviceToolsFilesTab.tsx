import { useState, useCallback } from "react";
import * as api from "../../api";
import { type DeviceToolsTabProps, type FileEntry, OUT, inputStyle, miniBtn, formatSize } from "./shared";

export function DeviceToolsFilesTab({ serial, run }: DeviceToolsTabProps) {
  const [filePath, setFilePath] = useState("/sdcard");
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [fileBusy, setFileBusy] = useState(false);

  const loadFiles = useCallback(async (path: string) => {
    if (!serial) return;
    setFileBusy(true);
    setFilePath(path);
    try {
      const raw = await api.listDeviceFiles(serial, path);
      const entries: FileEntry[] = raw.split("\n").filter(Boolean).map(line => {
        const [type, size, name, perms] = line.split("|");
        return { type, size, name, perms };
      });
      setFiles(entries);
    } catch { setFiles([]); }
    finally { setFileBusy(false); }
  }, [serial]);

  return (
    <div>
      <div style={{ display: "flex", gap: 4, padding: "6px 12px", borderBottom: "1px solid var(--border, #222)", alignItems: "center" }}>
        <button disabled={fileBusy || filePath === "/"} onClick={() => { const p = filePath.split("/").slice(0, -1).join("/") || "/"; loadFiles(p); }} style={miniBtn}>⬆ Up</button>
        <input value={filePath} onChange={e => setFilePath(e.target.value)} onKeyDown={e => e.key === "Enter" && loadFiles(filePath)} style={{ ...inputStyle("100%" as any), flex: 1 }} />
        <button disabled={fileBusy} onClick={() => loadFiles(filePath)} style={miniBtn}>{fileBusy ? "⏳" : "🔄"} Go</button>
        <button disabled={!serial} onClick={() => run("pullfile", () => api.pullDeviceFile(serial!, filePath, OUT))} style={miniBtn}>📥 Pull</button>
      </div>
      <div style={{ flex: 1, overflow: "auto", padding: "4px 12px", minHeight: 200, maxHeight: 400 }}>
        {files.length === 0 ? (
          <div style={{ color: "var(--text-secondary, #666)", fontSize: 11, textAlign: "center", padding: 20 }}>
            {fileBusy ? "Loading..." : "No files found or path doesn't exist"}
          </div>
        ) : (
          <table style={{ width: "100%", fontSize: 10.5, borderCollapse: "collapse" }}>
            <thead><tr style={{ borderBottom: "1px solid var(--border, #333)", color: "var(--text-secondary, #888)" }}>
              <th style={{ textAlign: "left", padding: "3px 6px" }}>Name</th>
              <th style={{ textAlign: "right", padding: "3px 6px", width: 60 }}>Size</th>
              <th style={{ textAlign: "left", padding: "3px 6px", width: 80 }}>Perms</th>
            </tr></thead>
            <tbody>
              {files.map((f, i) => (
                <tr key={i} style={{ cursor: f.type === "D" ? "pointer" : "default", background: i % 2 === 0 ? "transparent" : "rgba(255,255,255,0.02)" }}
                  onClick={() => f.type === "D" && loadFiles(`${filePath}/${f.name}`)}
                  onDoubleClick={() => f.type === "F" && run("pullfile", () => api.pullDeviceFile(serial!, `${filePath}/${f.name}`, OUT))}>
                  <td style={{ padding: "2px 6px", fontFamily: "monospace" }}>
                    <span style={{ marginRight: 4 }}>{f.type === "D" ? "📁" : "📄"}</span>{f.name}
                  </td>
                  <td style={{ padding: "2px 6px", textAlign: "right", fontFamily: "monospace", color: "var(--text-secondary, #888)" }}>
                    {f.type === "F" ? formatSize(parseInt(f.size)) : "—"}
                  </td>
                  <td style={{ padding: "2px 6px", fontFamily: "monospace", fontSize: 9, color: "var(--text-secondary, #666)" }}>{f.perms}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
