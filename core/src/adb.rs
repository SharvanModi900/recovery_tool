//! Read-only ADB acquisition layer.
//!
//! Safety model: only `devices`, `pull` and `exec-out` are ever executed.
//! Every other subcommand (`push`, `shell`, `install`, `rm`, ...) is rejected
//! by [`AdbClient::check_allowed`] before any process is spawned, so this tool
//! can never modify the connected phone.

use crate::error::{CoreError, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Spawn adb without flashing a console window on Windows.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The only adb subcommands this tool is allowed to run.
const ALLOWED_SUBCOMMANDS: &[&str] = &["devices", "pull", "exec-out"];

/// Timeout for quick device queries (getprop, screencap, keyevent).
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);
/// Timeout for the device list refresh.
const LIST_TIMEOUT: Duration = Duration::from_secs(15);
/// Timeout for `adb pull` — large files can legitimately take a while.
const PULL_TIMEOUT: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub serial: String,
    /// `device`, `unauthorized`, `offline`, ...
    pub state: String,
    pub model: Option<String>,
    pub product: Option<String>,
    pub android_version: Option<String>,
}

pub struct AdbClient {
    adb_path: PathBuf,
}

impl Default for AdbClient {
    fn default() -> Self {
        Self::new()
    }
}

impl AdbClient {
    pub fn new() -> Self {
        Self { adb_path: Self::discover() }
    }

    /// Locate `adb`: explicit `ADB_PATH` env var first, then common install
    /// locations, finally plain `adb` (resolved via PATH by the OS).
    fn discover() -> PathBuf {
        if let Ok(p) = std::env::var("ADB_PATH") {
            let p = PathBuf::from(p);
            if p.exists() {
                return p;
            }
        }
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let home = std::env::var("USERPROFILE").unwrap_or_default();
        let pf = std::env::var("ProgramFiles").unwrap_or_default();
        let pf86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();
        let candidates = [
            format!("{local}\\Android\\Sdk\\platform-tools\\adb.exe"),
            format!("{home}\\AppData\\Local\\Android\\Sdk\\platform-tools\\adb.exe"),
            format!("{pf}\\platform-tools\\adb.exe"),
            format!("{pf86}\\platform-tools\\adb.exe"),
        ];
        for c in candidates {
            if c.starts_with('\\') {
                continue; // env var was empty
            }
            let p = PathBuf::from(&c);
            if p.exists() {
                return p;
            }
        }
        PathBuf::from("adb")
    }

    /// Enforce the read-only allowlist: the first non-flag argument must be a
    /// permitted subcommand.
    pub fn check_allowed(args: &[String]) -> Result<()> {
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            if a == "-s" {
                // `-s <serial>` pair
                i += 2;
                continue;
            }
            if a == "-d" || a == "-e" || a.starts_with('-') {
                i += 1;
                continue;
            }
            if ALLOWED_SUBCOMMANDS.contains(&a) {
                return Ok(());
            }
            return Err(CoreError::Adb(format!(
                "adb subcommand not allowed (read-only tool): {a}"
            )));
        }
        Err(CoreError::Adb("empty adb argument list".into()))
    }

    /// Run adb with a hard timeout. A wedged adb server/transport must never
    /// be able to hang the app forever — on timeout the child is killed and
    /// the caller gets a normal error it can show to the user.
    fn run(&self, args: &[String], timeout: Duration) -> Result<Vec<u8>> {
        Self::check_allowed(args)?;
        let mut cmd = Command::new(&self.adb_path);
        cmd.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd.spawn().map_err(|e| {
            CoreError::Adb(format!(
                "failed to run adb ({e}); install platform-tools or set ADB_PATH"
            ))
        })?;
        // Drain both pipes on their own threads so large outputs (2 MB+
        // screencaps) never deadlock on a full pipe buffer.
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
                Ok(Some(status)) => {
                    let stdout = out_thread.join().unwrap_or_default();
                    let stderr = err_thread.join().unwrap_or_default();
                    if !status.success() {
                        return Err(CoreError::Adb(
                            String::from_utf8_lossy(&stderr).trim().to_string(),
                        ));
                    }
                    return Ok(stdout);
                }
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(CoreError::Adb(format!(
                            "adb timed out after {}s ({}) — try the ⟳ refresh or reconnect the phone",
                            timeout.as_secs(),
                            args.join(" ")
                        )));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    return Err(CoreError::Adb(format!("adb process error: {e}")));
                }
            }
        }
    }

    /// List connected devices (`adb devices -l`) with model info.
    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let out = self.run(&["devices".to_string(), "-l".to_string()], LIST_TIMEOUT)?;
        let text = String::from_utf8_lossy(&out);
        let mut devices = Vec::new();
        for line in text.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let mut parts = line.split_whitespace();
            let Some(serial) = parts.next() else { continue };
            let Some(state) = parts.next() else { continue };
            let mut model = None;
            let mut product = None;
            for kv in parts {
                if let Some((k, v)) = kv.split_once(':') {
                    match k {
                        "model" => model = Some(v.replace('_', " ")),
                        "product" => product = Some(v.to_string()),
                        _ => {}
                    }
                }
            }
            let android_version = if state == "device" {
                self.prop(serial, "ro.build.version.release")
                    .ok()
                    .flatten()
            } else {
                None
            };
            devices.push(Device {
                serial: serial.to_string(),
                state: state.to_string(),
                model,
                product,
                android_version,
            });
        }
        Ok(devices)
    }

    /// Read a single system property via `exec-out getprop` (read-only).
    pub fn prop(&self, serial: &str, prop: &str) -> Result<Option<String>> {
        let out = self.exec_out(serial, &["getprop", prop])?;
        let v = String::from_utf8_lossy(&out).trim().to_string();
        Ok(if v.is_empty() { None } else { Some(v) })
    }

    /// Run a read-only command on the device and return raw stdout bytes.
    pub fn exec_out(&self, serial: &str, cmd: &[&str]) -> Result<Vec<u8>> {
        let mut args: Vec<String> = vec!["-s".into(), serial.into(), "exec-out".into()];
        args.extend(cmd.iter().map(|c| c.to_string()));
        self.run(&args, QUERY_TIMEOUT)
    }

    /// Copy a file from the device to a local path (read-only on device).
    pub fn pull(&self, serial: &str, remote: &str, local: &Path) -> Result<()> {
        if let Some(parent) = local.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let args: Vec<String> = vec![
            "-s".into(),
            serial.into(),
            "pull".into(),
            remote.into(),
            local.display().to_string(),
        ];
        self.run(&args, PULL_TIMEOUT)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn allowlist_permits_read_only_commands() {
        assert!(AdbClient::check_allowed(&s(&["devices", "-l"])).is_ok());
        assert!(AdbClient::check_allowed(&s(&["-s", "XYZ", "pull", "/a/b", "C:\\tmp"])).is_ok());
        assert!(
            AdbClient::check_allowed(&s(&["-s", "XYZ", "exec-out", "ls", "-la", "/sdcard"])).is_ok()
        );
    }

    #[test]
    fn allowlist_rejects_writes() {
        assert!(AdbClient::check_allowed(&s(&["shell", "rm", "-rf", "/"])).is_err());
        assert!(AdbClient::check_allowed(&s(&["push", "x", "/sdcard"])).is_err());
        assert!(AdbClient::check_allowed(&s(&["install", "app.apk"])).is_err());
        assert!(AdbClient::check_allowed(&s(&["-s", "XYZ", "uninstall", "com.x"])).is_err());
        assert!(AdbClient::check_allowed(&s(&["root"])).is_err());
    }
}
