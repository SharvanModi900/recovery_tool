import { DeviceToolsPanel } from "./DeviceToolsPanel";

type Step = "connect" | "apps" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "forensics" | "device_tools" | "recover" | "export" | "case" | "timeline";

interface DeviceToolsStepProps {
  serial: string | null;
  onGoTo: (step: Step) => void;
}

export function DeviceToolsStep({ serial, onGoTo }: DeviceToolsStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">🛠 Device Tools</div>
        <div className="panel-body" style={{ padding: 0, display: "flex", flexDirection: "column", minHeight: 500 }}>
          <DeviceToolsPanel serial={serial} />
          <div style={{ padding: "8px 12px", display: "flex", gap: 8, borderTop: "1px solid var(--border, #222)" }}>
            <button className="mini" onClick={() => onGoTo("forensics")}>← Forensics Suite</button>
            <button className="mini" onClick={() => onGoTo("connect")}>← Back to Connect</button>
            <div style={{ flex: 1 }} />
            <button className="mini" onClick={() => onGoTo("recover")}>Continue to Recovery →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
