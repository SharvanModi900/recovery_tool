import type {
  Device, DeviceMirror, DirectAcquisition, WhatsAppDeepResult, RootCheckResult,
  CallEvent, CallRecording, ScreenCapture,
} from "../types";
import { ScreenMirrorPanel } from "./ScreenMirrorPanel";
import { ConnectOverview } from "./connect/ConnectOverview";
import { ConnectDataTab } from "./connect/ConnectDataTab";
import { ConnectWhatsAppTab } from "./connect/ConnectWhatsAppTab";
import { ConnectCallsTab } from "./connect/ConnectCallsTab";
import { ConnectRemoteTab } from "./connect/ConnectRemoteTab";

interface ConnectStepProps {
  serial: string;
  busy: boolean;
  devices: Device[];
  connectTab: "overview" | "data" | "whatsapp" | "calls" | "remote";
  setConnectTab: (t: "overview" | "data" | "whatsapp" | "calls" | "remote") => void;
  // System data
  directData: DirectAcquisition | null;
  directBusy: boolean;
  onAcquire: () => void;
  onViewFull: () => void;
  // Camera
  cameraBusy: boolean;
  cameraStatus: string;
  onCapturePhoto: () => void;
  // WhatsApp
  waDeepResult: WhatsAppDeepResult | null;
  waDeepBusy: boolean;
  rootCheck: RootCheckResult | null;
  rootBusy: boolean;
  onWhatsappDeep: () => void;
  onCheckRoot: () => void;
  onWhatsappDowngrade: () => void;
  onImportTxt: () => void;
  onProvideKey: () => void;
  // Call monitor
  callMonitorActive: boolean;
  callEvents: CallEvent[];
  currentCallNotification: string | null;
  callRecordings: CallRecording[];
  playingRec: { path: string; localPath: string; filename: string } | null;
  timeline: { kind: "event" | "recording"; ts: number; event?: CallEvent; rec?: CallRecording }[];
  onStartMonitor: () => void;
  onStopMonitor: () => void;
  onLoadRecordings: () => void;
  onPlayRecording: (rec: CallRecording) => void;
  onStopPlaying: () => void;
  // Screen mirror
  mirror: DeviceMirror | null;
  shot: string;
  shotTs: number;
  shotDark: boolean;
  mirrorBusy: boolean;
  mirrorErr: string;
  isVideoFrame: boolean;
  liveStream: boolean;
  streamFps: number;
  streamMode: string;
  autoRefresh: boolean;
  captures: ScreenCapture[];
  captureBusy: boolean;
  autoCaptureActive: boolean;
  captureInterval: number;
  onCapture: (s: string) => void;
  onStartStream: (s: string) => void;
  onStopStream: (s: string) => void;
  onTakeScreenshot: () => void;
  onStartAuto: () => void;
  onStopAuto: () => void;
  onIntervalChange: (n: number) => void;
  onRefreshCaptures: () => void;
  onDeleteCapture: (id: number) => void;
  onAutoRefreshChange: (v: boolean) => void;
  onStreamFpsChange: (v: number) => void;
  onStreamModeChange: (v: string) => void;
  controlMode: boolean;
  onControlModeChange: (v: boolean) => void;
  headlessActive: boolean;
  headlessBusy: boolean;
  onToggleHeadless: (v: boolean) => void;
  paused: boolean;
  onPausedChange: (v: boolean) => void;
  // Device actions (status strip)
  onRingDevice: () => void;
  onLockDevice: () => void;
  onToggleCharging: () => void;
  chargingStopped: boolean;
  // Navigation
  onGoTo: (step: string) => void;
}

export function ConnectStep(props: ConnectStepProps) {
  const {
    serial, busy, devices, connectTab, setConnectTab,
    mirror, directData, directBusy, onAcquire, onViewFull,
    cameraBusy, cameraStatus, onCapturePhoto,
    waDeepResult, waDeepBusy, rootCheck, rootBusy, onWhatsappDeep, onCheckRoot, onWhatsappDowngrade, onImportTxt, onProvideKey,
    callMonitorActive, callEvents, currentCallNotification, callRecordings, playingRec, timeline,
    onStartMonitor, onStopMonitor, onLoadRecordings, onPlayRecording, onStopPlaying,
    shot, shotTs, shotDark, mirrorBusy, mirrorErr, isVideoFrame, liveStream, streamFps, streamMode,
    autoRefresh, captures, captureBusy, autoCaptureActive, captureInterval,
    onCapture, onStartStream, onStopStream, onTakeScreenshot, onStartAuto, onStopAuto,
    onIntervalChange, onRefreshCaptures, onDeleteCapture, onAutoRefreshChange, onStreamFpsChange, onStreamModeChange,
    controlMode, onControlModeChange,
    headlessActive, headlessBusy, onToggleHeadless,
    paused, onPausedChange,
    onRingDevice, onLockDevice, onToggleCharging, chargingStopped,
    onGoTo,
  } = props;

  // Build ScreenMirrorPanel once — shared across tabs
  const screenMirrorNode = (
    <ScreenMirrorPanel
      serial={serial}
      shot={shot} shotTs={shotTs} shotDark={shotDark}
      mirrorBusy={mirrorBusy} mirrorErr={mirrorErr} isVideoFrame={isVideoFrame}
      liveStream={liveStream} streamFps={streamFps} streamMode={streamMode}
      autoRefresh={autoRefresh}
      mirror={mirror}
      captures={captures} captureBusy={captureBusy}
      autoCaptureActive={autoCaptureActive} captureInterval={captureInterval}
      onCapture={onCapture}
      onStartStream={onStartStream}
      onStopStream={onStopStream}
      onTakeScreenshot={onTakeScreenshot}
      onStartAuto={onStartAuto}
      onStopAuto={onStopAuto}
      onIntervalChange={onIntervalChange}
      onRefreshCaptures={onRefreshCaptures}
      onDeleteCapture={onDeleteCapture}
      onAutoRefreshChange={onAutoRefreshChange}
      onStreamFpsChange={onStreamFpsChange}
      onStreamModeChange={onStreamModeChange}
      controlMode={controlMode}
      onControlModeChange={onControlModeChange}
      headlessActive={headlessActive}
      headlessBusy={headlessBusy}
      onToggleHeadless={onToggleHeadless}
      paused={paused}
      onPausedChange={onPausedChange}
    />
  );

  return (
    <section className="workbench wb-connect">
      {/* TOP — status strip */}
      <div className="panel timeline" style={{ gridArea: "strip" }}>
        <div className="panel-body tl">
          <span className="hint nowrap">
            {devices.length === 0
              ? "No device detected — enable USB debugging, plug in via USB-C, tap Allow, then hit ⟳ in the header."
              : `${devices.length} device${devices.length === 1 ? "" : "s"} · ${serial || "none selected"} · read-only`}
          </span>
          {devices.some((d) => d.serial === serial && d.state === "device") && (
            <>
              <button className="primary next" onClick={() => onGoTo("scan")}>
                Continue to Scan →
              </button>
              <div className="quick-actions-inline">
                <button className="qa-btn" onClick={onViewFull} title="View Full Details">
                   Full Details
                </button>
                <button className="qa-btn primary" onClick={onAcquire} disabled={directBusy} title="Acquire Data">
                   Acquire Data
                </button>
                <button className="qa-btn" onClick={onLockDevice} title="Lock Device">
                   Lock
                </button>
                <button className="qa-btn" onClick={onRingDevice} title="Ring Device">
                   Ring
                </button>
                <button className={`qa-btn ${chargingStopped ? "warning" : ""}`} onClick={onToggleCharging} title={chargingStopped ? "Resume Charging" : "Stop Charging"}>
                  {chargingStopped ? "" : ""} {chargingStopped ? "Start Charge" : "Stop Charge"}
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* TAB BAR */}
      <div className="wb-connect-tabs">
        <button className={connectTab === "overview" ? "active" : ""} onClick={() => setConnectTab("overview")}>
          📋 Overview
        </button>
        <button className={connectTab === "data" ? "active" : ""} onClick={() => setConnectTab("data")}>
          📞 System Data
          {directData && <span className="tab-badge">{directData.call_logs.length + directData.sms_messages.length + directData.contacts.length}</span>}
        </button>
        <button className={connectTab === "whatsapp" ? "active" : ""} onClick={() => setConnectTab("whatsapp")}>
          🔐 WhatsApp
          {waDeepResult && <span className="tab-badge">{waDeepResult.total_messages || "✓"}</span>}
        </button>
        <button className={connectTab === "calls" ? "active" : ""} onClick={() => setConnectTab("calls")}>
          📞 Calls
          {callMonitorActive && <span className="tab-badge" style={{ background: "var(--ok)", color: "#fff" }}>●</span>}
        </button>
        <button className={connectTab === "remote" ? "active" : ""} onClick={() => setConnectTab("remote")}>
          📡 Remote
        </button>
      </div>

      {/* Tab content */}
      {connectTab === "overview" && (
        <ConnectOverview
          serial={serial}
          mirror={mirror}
          directData={directData}
          screenMirror={screenMirrorNode}
          onGoTo={onGoTo}
        />
      )}
      {connectTab === "data" && (
        <ConnectDataTab
          serial={serial} busy={busy}
          directData={directData} directBusy={directBusy}
          onAcquire={onAcquire} onViewFull={onViewFull}
          cameraBusy={cameraBusy} cameraStatus={cameraStatus} onCapturePhoto={onCapturePhoto}
        />
      )}
      {connectTab === "whatsapp" && (
        <ConnectWhatsAppTab
          serial={serial} busy={busy}
          waDeepResult={waDeepResult} waDeepBusy={waDeepBusy}
          rootCheck={rootCheck} rootBusy={rootBusy}
          onWhatsappDeep={onWhatsappDeep} onCheckRoot={onCheckRoot}
          onWhatsappDowngrade={onWhatsappDowngrade} onImportTxt={onImportTxt} onProvideKey={onProvideKey}
        />
      )}
      {connectTab === "calls" && (
        <ConnectCallsTab
          serial={serial}
          callMonitorActive={callMonitorActive} callEvents={callEvents}
          currentCallNotification={currentCallNotification} callRecordings={callRecordings}
          playingRec={playingRec} timeline={timeline}
          onStartMonitor={onStartMonitor} onStopMonitor={onStopMonitor}
          onLoadRecordings={onLoadRecordings} onPlayRecording={onPlayRecording} onStopPlaying={onStopPlaying}
        />
      )}
      {connectTab === "remote" && (
        <ConnectRemoteTab serial={serial} />
      )}

      {/* Screen mirror — shown alongside non-overview tabs */}
      {connectTab !== "overview" && screenMirrorNode}
    </section>
  );
}
