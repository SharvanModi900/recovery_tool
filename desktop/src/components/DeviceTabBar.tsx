import type { DeviceSession } from "../App";

interface DeviceTabBarProps {
  sessions: Map<string, DeviceSession>;
  activeSerial: string;
  onSelect: (serial: string) => void;
  onClose: (serial: string) => void;
}

export function DeviceTabBar({ sessions, activeSerial, onSelect, onClose }: DeviceTabBarProps) {
  if (sessions.size === 0) return null;

  return (
    <div className="device-tabs">
      {Array.from(sessions.values()).map((session) => (
        <button
          key={session.serial}
          className={`device-tab ${session.serial === activeSerial ? "active" : ""}`}
          onClick={() => onSelect(session.serial)}
        >
          📱 {session.model || session.serial.slice(0, 12)}
          {session.liveStream && <span className="live-dot">●</span>}
          {session.report && <span className="data-dot">📊</span>}
          <span
            className="close-tab"
            onClick={(e) => { e.stopPropagation(); onClose(session.serial); }}
          >
            ×
          </span>
        </button>
      ))}
    </div>
  );
}
