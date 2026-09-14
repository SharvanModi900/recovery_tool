import { BrowserForensicsPanel } from "./BrowserForensicsPanel";

type Step = "connect" | "apps" | "system" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "artifacts" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

interface BrowserForensicsStepProps {
  serial: string;
  onGoTo: (step: Step) => void;
}

export function BrowserForensicsStep({ serial, onGoTo }: BrowserForensicsStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">🌐 Browser Forensics</div>
        <div className="panel-body">
          <p className="hint small" style={{ marginBottom: 12 }}>
            Collect browser history, bookmarks, downloads, and cookies from the device.
          </p>
          <BrowserForensicsPanel serial={serial} />
          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("deleted")}>← Back to Deleted Data</button>
            <button className="mini" onClick={() => onGoTo("network")}>Continue to Network →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
