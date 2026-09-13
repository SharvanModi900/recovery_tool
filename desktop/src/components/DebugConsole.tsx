import { useState, useRef, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

interface DebugConsoleProps {
  serial: string;
  isOpen: boolean;
  onClose: () => void;
}

export function DebugConsole({ serial, isOpen, onClose }: DebugConsoleProps) {
  const [lines, setLines] = useState<{ text: string; type: "input" | "output" | "error" | "info" }[]>([
    { text: "RecoverKit Debug Console v1.0", type: "info" },
    { text: "Type ADB shell commands below. Press Enter to execute.", type: "info" },
    { text: "────────────────────────────────────────", type: "info" },
  ]);
  const [currentInput, setCurrentInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [isExecuting, setIsExecuting] = useState(false);
  const terminalRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Throttled scroll - only scroll on new input, not on every line change
  useEffect(() => {
    if (terminalRef.current) {
      terminalRef.current.scrollTop = terminalRef.current.scrollHeight;
    }
  }, [currentInput]); // Only scroll when input changes, not on every line

  useEffect(() => {
    if (isOpen) {
      inputRef.current?.focus();
    }
  }, [isOpen]);

  const addLine = useCallback((text: string, type: "input" | "output" | "error" | "info" = "output") => {
    setLines(prev => {
      // Cap at 500 lines to prevent memory leak
      const next = [...prev, { text, type }];
      return next.length > 500 ? next.slice(-500) : next;
    });
  }, []);

  const executeCommand = async (cmd: string) => {
    const trimmed = cmd.trim();
    if (!trimmed) return;

    // Add to history
    setHistory(prev => [trimmed, ...prev].slice(0, 50));
    setHistoryIndex(-1);

    // Show the command in terminal
    addLine(`C:\\RecoverKit> ${trimmed}`, "input");

    if (!serial) {
      addLine("Error: No device connected", "error");
      return;
    }

    // Handle built-in commands
    if (trimmed === "clear" || trimmed === "cls") {
      setLines([]);
      return;
    }
    if (trimmed === "help") {
      addLine("Available commands:", "info");
      addLine("  Any ADB shell command (ls, getprop, cat, dumpsys, etc.)", "info");
      addLine("  clear/cls  - Clear terminal", "info");
      addLine("  help       - Show this help", "info");
      addLine("  history    - Show command history", "info");
      return;
    }
    if (trimmed === "history") {
      history.forEach((h, i) => addLine(`  ${i + 1}. ${h}`, "info"));
      return;
    }

    setIsExecuting(true);
    try {
      const result = await invoke<string>("adb_shell_command", {
        serial,
        command: trimmed
      });
      if (result && result.trim()) {
        result.split("\n").forEach(line => addLine(line, "output"));
      }
    } catch (err) {
      addLine(`Error: ${err}`, "error");
    } finally {
      setIsExecuting(false);
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      e.preventDefault();
      const cmd = currentInput;
      setCurrentInput("");
      executeCommand(cmd);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (history.length > 0) {
        const newIndex = Math.min(historyIndex + 1, history.length - 1);
        setHistoryIndex(newIndex);
        setCurrentInput(history[newIndex]);
      }
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      if (historyIndex > 0) {
        const newIndex = historyIndex - 1;
        setHistoryIndex(newIndex);
        setCurrentInput(history[newIndex]);
      } else {
        setHistoryIndex(-1);
        setCurrentInput("");
      }
    } else if (e.key === "Tab") {
      e.preventDefault();
      // Simple tab completion hint
      addLine("  (tab completion not available - type full command)", "info");
    }
  };

  const handleTerminalClick = () => {
    inputRef.current?.focus();
  };

  if (!isOpen) return null;

  return (
    <div
      style={{
        position: "fixed",
        bottom: 0,
        left: 0,
        right: 0,
        height: "350px",
        background: "#0c0c0c",
        borderTop: "2px solid #333",
        display: "flex",
        flexDirection: "column",
        zIndex: 9999,
        fontFamily: "Consolas, 'Courier New', monospace",
      }}
    >
      {/* Title bar */}
      <div
        style={{
          padding: "4px 12px",
          background: "#1f1f1f",
          borderBottom: "1px solid #333",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          userSelect: "none",
        }}
      >
        <span style={{ color: "#ccc", fontSize: "12px" }}>
          🐛 RecoverKit Debug Console — {serial ? serial.substring(0, 20) : "No device"}
        </span>
        <div style={{ display: "flex", gap: "8px" }}>
          <button
            onClick={() => { setLines([]); }}
            style={{
              background: "transparent",
              color: "#888",
              border: "1px solid #444",
              padding: "2px 8px",
              cursor: "pointer",
              fontSize: "11px",
              fontFamily: "inherit",
            }}
          >
            Clear
          </button>
          <button
            onClick={onClose}
            style={{
              background: "transparent",
              color: "#e74c3c",
              border: "1px solid #444",
              padding: "2px 8px",
              cursor: "pointer",
              fontSize: "11px",
              fontFamily: "inherit",
            }}
          >
            ✕
          </button>
        </div>
      </div>

      {/* Terminal area */}
      <div
        ref={terminalRef}
        onClick={handleTerminalClick}
        style={{
          flex: 1,
          overflow: "auto",
          padding: "8px 12px",
          cursor: "text",
        }}
      >
        {lines.map((line, i) => (
          <div
            key={i}
            style={{
              color: line.type === "error" ? "#e74c3c" :
                     line.type === "input" ? "#2ecc71" :
                     line.type === "info" ? "#f39c12" :
                     "#cccccc",
              fontSize: "13px",
              lineHeight: "1.4",
              whiteSpace: "pre-wrap",
              wordBreak: "break-all",
            }}
          >
            {line.text}
          </div>
        ))}

        {/* Current input line with prompt */}
        <div style={{ display: "flex", alignItems: "center", color: "#2ecc71", fontSize: "13px" }}>
          <span style={{ whiteSpace: "pre" }}>C:\RecoverKit&gt; </span>
          <input
            ref={inputRef}
            type="text"
            value={currentInput}
            onChange={(e) => setCurrentInput(e.target.value)}
            onKeyDown={handleKeyDown}
            disabled={isExecuting}
            spellCheck={false}
            autoComplete="off"
            style={{
              flex: 1,
              background: "transparent",
              color: "#2ecc71",
              border: "none",
              outline: "none",
              fontFamily: "inherit",
              fontSize: "inherit",
              padding: 0,
              margin: 0,
              caretColor: "#2ecc71",
            }}
          />
        </div>
      </div>
    </div>
  );
}
