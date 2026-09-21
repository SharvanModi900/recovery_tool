import type { ReactNode } from "react";

export interface ToolResult {
  id: string;
  ok: boolean;
  text: string;
  ts: number;
}

export interface DeviceToolsTabProps {
  serial: string | null;
  results: ToolResult[];
  busy: Set<string>;
  run: (id: string, fn: () => Promise<string>) => Promise<void>;
  addResult: (id: string, ok: boolean, text: string) => void;
}

export const OUT = "D:\\recovery-tool\\workspace";

export const inputStyle = (w: any): React.CSSProperties => ({
  width: w, padding: "3px 6px", fontSize: 10.5, borderRadius: 4,
  border: "1px solid var(--border, #444)", background: "var(--panel-alt, #111)", color: "var(--text, #eee)",
});

export const miniBtn: React.CSSProperties = {
  padding: "3px 8px", fontSize: 10.5, borderRadius: 4,
  border: "1px solid var(--border, #444)", background: "var(--panel-alt, #1a1a2e)", color: "var(--text, #eee)", cursor: "pointer",
};

export const toolBtn = (isBusy: boolean, disabled?: boolean): React.CSSProperties => ({
  padding: "4px 10px", fontSize: 11, borderRadius: 6,
  border: "1px solid var(--border, #444)",
  background: isBusy ? "var(--info, #3b82f6)" : "var(--panel-alt, #1a1a2e)",
  color: "var(--text, #eee)", cursor: disabled ? "default" : "pointer",
  opacity: disabled ? 0.4 : 1, whiteSpace: "nowrap",
});

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)}KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)}MB`;
}

export interface FileEntry {
  type: string;
  size: string;
  name: string;
  perms: string;
}

export type { ReactNode };
