import type { Step } from "../types";
import { DeviceToolsPanel } from "./DeviceToolsPanel";
import { StepNav } from "./DataStepLayout";

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
          <StepNav buttons={[
            { label: "← Forensics Suite", target: "forensics" },
            { label: "← Back to Connect", target: "connect" },
            { label: "Continue to Recovery →", target: "recover" },
          ]} onGoTo={onGoTo} />
        </div>
      </div>
    </section>
  );
}
