//! Ringtone playback — plays a generated tone on the connected device.
//!
//! Generates a short WAV file (800 Hz sine wave, 3 seconds), pushes it to
//! the device, then plays it at full volume.  Three methods are tried in
//! order of preference:
//!   1. `/system/bin/media player play` — simplest, but the binary is
//!      absent on many devices.
//!   2. `app_process` with an embedded DEX (SilentPlayer) — runs Java
//!      `MediaPlayer` code directly on the device with **zero UI**.
//!   3. `am start` with the system ringtone `content://` URI — always
//!      produces audio but briefly opens a media-player activity.

use crate::adb::AdbClient;
use crate::error::{CoreError, Result};
use std::io::Write;
use std::path::PathBuf;

const DEVICE_WAV: &str = "/sdcard/Music/recoverkit_ringtone.wav";
const DEVICE_DEX: &str = "/data/local/tmp/recoverkit_silent.dex";
const DEVICE_STOP_FLAG: &str = "/data/local/tmp/recoverkit_stop";
const DEX_CLASS: &str = "com.recoverkit.audio.SilentPlayer";
const LOCKER_CLASS: &str = "com.recoverkit.audio.DeviceLocker";
const UNLOCK_SIGNAL: &str = "/data/local/tmp/recoverkit_unlock_signal";
const LOCK_PID_FILE: &str = "/data/local/tmp/recoverkit_lock.pid";
const SAMPLE_RATE: u32 = 16000;
const DURATION_SECS: f32 = 3.0;
const FREQUENCY: f32 = 800.0;

/// Pre-compiled DEX containing SilentPlayer + DeviceLocker — embedded at compile time.
static SILENT_PLAYER_DEX: &[u8] = include_bytes!(
    "../assets/ringtone-helper/dex/classes.dex"
);

/// Pre-compiled DEX containing AppRestrictor — disables user apps from system context.
static APP_RESTRICTOR_DEX: &[u8] = include_bytes!(
    "../assets/ringtone-helper/dex_restrict/classes.dex"
);

/// Device path for the AppRestrictor DEX.
const RESTRICTOR_DEX: &str = "/data/local/tmp/recoverkit_restrictor.dex";

/// AppRestrictor main class.
const RESTRICTOR_CLASS: &str = "com.recoverkit.audio.AppRestrictor";

/// Pre-compiled DEX containing ScreenOverlay — fullscreen black overlay.
static SCREEN_OVERLAY_DEX: &[u8] = include_bytes!(
    "../assets/ringtone-helper/dex_overlay/classes.dex"
);

/// Device path for the ScreenOverlay DEX.
const OVERLAY_DEX: &str = "/data/local/tmp/recoverkit_overlay.dex";

/// ScreenOverlay main class.
const OVERLAY_CLASS: &str = "com.recoverkit.audio.ScreenOverlay";

/// Generate a PCM WAV file (16-bit mono) containing a sine-wave tone.
fn generate_wav(freq: f32, duration: f32, sample_rate: u32) -> Vec<u8> {
    let num_samples = (sample_rate as f32 * duration) as u32;
    let data_size = num_samples * 2; // 16-bit = 2 bytes per sample
    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&(sample_rate * 2).to_le_bytes()); // byte rate
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for i in 0..num_samples {
        let t = i as f32 / sample_rate as f32;
        // Fade in/out envelope to avoid clicks
        let env = if t < 0.05 {
            t / 0.05
        } else if t > duration - 0.1 {
            (duration - t) / 0.1
        } else {
            1.0
        };
        let sample = (env * 0.8 * (2.0 * std::f32::consts::PI * freq * t).sin()) as i16;
        buf.extend_from_slice(&sample.to_le_bytes());
    }

    buf
}

/// Play a ringtone on the connected device.
///
/// Three methods are tried in order:
///   1. `/system/bin/media player play` — no UI, but binary may be absent.
///   2. `app_process` + embedded DEX (`SilentPlayer`) — plays via Java
///      `MediaPlayer` with **zero UI** on the device screen.
///   3. `am start` with system ringtone `content://` URI — opens a media
///      player activity briefly; press HOME to push it to background.
pub fn play_ringtone(adb: &AdbClient, serial: &str) -> Result<()> {
    eprintln!("[RINGTONE] Starting ringtone playback for device: {}", serial);

    // ── Max volume on ringtone stream ────────────────────────────────────
    //    Use `cmd media_session` which works on Android 13+ (where `media`
    //    binary is absent).  Also try `settings put` as a fallback.
    eprintln!("[RINGTONE] Setting max volume...");
    let _ = adb.shell(serial, &["cmd", "media_session", "volume", "--show", "--stream", "2", "--set", "7"]);
    let _ = adb.shell(serial, &["cmd", "media_session", "volume", "--show", "--stream", "0", "--set", "7"]);
    let _ = adb.shell(serial, &["settings", "put", "system", "volume_ring", "15"]);

    // ── Lock down device: hide navigation + status bars ──────────────────
    //    Prevents the finder from pressing Home, Back, Recents, or pulling
    //    down the notification shade to silence the phone.
    lock_device(adb, serial);

    // ── Generate & push WAV ──────────────────────────────────────────────
    let wav = generate_wav(FREQUENCY, DURATION_SECS, SAMPLE_RATE);
    eprintln!("[RINGTONE] Generated WAV file: {} bytes", wav.len());

    let local_wav = std::env::temp_dir().join("recoverkit_ringtone.wav");
    let mut f = std::fs::File::create(&local_wav)?;
    f.write_all(&wav)?;
    drop(f);

    let _ = adb.shell(serial, &["mkdir", "-p", "/sdcard/Music"]);
    adb.push(serial, &local_wav, DEVICE_WAV)?;
    eprintln!("[RINGTONE] Pushed WAV to device: {}", DEVICE_WAV);

    // ── Method 1: app_process + SilentPlayer DEX (PRIMARY — no UI) ───────
    //    Push the pre-compiled DEX, then run it via app_process.  The Java
    //    code uses ToneGenerator directly — no file, no Activity, no UI.
    //    This is the PRIMARY method because it generates audio in-memory
    //    and plays it on STREAM_ALARM (bypasses silent/DND modes).
    eprintln!("[RINGTONE] Method 1: app_process SilentPlayer...");

    let local_dex = std::env::temp_dir().join("recoverkit_silent.dex");
    std::fs::write(&local_dex, SILENT_PLAYER_DEX)?;
    eprintln!("[RINGTONE] Wrote DEX to local temp: {} bytes", SILENT_PLAYER_DEX.len());

    adb.push(serial, &local_dex, DEVICE_DEX)?;
    eprintln!("[RINGTONE] Pushed DEX to device: {}", DEVICE_DEX);

    // app_process needs the DEX on the CLASSPATH env var (not as a direct arg).
    // IMPORTANT: We pass the whole command as a SINGLE arg to `adb shell`.
    // Using `sh -c` as separate args truncates the command (sh -c only takes
    // the next arg as the command string, losing the rest).
    let cmd = format!("CLASSPATH={} app_process / {}", DEVICE_DEX, DEX_CLASS);
    let r1 = adb.shell(serial, &[&cmd]);
    eprintln!("[RINGTONE] Method 1 result: {:?}", r1);

    if r1.is_ok() {
        eprintln!("[RINGTONE] Silent playback via app_process — waiting 7 s...");
        std::thread::sleep(std::time::Duration::from_secs(7));
        cleanup(adb, serial, &local_wav);
        let _ = adb.shell(serial, &["rm", "-f", DEVICE_DEX]);
        let _ = std::fs::remove_file(&local_dex);
        unlock_device(adb, serial);
        return Ok(());
    }

    // ── Method 2 (FALLBACK): `am start` with WAV file via system player ──
    eprintln!("[RINGTONE] Method 1 failed, trying Method 2: am start with WAV file...");
    let r2 = adb.shell(serial, &[
        "am", "start",
        "-a", "android.intent.action.VIEW",
        "-d", &format!("file://{}", DEVICE_WAV),
        "-t", "audio/wav",
        "-f", "0x10000000",  // FLAG_ACTIVITY_NEW_TASK
    ]);
    eprintln!("[RINGTONE] Method 2 result: {:?}", r2);
    if r2.is_ok() {
        eprintln!("[RINGTONE] am start succeeded — waiting 1.5 s then pressing HOME...");
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = adb.shell(serial, &["input", "keyevent", "KEYCODE_HOME"]);
        eprintln!("[RINGTONE] Pressed HOME — audio should continue in background");
        std::thread::sleep(std::time::Duration::from_secs(5));
        cleanup(adb, serial, &local_wav);
        unlock_device(adb, serial);
        return Ok(());
    }

    // ── Method 3 (LAST RESORT): system ringtone via content URI ──────────
    eprintln!("[RINGTONE] Method 2 failed, falling back to system ringtone...");
    let r3 = adb.shell(serial, &[
        "am", "start",
        "-a", "android.intent.action.VIEW",
        "-d", "content://settings/system/ringtone",
        "-t", "audio/*",
    ]);
    eprintln!("[RINGTONE] Method 3 result: {:?}", r3);

    std::thread::sleep(std::time::Duration::from_secs(1));
    let _ = adb.shell(serial, &["input", "keyevent", "KEYCODE_HOME"]);
    eprintln!("[RINGTONE] Pressed HOME to push player to background");

    eprintln!("[RINGTONE] Waiting 5 seconds for playback...");
    std::thread::sleep(std::time::Duration::from_secs(5));

    cleanup(adb, serial, &local_wav);
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_DEX]);
    unlock_device(adb, serial);

    eprintln!("[RINGTONE] Ringtone playback completed (fallback)");
    Ok(())
}

/// Start a looping ringtone on the connected device (non-blocking).
///
/// Pushes the DEX, then runs `app_process` in the **background** on the
/// device so that the ADB shell returns immediately.  The Java code loops
/// indefinitely, playing tones on STREAM_ALARM, until a stop-flag file
/// (`/data/local/tmp/recoverkit_stop`) appears — created by `stop_ringtone`.
pub fn start_ringtone_loop(adb: &AdbClient, serial: &str) -> Result<()> {
    eprintln!("[RINGTONE] Starting looping ringtone for device: {}", serial);

    // ── Max volume on alarm + ring streams ──────────────────────────────
    eprintln!("[RINGTONE] Setting max volume...");
    let _ = adb.shell(serial, &["cmd", "media_session", "volume", "--show", "--stream", "2", "--set", "7"]);
    let _ = adb.shell(serial, &["cmd", "media_session", "volume", "--show", "--stream", "0", "--set", "7"]);
    let _ = adb.shell(serial, &["settings", "put", "system", "volume_ring", "15"]);

    // ── Lock down device ────────────────────────────────────────────────
    lock_device(adb, serial);

    // ── Push DEX ────────────────────────────────────────────────────────
    let local_dex = std::env::temp_dir().join("recoverkit_silent.dex");
    std::fs::write(&local_dex, SILENT_PLAYER_DEX)?;
    eprintln!("[RINGTONE] Wrote DEX to local temp: {} bytes", SILENT_PLAYER_DEX.len());

    adb.push(serial, &local_dex, DEVICE_DEX)?;
    eprintln!("[RINGTONE] Pushed DEX to device: {}", DEVICE_DEX);

    // ── Remove any stale stop-flag from a previous run ──────────────────
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_STOP_FLAG]);

    // ── Run app_process in the BACKGROUND on the device ─────────────────
    //    The `&` at the end backgrounds the process so `sh` returns
    //    immediately.  Output goes to a log file for debugging.
    let cmd = format!(
        "CLASSPATH={} app_process / {} > /data/local/tmp/recoverkit_ring.log 2>&1 &",
        DEVICE_DEX, DEX_CLASS
    );
    let r = adb.shell(serial, &[&cmd]);
    eprintln!("[RINGTONE] Background app_process result: {:?}", r);

    // Give the Java code a moment to start up
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Verify it's running by checking the log file
    let log_check = adb.shell(serial, &["cat", "/data/local/tmp/recoverkit_ring.log"]);
    if let Ok(log_bytes) = log_check {
        let log_text = String::from_utf8_lossy(&log_bytes);
        eprintln!("[RINGTONE] Java startup log:\n{}", log_text);
    }

    eprintln!("[RINGTONE] Looping ringtone started — will play until stopped");
    Ok(())
}

/// Stop the looping ringtone by creating a stop-flag file on the device.
///
/// The Java code checks for this file every iteration; once it appears,
/// the loop breaks, audio stops, and the process exits cleanly.
pub fn stop_ringtone(adb: &AdbClient, serial: &str) -> Result<()> {
    eprintln!("[RINGTONE] Stopping ringtone...");

    // Create the stop-flag file — the Java loop checks for this
    let _ = adb.shell(serial, &["touch", DEVICE_STOP_FLAG]);

    // Wait a moment for the Java code to detect the flag and exit
    std::thread::sleep(std::time::Duration::from_secs(2));

    // Clean up device files
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_DEX]);
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_STOP_FLAG]);
    let _ = adb.shell(serial, &["rm", "-f", "/data/local/tmp/recoverkit_ring.log"]);

    // Unlock device
    unlock_device(adb, serial);

    eprintln!("[RINGTONE] Ringtone stopped and device unlocked");
    Ok(())
}

/// Remove temp files from device and host.
fn cleanup(adb: &AdbClient, serial: &str, local_wav: &std::path::Path) {
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_WAV]);
    let _ = std::fs::remove_file(local_wav);
}

/// The active system launcher (cannot be disabled without root).
const PRIMARY_LAUNCHER: &str = "com.android.launcher3";

/// Secondary launcher packages that CAN be disabled from shell.
const SECONDARY_LAUNCHERS: &[&str] = &[
    "com.vivo.simplelauncher",
    "com.jio.home",
];

/// Lock down the device: turn off screen, disable ALL user apps, block navigation.
/// Uses two DEX files:
///   1. DeviceLocker (system context) — goToSleep + wakeUp
///   2. AppRestrictor (system context) — disables all user-installed apps
/// Plus shell commands for launchers and navigation.
pub fn lock_device(adb: &AdbClient, serial: &str) {
    eprintln!("[LOCKDOWN] Locking device UI...");

    // 1. Push all DEX files
    let local_dex = std::env::temp_dir().join("recoverkit_silent.dex");
    let _ = std::fs::write(&local_dex, SILENT_PLAYER_DEX);
    let _ = adb.push(serial, &local_dex, DEVICE_DEX);

    let local_restrict = std::env::temp_dir().join("recoverkit_restrictor.dex");
    let _ = std::fs::write(&local_restrict, APP_RESTRICTOR_DEX);
    let _ = adb.push(serial, &local_restrict, RESTRICTOR_DEX);

    let local_overlay = std::env::temp_dir().join("recoverkit_overlay.dex");
    let _ = std::fs::write(&local_overlay, SCREEN_OVERLAY_DEX);
    let _ = adb.push(serial, &local_overlay, OVERLAY_DEX);
    eprintln!("[LOCKDOWN] Pushed DEX files: {} + {} + {} bytes",
             SILENT_PLAYER_DEX.len(), APP_RESTRICTOR_DEX.len(), SCREEN_OVERLAY_DEX.len());

    // 2. Clean up any stale signal/pid files
    let _ = adb.shell(serial, &["rm", "-f", UNLOCK_SIGNAL, LOCK_PID_FILE]);

    // 3. Run AppRestrictor in BACKGROUND — it stays alive calling stopAppSwitches()
    //    every 3 seconds to prevent ANY app from launching.
    eprintln!("[LOCKDOWN] Starting AppRestrictor daemon (disables apps + blocks launches)...");
    let cmd = format!(
        "CLASSPATH={} app_process / {} restrict > /data/local/tmp/recoverkit_restrict.log 2>&1 &",
        RESTRICTOR_DEX, RESTRICTOR_CLASS
    );
    let _ = adb.shell(serial, &[&cmd]);

    // 3b. Try fullscreen overlay (best-effort — may fail due to UID mismatch on Android 15)
    eprintln!("[LOCKDOWN] Attempting fullscreen overlay...");
    let cmd = format!(
        "CLASSPATH={} app_process / {} show > /data/local/tmp/recoverkit_overlay.log 2>&1 &",
        OVERLAY_DEX, OVERLAY_CLASS
    );
    let _ = adb.shell(serial, &[&cmd]);

    // 4. Run DeviceLocker in background (goToSleep via system context)
    let cmd = format!(
        "CLASSPATH={} app_process / {} lock > /data/local/tmp/recoverkit_lock.log 2>&1 &",
        DEVICE_DEX, LOCKER_CLASS
    );
    let _ = adb.shell(serial, &[&cmd]);

    // 4b. CRITICAL: Send POWER key to actually LOCK the device
    //     goToSleep only turns off screen; POWER key locks it (requires password on wake)
    let _ = adb.shell(serial, &["input", "keyevent", "26"]);

    // 4c. Stop charging — freeze battery as "unplugged" so device drains
    eprintln!("[LOCKDOWN] Stopping charging (battery unplug)...");
    let _ = adb.shell(serial, &["cmd", "battery", "unplug"]);

    // 5. Wait for screen to turn off and lock to engage
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 6. Disable secondary launcher packages
    eprintln!("[LOCKDOWN] Disabling secondary launchers...");
    for pkg in SECONDARY_LAUNCHERS {
        let _ = adb.shell(serial, &["pm", "disable-user", "--user", "0", pkg]);
    }

    // 7. Restrict primary launcher via appops
    let _ = adb.shell(serial, &["appops", "set", PRIMARY_LAUNCHER, "RUN_IN_BACKGROUND", "ignore"]);
    let _ = adb.shell(serial, &["appops", "set", PRIMARY_LAUNCHER, "WAKE_LOCK", "ignore"]);

    // 8. Force-stop all launchers
    let _ = adb.shell(serial, &["am", "force-stop", PRIMARY_LAUNCHER]);
    for pkg in SECONDARY_LAUNCHERS {
        let _ = adb.shell(serial, &["am", "force-stop", pkg]);
    }

    // 9. Switch from gesture nav to 3-button nav (removes gesture handle/pill)
    eprintln!("[LOCKDOWN] Switching to 3-button nav (removes gesture handle)...");
    let _ = adb.shell(serial, &["settings", "put", "secure", "navigation_mode", "2"]);
    std::thread::sleep(std::time::Duration::from_millis(500));

    // 10. Disable ALL navigation buttons + status bar
    let _ = adb.shell(serial, &[
        "cmd", "statusbar", "send-disable-flag",
        "home", "recents", "statusbar-expansion",
        "system-icons", "notification-peek",
        "clock", "notification-icons", "search",
    ]);

    eprintln!("[LOCKDOWN] Device locked (POWER lock + apps disabled + nav blocked)");
}

/// Restore the device UI to normal — re-enable apps, wake screen, restore nav.
pub fn unlock_device(adb: &AdbClient, serial: &str) {
    eprintln!("[LOCKDOWN] Unlocking device UI...");

    // 1. Re-enable ALL user-installed apps via AppRestrictor (system context)
    eprintln!("[LOCKDOWN] Re-enabling user apps...");
    let cmd = format!(
        "CLASSPATH={} app_process / {} restore",
        RESTRICTOR_DEX, RESTRICTOR_CLASS
    );
    let r = adb.shell(serial, &[&cmd]);
    eprintln!("[LOCKDOWN] AppRestrictor restore: {:?}", r.map(|v| String::from_utf8_lossy(&v).to_string()));

    // 2. Signal lock process to exit + wake screen
    let cmd = format!("CLASSPATH={} app_process / {} unlock", DEVICE_DEX, LOCKER_CLASS);
    let r = adb.shell(serial, &[&cmd]);
    eprintln!("[LOCKDOWN] Unlock result: {:?}", r);

    // 2b. Resume charging — reset battery to real hardware values
    eprintln!("[LOCKDOWN] Resuming charging (battery reset)...");
    let _ = adb.shell(serial, &["cmd", "battery", "reset"]);

    // 3. Wait for lock process cleanup
    std::thread::sleep(std::time::Duration::from_secs(2));

    // 4. Re-enable secondary launcher packages
    eprintln!("[LOCKDOWN] Re-enabling launchers...");
    for pkg in SECONDARY_LAUNCHERS {
        let _ = adb.shell(serial, &["pm", "enable", pkg]);
    }

    // 5. Reset primary launcher appops restrictions
    let _ = adb.shell(serial, &["appops", "set", PRIMARY_LAUNCHER, "RUN_IN_BACKGROUND", "allow"]);
    let _ = adb.shell(serial, &["appops", "set", PRIMARY_LAUNCHER, "WAKE_LOCK", "allow"]);

    // 6. Clear statusbar disable flags
    let _ = adb.shell(serial, &[
        "cmd", "statusbar", "send-disable-flag", "none",
    ]);

    // 7. Restore gesture navigation mode
    eprintln!("[LOCKDOWN] Restoring gesture navigation...");
    let _ = adb.shell(serial, &["settings", "put", "secure", "navigation_mode", "0"]);

    // 8. Clean up DEX and temp files
    let _ = adb.shell(serial, &["rm", "-f", DEVICE_DEX, RESTRICTOR_DEX, OVERLAY_DEX, UNLOCK_SIGNAL, LOCK_PID_FILE]);

    eprintln!("[LOCKDOWN] Device unlocked");
}

/// Get the temp path where the ringtone WAV is staged before push.
pub fn ringtone_staging_path() -> PathBuf {
    std::env::temp_dir().join("recoverkit_ringtone.wav")
}

/// Stop charging — freeze battery as "unplugged" via cmd battery.
/// The device thinks the charger is disconnected even if physically plugged in.
pub fn stop_charging(adb: &AdbClient, serial: &str) -> Result<String> {
    eprintln!("[BATTERY] Stopping charging...");
    let r = adb.shell(serial, &["cmd", "battery", "unplug"]);
    match r {
        Ok(_) => {
            let msg = "Charging STOPPED (battery frozen as unplugged)".to_string();
            eprintln!("[BATTERY] {}", msg);
            Ok(msg)
        }
        Err(e) => {
            let msg = format!("Failed to stop charging: {}", e);
            eprintln!("[BATTERY] {}", msg);
            Err(CoreError::Other(msg))
        }
    }
}

/// Start charging — reset battery to real hardware values.
/// Returns normal charging behavior.
pub fn start_charging(adb: &AdbClient, serial: &str) -> Result<String> {
    eprintln!("[BATTERY] Starting charging...");
    let r = adb.shell(serial, &["cmd", "battery", "reset"]);
    match r {
        Ok(_) => {
            let msg = "Charging STARTED (battery reset to hardware values)".to_string();
            eprintln!("[BATTERY] {}", msg);
            Ok(msg)
        }
        Err(e) => {
            let msg = format!("Failed to start charging: {}", e);
            eprintln!("[BATTERY] {}", msg);
            Err(CoreError::Other(msg))
        }
    }
}

/// Get current battery status.
pub fn get_battery_status(adb: &AdbClient, serial: &str) -> Result<String> {
    let r = adb.shell(serial, &["dumpsys", "battery"]);
    match r {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output).to_string();
            // Extract key fields
            let mut info = String::new();
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with("USB powered:")
                    || trimmed.starts_with("AC powered:")
                    || trimmed.starts_with("level:")
                    || trimmed.starts_with("status:")
                    || trimmed.starts_with("temperature:")
                    || trimmed.starts_with("Charging state:")
                {
                    info.push_str(trimmed);
                    info.push('\n');
                }
            }
            Ok(if info.is_empty() { text } else { info })
        }
        Err(e) => Err(CoreError::Other(format!("Failed to get battery status: {}", e))),
    }
}
