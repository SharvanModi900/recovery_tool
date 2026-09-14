import { DeletedDataPanel } from "./DeletedDataPanel";

type Step = "connect" | "apps" | "system" | "scan" | "media" | "messages" | "calls" | "contacts" | "sim_forensics" | "artifacts" | "deep" | "deleted" | "browser" | "network" | "recover" | "export" | "case" | "timeline";

interface DeletedDataStepProps {
  serial: string;
  onGoTo: (step: Step) => void;
}

export function DeletedDataStep({ serial, onGoTo }: DeletedDataStepProps) {
  return (
    <section style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0 }}>
      <div className="panel">
        <div className="panel-bar">🗑️ Deleted Data Recovery</div>
        <div className="panel-body">
          <p className="hint small" style={{ marginBottom: 12 }}>
            Recover deleted SMS messages, thumbnails, and carve data from device databases.
          </p>
          <DeletedDataPanel serial={serial} />
          <div style={{ marginTop: 16, display: "flex", gap: 8 }}>
            <button className="mini" onClick={() => onGoTo("deep")}>← Back to Deep Forensics</button>
            <button className="mini" onClick={() => onGoTo("browser")}>Continue to Browser →</button>
          </div>
        </div>
      </div>
    </section>
  );
}
