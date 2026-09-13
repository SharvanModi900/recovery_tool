import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "../api";
import type { DeviceMirror, ScreenCapture } from "../types";

interface UseDeviceMirrorArgs {
  serial: string;
  step: string;
  setStatus: (s: string) => void;
  setError: (s: string) => void;
}

export function useDeviceMirror({ serial, step, setStatus, setError }: UseDeviceMirrorArgs) {
  const [mirror, setMirror] = useState<DeviceMirror | null>(null);
  const [shot, setShot] = useState("");
  const [shotTs, setShotTs] = useState(0);
  const [shotDark, setShotDark] = useState(false);
  const [mirrorErr, setMirrorErr] = useState("");
  const [autoRefresh, setAutoRefresh] = useState(true);
  const [mirrorBusy, setMirrorBusy] = useState(false);
  const [liveStream, setLiveStream] = useState(false);
  const [streamFps, setStreamFps] = useState(15);
  const [streamMode, setStreamMode] = useState("normal");
  const [isVideoFrame, setIsVideoFrame] = useState(false);

  // Screen capture state
  const [captures, setCaptures] = useState<ScreenCapture[]>([]);
  const [autoCaptureActive, setAutoCaptureActive] = useState(false);
  const [captureBusy, setCaptureBusy] = useState(false);
  const [captureInterval, setCaptureInterval] = useState(10);

  const loadMirror = (s: string) => {
    api.deviceMirror(s).then((m) => { setMirror(m); setMirrorErr(""); }).catch(() => setMirror(null));
  };

  const capture = async (s: string) => {
    setMirrorBusy(true);
    try {
      const r = await api.deviceScreenshot(s);
      setShot(r.path);
      setShotDark(r.dark);
      setShotTs(Date.now());
      setMirrorErr("");
    } catch (e) {
      setMirrorErr(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    } finally {
      setMirrorBusy(false);
    }
  };

  const startStream = async (s: string) => {
    try {
      await api.startScreenStream(s, streamFps, streamMode);
      setLiveStream(true);
    } catch (e) {
      setMirrorErr(typeof e === "string" ? e : (e as Error)?.message ?? String(e));
    }
  };

  const stopStream = async (s: string) => {
    try {
      await api.stopScreenStream(s);
      setLiveStream(false);
    } catch (e) {
      console.error("Failed to stop stream:", e);
    }
  };

  const resetMirror = () => {
    setMirror(null);
    setShot("");
    setShotTs(0);
    setShotDark(false);
    setMirrorErr("");
  };

  // Listen for screen-frame events when live stream is active
  useEffect(() => {
    if (!liveStream || !serial) return;
    const unlisten = listen<{ serial: string; path: string; frame: number; timestamp: number; isVideo: boolean; mode: string }>(
      "screen-frame",
      (event) => {
        if (event.payload.serial === serial) {
          setShot(event.payload.path);
          setShotTs(event.payload.timestamp);
          setShotDark(false);
          setIsVideoFrame(event.payload.isVideo || false);
        }
      }
    );
    return () => {
      unlisten.then((u) => u());
    };
  }, [liveStream, serial]);

  // Auto-refresh mirror + telemetry (throttled to avoid lag)
  const refreshTickRef = useRef(0);
  useEffect(() => {
    if (!serial || !autoRefresh || step !== "connect") return;
    const t = setInterval(() => {
      // Only refresh every 3rd tick (~15s) to avoid constant ADB spam
      refreshTickRef.current++;
      if (refreshTickRef.current % 3 === 1) {
        capture(serial);
      } else {
        loadMirror(serial);
      }
    }, 5000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, autoRefresh, step]);

  // Screen capture handlers
  const loadCaptures = useCallback(async () => {
    try {
      const caps = await api.getScreenCaptures(serial || undefined, 100);
      setCaptures(caps);
    } catch (e) {
      console.error("Failed to load captures:", e);
    }
  }, [serial]);

  const takeScreenshot = useCallback(async () => {
    if (!serial || captureBusy) return;
    setCaptureBusy(true);
    try {
      await api.takeScreenshotCapture(serial);
      await loadCaptures();
      setStatus("Screenshot captured and saved");
    } catch (e) {
      setError(`Screenshot failed: ${e}`);
    } finally {
      setCaptureBusy(false);
    }
  }, [serial, captureBusy, loadCaptures, setStatus, setError]);

  const startAutoCaptureHandler = useCallback(async () => {
    if (!serial || autoCaptureActive) return;
    try {
      await api.startAutoCapture(serial, captureInterval);
      setAutoCaptureActive(true);
      setStatus(`Auto-capture started (every ${captureInterval}s)`);
      await loadCaptures();
    } catch (e) {
      setError(`Auto-capture failed: ${e}`);
    }
  }, [serial, autoCaptureActive, captureInterval, loadCaptures, setStatus, setError]);

  const stopAutoCaptureHandler = useCallback(async () => {
    try {
      await api.stopAutoCapture();
      setAutoCaptureActive(false);
      setStatus("Auto-capture stopped");
    } catch (e) {
      setError(`Stop failed: ${e}`);
    }
  }, [setStatus, setError]);

  const deleteCapture = useCallback(async (id: number) => {
    try {
      await api.deleteScreenCapture(id);
      await loadCaptures();
    } catch (e) {
      setError(`Delete failed: ${e}`);
    }
  }, [loadCaptures, setError]);

  // Listen for screen capture events from auto-capture
  useEffect(() => {
    const unlisten = listen<ScreenCapture>("screen-capture-event", (event) => {
      setCaptures(prev => [event.payload, ...prev].slice(0, 100));
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  return {
    mirror, shot, shotTs, shotDark, mirrorErr,
    autoRefresh, setAutoRefresh,
    mirrorBusy, liveStream, streamFps, setStreamFps, streamMode, setStreamMode, isVideoFrame,
    captures, autoCaptureActive, captureBusy, captureInterval, setCaptureInterval,
    loadMirror, capture, startStream, stopStream, resetMirror,
    loadCaptures, takeScreenshot, startAutoCaptureHandler, stopAutoCaptureHandler, deleteCapture,
  };
}
