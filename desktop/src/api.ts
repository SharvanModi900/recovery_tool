import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, Device, DeviceMirror, IconInfo, ScanReport, DecryptOutcome, MediaInspect, MediaRecovery, ScreenshotInfo, UiRecovery, DirectAcquisition } from "./types";

export const listDevices = () => invoke<Device[]>("list_devices");

export const scanFolder = (path: string) => invoke<ScanReport>("scan_folder", { path });

export const scanDevice = (serial: string, pull: boolean, includeMedia: boolean) =>
  invoke<[ScanReport, string[]]>("scan_device", { serial, pull, includeMedia });

export const decryptBackup = (input: string, keyFile: string, output?: string, serial?: string) =>
  invoke<DecryptOutcome>("decrypt_backup", { input, keyFile, output, serial });

export const fetchKey = (serial: string, remotePath?: string) =>
  invoke<string>("fetch_key", { serial, remotePath });

export const recoverDb = (dbPath: string, deep: boolean, serial?: string) =>
  invoke<UiRecovery>("recover_db", { dbPath, deep, serial });

export const importChatTxt = (path: string) =>
  invoke<UiRecovery>("import_chat_txt", { path });

export const importSmsXml = (path: string) =>
  invoke<UiRecovery>("import_sms_xml", { path });

export const importVcf = (path: string) =>
  invoke<UiRecovery>("import_vcf", { path });

export const recoverMedia = (serial: string, source?: string) =>
  invoke<MediaRecovery>("recover_media", { serial, source });

export const saveMedia = (serial: string, dest: string) =>
  invoke<string>("save_media", { serial, dest });

export const mediaDir = (serial: string) =>
  invoke<string>("media_dir", { serial });

export const inspectMedia = (path: string) => invoke<MediaInspect>("inspect_media", { path });

export const deviceMirror = (serial: string) => invoke<DeviceMirror>("device_mirror", { serial });

export const deviceScreenshot = (serial: string) => invoke<ScreenshotInfo>("device_screenshot", { serial });

export const startScreenStream = (serial: string, fps: number = 10, mode: string = "normal") =>
  invoke<void>("start_screen_stream", { serial, fps, mode });

export const stopScreenStream = (serial: string) =>
  invoke<void>("stop_screen_stream", { serial });

export const devicePresence = (serial: string) => invoke<string[]>("device_presence", { serial });

export const listApps = (serial: string) => invoke<AppInfo[]>("list_apps", { serial });

export const appDetails = (serial: string, pkg: string) =>
  invoke<AppInfo>("app_details", { serial, package: pkg });

export const appIcon = (serial: string, pkg: string) =>
  invoke<IconInfo>("app_icon", { serial, package: pkg });

export const directAcquisition = (serial: string) =>
  invoke<DirectAcquisition>("direct_acquisition", { serial });

export const exportResult = (format: string, dest: string) =>
  invoke<string>("export_result", { format, dest });
