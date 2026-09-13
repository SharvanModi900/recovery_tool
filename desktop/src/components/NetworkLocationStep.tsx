import { NetworkLocationPanel } from "./NetworkLocationPanel";

type Step = "connect" | "apps" | "scan" | "media" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

interface NetworkLocationStepProps {
  serial: string;
  onGoTo: (step: Step) => void;
}

export function NetworkLocationStep({ serial, onGoTo }: NetworkLocationStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">📍 Network & Location</div>
        <div className="panel-body">
          <p className="hint small" style={{ marginBottom: 12 }}>
            Collect network interfaces, WiFi connections, and location data from the device.
          </p>
          <NetworkLocationPanel serial={serial} />
          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("browser")}>← Back to Browser</button>
            <button className="mini" onClick={() => onGoTo("recover")}>Continue to Recover →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
