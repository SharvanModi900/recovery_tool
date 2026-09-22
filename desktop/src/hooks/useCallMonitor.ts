import { useState, useCallback, useMemo, useRef, useEffect } from "react";
import * as api from "../api";
import type { CallEvent, CallRecording } from "../types";

// Format a live call event into a short banner string for the monitor.
function liveBanner(e: CallEvent): string {
  const who = e.contact_name || e.phone_number || "";
  switch (e.event_type) {
    case "Incoming": return `📞 INCOMING CALL${who ? " from " + who : ""}`;
    case "Outgoing": return `📱 DIALING${who ? " to " + who : ""}`;
    case "Active": return `🟢 CALL ACTIVE${who ? " with " + who : ""}`;
    case "Missed": return `❌ MISSED CALL${who ? " from " + who : ""}`;
    case "Ended": return `📴 CALL ENDED${who ? " with " + who : ""}${e.duration ? ` (${e.duration}s)` : ""}`;
    case "Rejected": return `✋ REJECTED${who ? " " + who : ""}`;
    default: return "";
  }
}

interface UseCallMonitorArgs {
  serial: string;
  setStatus: (s: string) => void;
  setError: (s: string) => void;
}

export function useCallMonitor({ serial, setStatus, setError }: UseCallMonitorArgs) {
  const [callMonitorActive, setCallMonitorActive] = useState(false);
  const [callEvents, setCallEvents] = useState<CallEvent[]>([]);
  const [currentCallNotification, setCurrentCallNotification] = useState<string | null>(null);
  const callMonitorRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [callRecordings, setCallRecordings] = useState<CallRecording[]>([]);
  const [playingRec, setPlayingRec] = useState<{ path: string; localPath: string; filename: string } | null>(null);
  const lastLiveSigRef = useRef<string | null>(null);
  const lastEndedTsRef = useRef<number>(0);

  // Play bell sound notification
  const playBellSound = useCallback(() => {
    try {
      const audioContext = new (window.AudioContext || (window as any).webkitAudioContext)();
      const oscillator = audioContext.createOscillator();
      const gainNode = audioContext.createGain();
      oscillator.connect(gainNode);
      gainNode.connect(audioContext.destination);
      oscillator.frequency.setValueAtTime(800, audioContext.currentTime);
      oscillator.frequency.exponentialRampToValueAtTime(400, audioContext.currentTime + 0.3);
      oscillator.type = 'sine';
      gainNode.gain.setValueAtTime(0.3, audioContext.currentTime);
      gainNode.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.5);
      oscillator.start(audioContext.currentTime);
      oscillator.stop(audioContext.currentTime + 0.5);
      const osc2 = audioContext.createOscillator();
      const gain2 = audioContext.createGain();
      osc2.connect(gain2);
      gain2.connect(audioContext.destination);
      osc2.frequency.setValueAtTime(1200, audioContext.currentTime);
      osc2.frequency.exponentialRampToValueAtTime(600, audioContext.currentTime + 0.2);
      osc2.type = 'sine';
      gain2.gain.setValueAtTime(0.2, audioContext.currentTime);
      gain2.gain.exponentialRampToValueAtTime(0.01, audioContext.currentTime + 0.3);
      osc2.start(audioContext.currentTime);
      osc2.stop(audioContext.currentTime + 0.3);
    } catch (e) {
      console.log('Bell sound not available:', e);
    }
  }, []);

  // Load call recordings from the device (each matched to its call-log entry).
  const loadCallRecordings = useCallback(async () => {
    if (!serial) return;
    try {
      const recs = await api.getCallRecordings(serial);
      setCallRecordings(recs);
    } catch (e) {
      console.error("Failed to load call recordings:", e);
    }
  }, [serial]);

  // Pull a recording to local disk and play it inline in the timeline.
  const playRecording = useCallback(async (rec: CallRecording) => {
    if (!serial) return;
    try {
      setStatus("Pulling recording from device…");
      const localPath = await api.pullRecording(serial, rec.path);
      setPlayingRec({ path: rec.path, localPath, filename: rec.filename });
    } catch (e) {
      setError(`Playback failed: ${e}`);
    }
  }, [serial, setStatus, setError]);

  // Merge live call events + recording files into one chronological timeline.
  const timeline = useMemo(() => {
    const items: { kind: "event" | "recording"; ts: number; event?: CallEvent; rec?: CallRecording }[] = [];
    callEvents.forEach((e) => items.push({ kind: "event", ts: e.timestamp, event: e }));
    callRecordings.forEach((r) => { if (r.timestamp > 0) items.push({ kind: "recording", ts: r.timestamp, rec: r }); });
    items.sort((a, b) => b.ts - a.ts);
    return items;
  }, [callEvents, callRecordings]);

  // Call monitoring functions
  const startCallMonitor = useCallback(() => {
    if (!serial || callMonitorRef.current) return;
    setCallMonitorActive(true);
    setCallEvents([]);
    setCurrentCallNotification(null);
    lastLiveSigRef.current = null;
    lastEndedTsRef.current = 0;
    loadCallRecordings();

    callMonitorRef.current = setInterval(async () => {
      try {
        const event = await api.pollLiveCall(serial);
        if (!event) {
          lastLiveSigRef.current = null;
          setCurrentCallNotification(null);
          return;
        }
        const isLive = event.event_type === "Incoming" || event.event_type === "Outgoing" || event.event_type === "Active";
        setCurrentCallNotification(liveBanner(event));

        if (isLive) {
          const sig = `${event.event_type}|${event.phone_number || ""}`;
          if (lastLiveSigRef.current === sig) return;
          lastLiveSigRef.current = sig;
          if (event.event_type === "Incoming" || event.event_type === "Outgoing") playBellSound();
          setCallEvents((prev) => [...prev, event].slice(-100));
          return;
        }

        if ((event.event_type === "Ended" || event.event_type === "Missed") && lastEndedTsRef.current !== event.timestamp) {
          lastEndedTsRef.current = event.timestamp;
          if (event.event_type === "Missed") playBellSound();
          loadCallRecordings();
        }
        setCallEvents((prev) => {
          const dup = prev.some((e) => e.event_type === event.event_type && Math.abs(e.timestamp - event.timestamp) < 5000);
          if (dup) return prev;
          return [...prev, event].slice(-100);
        });
      } catch (e) {
        // Silently ignore transient errors during monitoring.
      }
    }, 8000); // Changed from 2000ms to 8000ms to reduce ADB spam
  }, [serial, loadCallRecordings, playBellSound]);

  const stopCallMonitor = useCallback(() => {
    if (callMonitorRef.current) {
      clearInterval(callMonitorRef.current);
      callMonitorRef.current = null;
    }
    setCallMonitorActive(false);
    setCurrentCallNotification(null);
  }, []);

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      if (callMonitorRef.current) {
        clearInterval(callMonitorRef.current);
      }
    };
  }, []);

  // Auto-start call monitor when device is connected
  useEffect(() => {
    if (serial && !callMonitorActive && !callMonitorRef.current) {
      const timer = setTimeout(() => {
        startCallMonitor();
      }, 1000);
      return () => clearTimeout(timer);
    }
  }, [serial]); // eslint-disable-line react-hooks/exhaustive-deps

  return {
    callMonitorActive, callEvents, setCallEvents, currentCallNotification,
    callRecordings, playingRec, timeline,
    startCallMonitor, stopCallMonitor, loadCallRecordings,
    playRecording, setPlayingRec, playBellSound,
  };
}
