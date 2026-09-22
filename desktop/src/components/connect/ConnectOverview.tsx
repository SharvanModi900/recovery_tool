import type { ReactNode } from "react";
import type { DeviceMirror, DirectAcquisition } from "../../types";
import type { Activity } from "../../hooks/useActivityLog";
import { OverviewTab } from "../OverviewTab";

interface ConnectOverviewProps {
  serial: string;
  mirror: DeviceMirror | null;
  directData: DirectAcquisition | null;
  screenMirror: ReactNode;
  onGoTo?: (step: string) => void;
  activities: Activity[];
}

export function ConnectOverview({ serial, mirror, directData, screenMirror, onGoTo, activities }: ConnectOverviewProps) {
  return (
    <div style={{ gridArea: "main", gridColumn: "1 / -1" }}>
      <OverviewTab
        serial={serial}
        mirror={mirror}
        directData={directData}
        screenMirror={screenMirror}
        onGoTo={onGoTo}
        activities={activities}
      />
    </div>
  );
}
