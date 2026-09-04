import { useState } from "react";
import type { AppInventory, AppDetection } from "../types";

interface AppsPanelProps {
  inventory: AppInventory | null;
  onDetect: () => void;
  onExtract: (appId: string) => void;
  loading?: boolean;
}

export function AppsPanel({ inventory, onDetect, onExtract, loading }: AppsPanelProps) {
  const [selectedApp, setSelectedApp] = useState<string | null>(null);

  if (!inventory) {
    return (
      <div style={{ padding: 24, textAlign: "center" }}>
        <div style={{ fontSize: 48, marginBottom: 16 }}>📱</div>
        <h3>Detect Messaging Apps</h3>
        <p style={{ color: "#666", marginBottom: 16 }}>
          Scan the device for supported messaging apps and extract forensic data.
        </p>
        <button className="btn primary" onClick={onDetect} disabled={loading}>
          {loading ? "⏳ Scanning..." : "🔍 Detect Apps"}
        </button>
      </div>
    );
  }

  const installedApps = inventory.apps.filter(a => a.is_installed);

  return (
    <div style={{ padding: 16 }}>
      {/* Summary Header */}
      <div style={{ display: "flex", gap: 12, marginBottom: 16, flexWrap: "wrap" }}>
        <span className="pill ok">📱 {inventory.total_installed} Apps Installed</span>
        <span className="pill ok">💾 {inventory.total_with_data} With Data</span>
        {inventory.device_has_root && (
          <span className="pill ok">🔓 Root Access</span>
        )}
        {!inventory.device_has_root && (
          <span className="pill" style={{ background: "#fff3cd", color: "#856404" }}>
            🔒 No Root
          </span>
        )}
        <button className="mini" onClick={onDetect} disabled={loading} style={{ marginLeft: "auto" }}>
          🔄 Rescan
        </button>
      </div>

      {/* Apps Grid */}
      <div style={{ 
        display: "grid", 
        gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", 
        gap: 12 
      }}>
        {installedApps.map(app => (
          <AppCard
            key={app.id}
            app={app}
            isSelected={selectedApp === app.id}
            onSelect={() => setSelectedApp(selectedApp === app.id ? null : app.id)}
            onExtract={() => onExtract(app.id)}
          />
        ))}
      </div>

      {/* Not Installed Apps */}
      {inventory.apps.filter(a => !a.is_installed).length > 0 && (
        <div style={{ marginTop: 24 }}>
          <h4 style={{ color: "#999", marginBottom: 8 }}>Not Installed</h4>
          <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
            {inventory.apps.filter(a => !a.is_installed).map(app => (
              <span key={app.id} className="pill" style={{ opacity: 0.5 }}>
                {app.icon} {app.name}
              </span>
            ))}
          </div>
        </div>
      )}

      {/* Selected App Details */}
      {selectedApp && (
        <AppDetails
          app={inventory.apps.find(a => a.id === selectedApp)!}
          onClose={() => setSelectedApp(null)}
        />
      )}
    </div>
  );
}

function AppCard({ app, isSelected, onSelect, onExtract }: {
  app: AppDetection;
  isSelected: boolean;
  onSelect: () => void;
  onExtract: () => void;
}) {
  return (
    <div
      onClick={onSelect}
      style={{
        padding: 16,
        border: `2px solid ${isSelected ? "#2e7d32" : app.has_data ? "#4caf50" : "#e0e0e0"}`,
        borderRadius: 8,
        cursor: "pointer",
        background: isSelected ? "rgba(46, 125, 50, 0.05)" : app.has_data ? "rgba(76, 175, 80, 0.02)" : "#fafafa",
        transition: "all 0.2s"
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 8 }}>
        <span style={{ fontSize: 32 }}>{app.icon}</span>
        <div>
          <div style={{ fontWeight: "bold", fontSize: 16 }}>{app.name}</div>
          <div style={{ fontSize: 11, color: "#999", fontFamily: "monospace" }}>{app.package}</div>
        </div>
      </div>

      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 8 }}>
        {app.has_data && (
          <span className="pill ok" style={{ fontSize: 10, padding: "2px 6px" }}>
            ✓ Data Found
          </span>
        )}
        {app.requires_root && !app.root_paths_found.length && (
          <span className="pill" style={{ fontSize: 10, padding: "2px 6px", background: "#fff3cd", color: "#856404" }}>
            🔒 Root Required
          </span>
        )}
        {app.media_count > 0 && (
          <span className="pill ok" style={{ fontSize: 10, padding: "2px 6px" }}>
            📁 {app.media_count} files
          </span>
        )}
      </div>

      {app.has_data && (
        <button
          className="mini"
          onClick={(e) => { e.stopPropagation(); onExtract(); }}
          style={{ width: "100%", marginTop: 4 }}
        >
          🔍 Extract Data
        </button>
      )}
    </div>
  );
}

function AppDetails({ app, onClose }: { app: AppDetection; onClose: () => void }) {
  return (
    <div style={{
      marginTop: 16,
      padding: 16,
      background: "#f5f5f5",
      borderRadius: 8,
      border: "1px solid #ddd"
    }}>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 }}>
        <h3 style={{ margin: 0 }}>{app.icon} {app.name} Details</h3>
        <button className="mini" onClick={onClose}>✕</button>
      </div>

      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
        {/* Accessible Paths */}
        <div>
          <h4 style={{ margin: "0 0 8px", fontSize: 13 }}>📂 Accessible Paths</h4>
          {app.accessible_paths_found.length > 0 ? (
            <ul style={{ margin: 0, paddingLeft: 20, fontSize: 12 }}>
              {app.accessible_paths_found.map((p, i) => (
                <li key={i} style={{ fontFamily: "monospace", color: "#2e7d32" }}>{p}</li>
              ))}
            </ul>
          ) : (
            <span style={{ color: "#999", fontSize: 12 }}>None found</span>
          )}
        </div>

        {/* Root Paths */}
        <div>
          <h4 style={{ margin: "0 0 8px", fontSize: 13 }}>🔐 Root Paths</h4>
          {app.root_paths_found.length > 0 ? (
            <ul style={{ margin: 0, paddingLeft: 20, fontSize: 12 }}>
              {app.root_paths_found.map((p, i) => (
                <li key={i} style={{ fontFamily: "monospace", color: "#2e7d32" }}>{p}</li>
              ))}
            </ul>
          ) : (
            <span style={{ color: "#999", fontSize: 12 }}>
              {app.requires_root ? "Root access needed" : "None found"}
            </span>
          )}
        </div>

        {/* Databases */}
        <div>
          <h4 style={{ margin: "0 0 8px", fontSize: 13 }}>🗄️ Databases</h4>
          {app.databases_found.length > 0 ? (
            <ul style={{ margin: 0, paddingLeft: 20, fontSize: 12 }}>
              {app.databases_found.map((db, i) => (
                <li key={i} style={{ fontFamily: "monospace", color: "#1565c0" }}>{db}</li>
              ))}
            </ul>
          ) : (
            <span style={{ color: "#999", fontSize: 12 }}>No databases found</span>
          )}
        </div>

        {/* Media Stats */}
        <div>
          <h4 style={{ margin: "0 0 8px", fontSize: 13 }}>📊 Statistics</h4>
          <div style={{ fontSize: 12 }}>
            <div>Media files: <b>{app.media_count}</b></div>
            <div>Status: <b style={{ color: app.has_data ? "#2e7d32" : "#999" }}>
              {app.has_data ? "Data Available" : "No Data"}
            </b></div>
          </div>
        </div>
      </div>
    </div>
  );
}
