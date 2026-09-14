import { DeepForensicsPanel } from "./DeepForensicsPanel";

type Step = "connect" | "apps" | "system" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "artifacts" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

interface ArtifactsStepProps {
  serial: string;
  onGoTo: (step: Step) => void;
}

export function ArtifactsStep({ serial, onGoTo }: ArtifactsStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">🔍 Device Artifacts</div>
        <div className="panel-body">
          <p className="hint small" style={{ marginBottom: 12 }}>
            Collect WiFi networks, Bluetooth devices, device accounts, power events, notifications, and dictionary words from the device.
          </p>
          <DeepForensicsPanel serial={serial} />
          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("sim_forensics")}>← SIM Forensics</button>
            <button className="mini" onClick={() => onGoTo("deleted")}>Continue to Deleted →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
