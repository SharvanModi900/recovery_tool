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

  // Headless mode state
  const [headlessActive, setHeadlessActive] = useState(false);
  const [headlessBusy, setHeadlessBusy] = useState(false);

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
  // Throttle to avoid flickering — batch state updates and cap at ~12 fps
  const lastFrameRef = useRef(0);
  const [paused, setPaused] = useState(false);
  const pausedRef = useRef(false);

  // Keep ref in sync with state so the event listener can read it without re-subscribing
  useEffect(() => { pausedRef.current = paused; }, [paused]);

  useEffect(() => {
    if (!liveStream || !serial) return;
    const unlisten = listen<{ serial: string; path: string; frame: number; timestamp: number; isVideo: boolean; mode: string }>(
      "screen-frame",
      (event) => {
        if (pausedRef.current) return; // Paused — drop all frames
        if (event.payload.serial === serial) {
          const now = Date.now();
          // Throttle: skip frames that arrive within 80ms of the last accepted one (~12fps cap)
          if (now - lastFrameRef.current < 80) return;
          lastFrameRef.current = now;
          // Batch all state updates together to prevent cascading re-renders
          const p = event.payload;
          setShot(p.path);
          setShotTs(p.timestamp);
          setShotDark(false);
          setIsVideoFrame(p.isVideo || false);
        }
      }
    );
    return () => {
      unlisten.then((u) => u());
    };
  }, [liveStream, serial]);

  // Auto-refresh mirror + telemetry (throttled to avoid lag)
  const refreshTickRef = useRef(0);
  const [controlMode, setControlModeRaw] = useState(false);

  // Wrap setControlMode to auto-start/stop live stream
  const setControlMode = useCallback((v: boolean) => {
    setControlModeRaw(v);
    if (serial) {
      if (v) {
        // Auto-start live stream at 10fps when control is enabled
        api.startScreenStream(serial, 10, "normal").catch(() => {});
        setLiveStream(true);
      } else {
        // Stop stream when control is disabled
        api.stopScreenStream(serial).catch(() => {});
        setLiveStream(false);
      }
    }
  }, [serial]);

  // Headless mode toggle — screen off + live stream + control
  const toggleHeadless = useCallback(async (activate: boolean) => {
    if (!serial) return;
    setHeadlessBusy(true);
    try {
      if (activate) {
        // 1. Start backend headless (wake lock + screen off)
        await api.headlessStart(serial);
        // 2. Start live stream in fast mode (screenrecord works with display off)
        await api.startScreenStream(serial, 15, "fast");
        setLiveStream(true);
        setStreamFps(15);
        setStreamMode("fast");
        // 3. Enable control mode
        setControlModeRaw(true);
        setHeadlessActive(true);
        setStatus("Headless mode ON — phone screen off, controlling from desktop");
      } else {
        // 1. Stop headless backend (wake screen, release lock)
        await api.headlessStop(serial);
        // 2. Stop stream
        await api.stopScreenStream(serial);
        setLiveStream(false);
        // 3. Disable control mode
        setControlModeRaw(false);
        setHeadlessActive(false);
        setStatus("Headless mode OFF — phone screen restored");
      }
    } catch (e) {
      setError(`Headless mode failed: ${e}`);
    } finally {
      setHeadlessBusy(false);
    }
  }, [serial, setStatus, setError]);

  // Normal auto-refresh when NOT in control/stream mode
  useEffect(() => {
    if (!serial || !autoRefresh || (step !== "connect" && step !== "apps")) return;
    if (controlMode) return; // Don't do periodic refresh when streaming
    const t = setInterval(() => {
      refreshTickRef.current++;
      if (refreshTickRef.current % 3 === 1) {
        capture(serial);
      } else {
        loadMirror(serial);
      }
    }, 5000);
    return () => clearInterval(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [serial, autoRefresh, step, controlMode]);

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
    controlMode, setControlMode,
    mirrorBusy, liveStream, streamFps, setStreamFps, streamMode, setStreamMode, isVideoFrame,
    paused, setPaused,
    headlessActive, headlessBusy, toggleHeadless,
    captures, autoCaptureActive, captureBusy, captureInterval, setCaptureInterval,
    loadMirror, capture, startStream, stopStream, resetMirror,
    loadCaptures, takeScreenshot, startAutoCaptureHandler, stopAutoCaptureHandler, deleteCapture,
  };
}
