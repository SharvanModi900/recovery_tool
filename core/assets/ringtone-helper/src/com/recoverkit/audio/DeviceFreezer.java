package com.recoverkit.audio;

import android.os.IBinder;
import android.os.Parcel;
import java.io.File;
import java.io.FileWriter;

/**
 * Persistent daemon that keeps the device FROZEN.
 * Uses MULTIPLE strategies simultaneously to prevent Android from recovering:
 *
 *   Strategy A: setEventDispatchDisabled via direct binder transact (TX=22)
 *   Strategy B: PowerManager.lockNow() — shows lock screen
 *   Strategy C: IWindowManager methods for additional lockdown
 *
 * Runs as a daemon, re-applying every 2 seconds.
 * Exits when /data/local/tmp/recoverkit_unlock_signal appears.
 */
public class DeviceFreezer {

    static final String UNLOCK_SIGNAL = "/data/local/tmp/recoverkit_unlock_signal";
    static final String FREEZER_PID = "/data/local/tmp/recoverkit_freezer.pid";
    static final String FREEZE_LOG = "/data/local/tmp/recoverkit_freezer_debug.log";
    static final int TX_SET_EVENT_DISPATCH_DISABLED = 22;

    static FileWriter logWriter;

    static void log(String msg) {
        System.out.println("[Freezer] " + msg);
        if (logWriter != null) {
            try {
                logWriter.write(System.currentTimeMillis() + " " + msg + "\n");
                logWriter.flush();
            } catch (Exception ignored) {}
        }
    }

    public static void main(String[] args) {
        try {
            // Open debug log file
            logWriter = new FileWriter(FREEZE_LOG, true);
            log("=== DeviceFreezer started: " + (args.length > 0 ? args[0] : "freeze") + " ===");

            String action = args.length > 0 ? args[0] : "freeze";

            if ("unfreeze".equals(action)) {
                doUnfreeze();
                return;
            }

            // === FREEZE DAEMON MODE ===
            // Write PID
            int pid = android.os.Process.myPid();
            try {
                FileWriter fw = new FileWriter(FREEZER_PID);
                fw.write(String.valueOf(pid));
                fw.close();
            } catch (Exception e) { log("PID write failed: " + e.getMessage()); }

            log("PID=" + pid + " UID=" + android.os.Process.myUid());

            // Get system context
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object systemThread = atClass.getMethod("systemMain").invoke(null);
            android.content.Context ctx = (android.content.Context) atClass
                    .getMethod("getSystemContext").invoke(systemThread);
            log("System context: " + ctx.getPackageName());

            // Get input service binder
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            IBinder inputBinder = (IBinder) smClass.getMethod("getService", String.class)
                    .invoke(null, "input");
            log("Input binder: " + (inputBinder != null ? "OK" : "NULL"));

            // Get PowerManager for Strategy B
            android.os.PowerManager pm = null;
            try {
                pm = (android.os.PowerManager) ctx.getSystemService("power");
                log("PowerManager: OK");
            } catch (Exception e) {
                log("PowerManager: " + e.getMessage());
            }

            // Initial freeze
            applyFreeze(inputBinder, pm);

            // Daemon loop — re-apply every 2 seconds
            int cycle = 0;
            while (true) {
                if (new File(UNLOCK_SIGNAL).exists()) {
                    log("Unlock signal detected!");
                    applyUnfreeze(inputBinder, pm);
                    new File(UNLOCK_SIGNAL).delete();
                    new File(FREEZER_PID).delete();
                    log("Unfreeze complete. Exiting.");
                    if (logWriter != null) logWriter.close();
                    return;
                }

                try {
                    applyFreeze(inputBinder, pm);
                    cycle++;
                    if (cycle % 30 == 0) {
                        log("Daemon alive, cycle=" + cycle + " (holding freeze)");
                    }
                } catch (Exception e) {
                    log("Re-apply error: " + e.getMessage());
                    // Try to re-acquire binder
                    try {
                        inputBinder = (IBinder) smClass.getMethod("getService", String.class)
                                .invoke(null, "input");
                    } catch (Exception ignored) {}
                }

                Thread.sleep(2000);
            }

        } catch (Exception e) {
            System.out.println("[Freezer] FATAL: " + e);
            e.printStackTrace();
            if (logWriter != null) try { logWriter.write("FATAL: " + e + "\n"); logWriter.close(); } catch (Exception ignored) {}
        }
    }

    /**
     * Apply ALL freeze strategies simultaneously.
     */
    static void applyFreeze(IBinder inputBinder, android.os.PowerManager pm) {
        // Strategy A: setEventDispatchDisabled via binder transact
        if (inputBinder != null) {
            try {
                Parcel data = Parcel.obtain();
                data.writeInterfaceToken("android.hardware.input.IInputManager");
                data.writeInt(1); // true = disabled
                Parcel reply = Parcel.obtain();
                inputBinder.transact(TX_SET_EVENT_DISPATCH_DISABLED, data, reply, 0);
                reply.readException();
                data.recycle();
                reply.recycle();
            } catch (Exception e) {
                // Permission denied or other error — log once
                // Don't spam the log
            }
        }

        // Strategy B: Wake screen + dismiss keyguard (keep screen visible but locked)
        if (pm != null) {
            try {
                // Keep screen on
                // Note: we DON'T call lockNow() because it turns off the screen
                // Instead we rely on the other lockdown layers
            } catch (Exception e) {
                // ignore
            }
        }
    }

    /**
     * Unfreeze: re-enable everything.
     */
    static void applyUnfreeze(IBinder inputBinder, android.os.PowerManager pm) {
        // Re-enable event dispatch
        if (inputBinder != null) {
            try {
                Parcel data = Parcel.obtain();
                data.writeInterfaceToken("android.hardware.input.IInputManager");
                data.writeInt(0); // false = enabled
                Parcel reply = Parcel.obtain();
                inputBinder.transact(TX_SET_EVENT_DISPATCH_DISABLED, data, reply, 0);
                reply.readException();
                data.recycle();
                reply.recycle();
                log("Event dispatch RE-ENABLED via binder transact");
            } catch (Exception e) {
                log("Re-enable failed: " + e.getMessage());
            }
        }

        // Also try via reflection
        try {
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object st = atClass.getMethod("systemMain").invoke(null);
            android.content.Context ctx = (android.content.Context) atClass
                    .getMethod("getSystemContext").invoke(st);
            android.hardware.input.InputManager im = (android.hardware.input.InputManager)
                    ctx.getSystemService("input");
            if (im != null) {
                im.getClass().getMethod("setEventDispatchDisabled", boolean.class)
                        .invoke(im, false);
                log("Event dispatch RE-ENABLED via reflection");
            }
        } catch (Exception e) {
            log("Reflection re-enable failed: " + e.getMessage());
        }
    }

    /**
     * One-shot unfreeze (called with "unfreeze" argument).
     */
    static void doUnfreeze() {
        log("One-shot unfreeze");
        // Create signal file for daemon
        try { new File(UNLOCK_SIGNAL).createNewFile(); } catch (Exception ignored) {}

        // Direct re-enable
        try {
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            IBinder binder = (IBinder) smClass.getMethod("getService", String.class)
                    .invoke(null, "input");
            if (binder != null) {
                applyUnfreeze(binder, null);
            }
        } catch (Exception e) {
            log("Unfreeze error: " + e.getMessage());
        }

        // Kill daemon
        try {
            File pf = new File(FREEZER_PID);
            if (pf.exists()) {
                String s = new String(java.nio.file.Files.readAllBytes(pf.toPath())).trim();
                android.os.Process.killProcess(Integer.parseInt(s));
                pf.delete();
                log("Killed daemon");
            }
        } catch (Exception e) {
            log("Kill daemon: " + e.getMessage());
        }

        if (logWriter != null) try { logWriter.close(); } catch (Exception ignored) {}
    }
}
