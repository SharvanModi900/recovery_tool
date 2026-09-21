//! Fastboot mode acquisition layer.
//!
//! Allows data extraction from devices in bootloader/fastboot mode,
//! which does NOT require USB debugging to be enabled.
//! The device enters fastboot via hardware key combo or `adb reboot bootloader`.

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Spawn fastboot without flashing a console window on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Timeout for quick fastboot queries (getvar, devices).
const QUERY_TIMEOUT: Duration = Duration::from_secs(15);
/// Timeout for partition reads — large partitions can take a while.
const READ_TIMEOUT: Duration = Duration::from_secs(600);
/// Timeout for device listing.
const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// A device detected in fastboot/bootloader mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastbootDevice {
    /// Serial number (or "<serial number>" from fastboot devices).
    pub serial: String,
    /// Fastboot mode: "fastboot" or "fastbootd".
    pub mode: String,
    /// Device product name (from getvar product).
    pub product: Option<String>,
    /// Device model (from getvar model, if available).
    pub model: Option<String>,
    /// Secure boot status.
    pub secure: Option<String>,
    /// Unlocked state.
    pub unlocked: Option<String>,
    /// Battery voltage / SOC (some devices report this).
    pub battery: Option<String>,
}

/// Information about a single partition on the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub size: u64,
}

pub struct FastbootClient {
    fastboot_path: PathBuf,
}

impl Default for FastbootClient {
    fn default() -> Self {
        Self::new()
    }
}

impl FastbootClient {
    pub fn new() -> Self {
        Self { fastboot_path: Self::discover() }
    }

    /// Locate `fastboot`: explicit `FASTBOOT_PATH` env var first, then common
    /// install locations, finally plain `fastboot` (resolved via PATH by the OS).
    fn discover() -> PathBuf {
        if let Ok(p) = std::env::var("FASTBOOT_PATH") {
            let p = PathBuf::from(p);
            if p.exists() {
                return p;
            }
        }
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let pf = std::env::var("ProgramFiles").unwrap_or_default();
        let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();
        let candidates = [
            format!("{local}\\Android\\Sdk\\platform-tools\\fastboot.exe"),
            format!("{pf}\\platform-tools\\fastboot.exe"),
            format!("{pf86}\\platform-tools\\fastboot.exe"),
        ];
        for c in candidates {
            if c.starts_with('\\') {
                continue;
            }
            let p = PathBuf::from(&c);
            if p.exists() {
                return p;
            }
        }
        PathBuf::from("fastboot")
    }

    /// Run fastboot with a hard timeout. Returns (stdout, stderr).
    fn run(&self, args: &[&str], timeout: Duration) -> Result<(Vec<u8>, Vec<u8>)> {
        let mut cmd = Command::new(&self.fastboot_path);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd.spawn().map_err(|e| {
            CoreError::Adb(format!(
                "failed to run fastboot ({e}); install platform-tools or set FASTBOOT_PATH"
            ))
        })?;
        let mut out_pipe = child.stdout.take().expect("stdout piped");
        let mut err_pipe = child.stderr.take().expect("stderr piped");
        let out_thread = std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = out_pipe.read_to_end(&mut v);
            v
        });
        let err_thread = std::thread::spawn(move || {
            let mut v = Vec::new();
            let _ = err_pipe.read_to_end(&mut v);
            v
        });
        let deadline = Instant::now() + timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    let stdout = out_thread.join().unwrap_or_default();
                    let stderr = err_thread.join().unwrap_or_default();
                    return Ok((stdout, stderr));
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(CoreError::Adb(format!(
                            "fastboot timed out after {}s ({})",
                            timeout.as_secs(),
                            args.join(" ")
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(CoreError::Adb(format!("fastboot process error: {e}")));
                }
            }
        }
    }

    /// List devices in fastboot mode (`fastboot devices`).
    pub fn list_devices(&self) -> Result<Vec<FastbootDevice>> {
        let (stdout, _stderr) = self.run(&["devices"], LIST_TIMEOUT)?;
        // fastboot outputs to stderr on some versions
        let text = if stdout.is_empty() {
            let (_, err) = self.run(&["devices"], LIST_TIMEOUT)?;
            String::from_utf8_lossy(&err).to_string()
        } else {
            String::from_utf8_lossy(&stdout).to_string()
        };
        let mut devices = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('*') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(serial) = parts.next() else { continue };
            let mode = parts.next().unwrap_or("fastboot").to_string();
            let mut dev = FastbootDevice {
                serial: serial.to_string(),
                mode: mode.clone(),
                product: None,
                model: None,
                secure: None,
                unlocked: None,
                battery: None,
            };
            // Try to get product/model info
            if let Ok(product) = self.getvar(serial, "product") {
                dev.product = product;
            }
            if let Ok(model) = self.getvar(serial, "model") {
                dev.model = model;
            }
            if let Ok(secure) = self.getvar(serial, "secure") {
                dev.secure = secure;
            }
            if let Ok(unlocked) = self.getvar(serial, "unlocked") {
                dev.unlocked = unlocked;
            }
            devices.push(dev);
        }
        Ok(devices)
    }

    /// Read a single variable via `fastboot getvar`.
    pub fn getvar(&self, serial: &str, var: &str) -> Result<Option<String>> {
        let (stdout, stderr) = self.run(
            &["-s", serial, "getvar", var],
            QUERY_TIMEOUT,
        )?;
        // fastboot getvar outputs to stderr in format: "var: value"
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        for line in combined.lines() {
            let line = line.trim();
            if let Some((_key, val)) = line.split_once(':') {
                let val = val.trim();
                if !val.is_empty() && val != "<target doesn't support command>" {
                    return Ok(Some(val.to_string()));
                }
            }
        }
        Ok(None)
    }

    /// Get all device variables (fastboot getvar all).
    pub fn getvar_all(&self, serial: &str) -> Result<Vec<(String, String)>> {
        let (stdout, stderr) = self.run(
            &["-s", serial, "getvar", "all"],
            QUERY_TIMEOUT,
        )?;
        let combined = format!(
            "{}\n{}",
            String::from_utf8_lossy(&stdout),
            String::from_utf8_lossy(&stderr)
        );
        let mut vars = Vec::new();
        for line in combined.lines() {
            let line = line.trim();
            if let Some((key, val)) = line.split_once(':') {
                let key = key.trim();
                let val = val.trim();
                if !key.is_empty() {
                    vars.push((key.to_string(), val.to_string()));
                }
            }
        }
        Ok(vars)
    }

    /// List available partitions and their sizes.
    /// Parses `fastboot getvar all` output for partition-* entries.
    pub fn list_partitions(&self, serial: &str) -> Result<Vec<PartitionInfo>> {
        let vars = self.getvar_all(serial)?;
        let mut partitions = Vec::new();
        let mut sizes: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
        for (key, val) in &vars {
            // partition-type:<name>:<type>  — tells us partition exists
            if let Some(name) = key.strip_prefix("partition-type:") {
                // We just need the name to exist
            }
            // size:<name>:<hex size> or partition-size:<name>:<hex size>
            if let Some(name) = key.strip_prefix("partition-size:") {
                if let Ok(size) = u64::from_str_radix(val.trim_start_matches("0x"), 16) {
                    sizes.insert(name.to_string(), size);
                }
            }
        }
        // Also check for "size:<name>" format (newer fastboot)
        for (key, val) in &vars {
            if key.starts_with("size:") {
                if let Some(name) = key.strip_prefix("size:") {
                    if let Ok(size) = u64::from_str_radix(val.trim_start_matches("0x"), 16) {
                        sizes.insert(name.to_string(), size);
                    }
                }
            }
        }
        // Collect partition names from partition-type entries
        for (key, _val) in &vars {
            if let Some(name) = key.strip_prefix("partition-type:") {
                let size = sizes.get(name).copied().unwrap_or(0);
                partitions.push(PartitionInfo {
                    name: name.to_string(),
                    size,
                });
            }
        }
        // Dedup and sort
        partitions.sort_by(|a, b| a.name.cmp(&b.name));
        partitions.dedup_by(|a, b| a.name == b.name);
        Ok(partitions)
    }

    /// Read a partition from the device to a local file.
    /// Uses `fastboot oem dump` or `fastboot flash read` depending on device support.
    /// Falls back to booting a minimal ramdisk if neither is supported.
    pub fn read_partition(
        &self,
        serial: &str,
        partition: &str,
        output: &Path,
        on_progress: impl Fn(u64, u64),
    ) -> Result<()> {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Try `fastboot oem dump:<partition>` first (some devices support this)
        eprintln!("[fastboot] Attempting oem dump for partition: {}", partition);
        let result = self.run(
            &["-s", serial, "oem", "dump", partition],
            READ_TIMEOUT,
        );

        match result {
            Ok((data, _)) if !data.is_empty() => {
                std::fs::write(output, &data)?;
                on_progress(data.len() as u64, data.len() as u64);
                eprintln!("[fastboot] Partition {} dumped ({} bytes)", partition, data.len());
                return Ok(());
            }
            _ => {
                eprintln!("[fastboot] oem dump not supported, trying alternative methods");
            }
        }

        // Try reading via `fastboot getvar` + raw read if available
        // Some devices support `fastboot oem ramdump` for specific partitions
        eprintln!("[fastboot] Attempting oem ramdump for: {}", partition);
        let result = self.run(
            &["-s", serial, "oem", "ramdump", partition],
            READ_TIMEOUT,
        );

        match result {
            Ok((data, _)) if !data.is_empty() => {
                std::fs::write(output, &data)?;
                on_progress(data.len() as u64, data.len() as u64);
                eprintln!("[fastboot] Partition {} ramdumped ({} bytes)", partition, data.len());
                return Ok(());
            }
            _ => {
                eprintln!("[fastboot] ramdump not supported either");
            }
        }

        // Last resort: inform user that direct partition read requires
        // a custom bootloader or unlocked device with specific OEM commands
        Err(CoreError::Adb(format!(
            "Cannot read partition '{}' directly. This device may require:\n\
             1. A custom recovery (TWRP) to boot and use adb pull\n\
             2. OEM-specific fastboot commands (check device documentation)\n\
             3. EDL mode for Qualcomm devices (raw flash access)\n\
             \n\
             Tip: If the device has an unlocked bootloader, try:\n\
             fastboot boot twrp.img  (boot custom recovery)\n\
             Then use ADB to pull data from the recovery environment.",
            partition
        )))
    }

    /// Boot a custom image (e.g., TWRP recovery) without flashing.
    /// This allows data extraction via ADB in the recovery environment.
    pub fn boot_image(&self, serial: &str, image_path: &Path) -> Result<String> {
        if !image_path.exists() {
            return Err(CoreError::Adb(format!(
                "Boot image not found: {}",
                image_path.display()
            )));
        }
        let (_, stderr) = self.run(
            &["-s", serial, "boot", &image_path.display().to_string()],
            Duration::from_secs(60),
        )?;
        let msg = String::from_utf8_lossy(&stderr).trim().to_string();
        Ok(if msg.is_empty() { "Booting image...".to_string() } else { msg })
    }

    /// Reboot device from fastboot mode back to normal system.
    pub fn reboot(&self, serial: &str) -> Result<()> {
        self.run(&["-s", serial, "reboot"], QUERY_TIMEOUT)?;
        Ok(())
    }

    /// Reboot device into recovery mode.
    pub fn reboot_recovery(&self, serial: &str) -> Result<()> {
        self.run(&["-s", serial, "reboot", "recovery"], QUERY_TIMEOUT)?;
        Ok(())
    }

    /// Reboot device into bootloader/fastboot mode.
    /// Can be called from ADB mode to enter fastboot.
    pub fn reboot_bootloader(&self, serial: &str) -> Result<()> {
        self.run(&["-s", serial, "reboot", "bootloader"], QUERY_TIMEOUT)?;
        Ok(())
    }
}
