import type { ReactNode } from "react";
import type { DeviceMirror, DirectAcquisition } from "../../types";
import { OverviewTab } from "../OverviewTab";

interface ConnectOverviewProps {
  serial: string;
  mirror: DeviceMirror | null;
  directData: DirectAcquisition | null;
  screenMirror: ReactNode;
  onGoTo?: (step: string) => void;
}

export function ConnectOverview({ serial, mirror, directData, screenMirror, onGoTo }: ConnectOverviewProps) {
  return (
    <div style={{ gridArea: "main", gridColumn: "1 / -1" }}>
      <OverviewTab
        serial={serial}
        mirror={mirror}
        directData={directData}
        screenMirror={screenMirror}
        onGoTo={onGoTo}
      />
    </div>
  );
}
