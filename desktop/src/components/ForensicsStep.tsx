import { ForensicsPanel } from "./ForensicsPanel";

type Step = "connect" | "apps" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "forensics" | "device_tools" | "recover" | "export" | "case" | "timeline";

interface ForensicsStepProps {
  serial: string;
  onGoTo: (step: Step) => void;
}

export function ForensicsStep({ serial, onGoTo }: ForensicsStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">🔬 Forensic Analysis Suite</div>
        <div className="panel-body">
          <p className="hint small" style={{ marginBottom: 12 }}>
            Comprehensive forensic analysis: device info, data extraction, network analysis, deep forensics, and security audit.
          </p>
          <ForensicsPanel serial={serial} onClose={() => onGoTo("connect")} />
          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("sim_forensics")}>← SIM Forensics</button>
            <button className="mini" onClick={() => onGoTo("recover")}>Continue to Recovery →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
