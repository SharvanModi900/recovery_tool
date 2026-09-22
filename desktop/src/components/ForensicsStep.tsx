import type { Step } from "../types";
import { ForensicsPanel } from "./ForensicsPanel";
import { StepNav } from "./DataStepLayout";

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
          <StepNav buttons={[
            { label: "← SIM Forensics", target: "sim_forensics" },
            { label: "Continue to Recovery →", target: "recover" },
          ]} onGoTo={onGoTo} />
        </div>
      </div>
    </section>
  );
}
