package com.recoverkit.audio;

import android.content.Context;
import android.os.Looper;
import android.os.PowerManager;

import java.io.File;

/**
 * Device locker for Android 15 (no root required).
 *
 * Lock:  Turns off screen via input keyevent 26 (power button sim).
 *         Process stays alive polling for unlock signal.
 * Unlock: Wakes screen + signals lock process to exit.
 *
 * Note: On Android 15 without root, overlay windows and goToSleep
 * are blocked from shell UID. input keyevent 26 is the reliable fallback.
 */
public class DeviceLocker {

    private static final String UNLOCK_SIG = "/data/local/tmp/recoverkit_unlock_signal";
    private static final String LOCK_PID   = "/data/local/tmp/recoverkit_lock.pid";

    public static void main(String[] args) {
        if (args.length < 1) {
            System.err.println("[RecoverKit] Usage: DeviceLocker lock|unlock");
            System.exit(1);
        }
        String action = args[0];
        System.out.println("[RecoverKit] DeviceLocker: " + action);
        try {
            Looper.prepareMainLooper();

            Context sysCtx;
            try {
                Class<?> atClass = Class.forName("android.app.ActivityThread");
                Object at = atClass.getMethod("systemMain").invoke(null);
                sysCtx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            } catch (Exception e) {
                System.err.println("[RecoverKit] No system context: " + e.getMessage());
                System.exit(1);
                return;
            }

            if ("lock".equals(action)) {
                doLock(sysCtx);
            } else {
                doUnlock(sysCtx);
            }
        } catch (Exception e) {
            System.err.println("[RecoverKit] Error: " + e.getMessage());
            System.exit(1);
        }
        System.exit(0);
    }

    private static void doLock(Context ctx) throws Exception {
        new File(UNLOCK_SIG).delete();

        // Write PID
        try {
            java.io.FileWriter fw = new java.io.FileWriter(LOCK_PID);
            fw.write(String.valueOf(android.os.Process.myPid()));
            fw.close();
        } catch (Exception ignored) {}

        // 1. Try goToSleep via system context (needs uptimeMillis, not currentTimeMillis!)
        boolean screenOff = false;
        try {
            PowerManager pm = (PowerManager) ctx.getSystemService(Context.POWER_SERVICE);
            long uptime = (Long) Class.forName("android.os.SystemClock")
                    .getMethod("uptimeMillis").invoke(null);
            pm.getClass().getMethod("goToSleep", long.class)
              .invoke(pm, uptime);
            System.out.println("[RecoverKit] Screen off (goToSleep via system context)");
            screenOff = true;
        } catch (Exception e) {
            System.err.println("[RecoverKit] goToSleep failed: " + e.getClass().getName() + ": " + e.getMessage());
            if (e.getCause() != null) {
                System.err.println("[RecoverKit]   cause: " + e.getCause().getMessage());
            }
        }

        // 2. Fallback: input keyevent 26 (power button press)
        if (!screenOff) {
            try {
                Runtime.getRuntime().exec(new String[]{
                    "/system/bin/sh", "-c", "input keyevent KEYCODE_SLEEP"
                });
                System.out.println("[RecoverKit] Screen off (input keyevent SLEEP fallback)");
                screenOff = true;
            } catch (Exception e) {
                System.err.println("[RecoverKit] SLEEP key failed: " + e.getMessage());
            }
        }

        // 3. Disable ALL input dispatch via IWindowManager.setEventDispatching(false)
        trySetEventDispatching(true);

        // 4. Try to disable input dispatch via IWindowManager legacy method
        trySetInputDispatch(true);

        System.out.println("[RecoverKit] Device LOCKED");

        // 5. Poll for unlock signal — use a thread instead of Looper.quit()
        final boolean[] running = { true };
        Thread poller = new Thread(new Runnable() {
            @Override public void run() {
                while (running[0]) {
                    if (new File(UNLOCK_SIG).exists()) {
                        running[0] = false;
                        new File(UNLOCK_SIG).delete();
                        new File(LOCK_PID).delete();
                        System.out.println("[RecoverKit] Unlock signal received");
                        System.exit(0);
                        return;
                    }
                    try { Thread.sleep(500); } catch (Exception ignored) {}
                }
            }
        });
        poller.setDaemon(true);
        poller.start();

        // Keep process alive
        Looper.loop();
    }

    private static void doUnlock(Context ctx) throws Exception {
        // 1. Signal lock process to exit
        try {
            java.io.FileWriter fw = new java.io.FileWriter(UNLOCK_SIG);
            fw.write("unlock"); fw.close();
            System.out.println("[RecoverKit] Unlock signal sent");
        } catch (Exception e) {
            System.err.println("[RecoverKit] Signal failed: " + e.getMessage());
        }

        // 2. Wake screen using uptimeMillis
        boolean woke = false;
        try {
            PowerManager pm = (PowerManager) ctx.getSystemService(Context.POWER_SERVICE);
            long uptime = (Long) Class.forName("android.os.SystemClock")
                    .getMethod("uptimeMillis").invoke(null);
            pm.getClass().getMethod("wakeUp", long.class)
              .invoke(pm, uptime);
            System.out.println("[RecoverKit] Screen wake (wakeUp via system context)");
            woke = true;
        } catch (Exception e) {
            System.err.println("[RecoverKit] wakeUp failed: " + e.getMessage());
        }
        if (!woke) {
            try {
                Runtime.getRuntime().exec(new String[]{
                    "/system/bin/sh", "-c", "input keyevent KEYCODE_WAKEUP"
                });
                System.out.println("[RecoverKit] Screen wake (KEYCODE_WAKEUP)");
            } catch (Exception e) {
                System.err.println("[RecoverKit] Wake key failed: " + e.getMessage());
            }
        }

        // 3. Re-enable input dispatch
        trySetEventDispatching(false);

        // 4. Re-enable input dispatch (legacy)
        trySetInputDispatch(false);

        // 5. Wait for lock process to exit
        Thread.sleep(2000);

        // 6. Kill lock process if still alive
        try {
            File pidFile = new File(LOCK_PID);
            if (pidFile.exists()) {
                java.io.BufferedReader br = new java.io.BufferedReader(new java.io.FileReader(pidFile));
                String pidStr = br.readLine(); br.close();
                if (pidStr != null && !pidStr.isEmpty()) {
                    android.os.Process.killProcess(Integer.parseInt(pidStr.trim()));
                }
                pidFile.delete();
            }
        } catch (Exception ignored) {}

        System.out.println("[RecoverKit] Device UNLOCKED");
    }

    private static void trySetEventDispatching(boolean disable) {
        try {
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Object binder = smClass.getMethod("getService", String.class).invoke(null, "window");
            Class<?> stubClass = Class.forName("android.view.IWindowManager$Stub");
            Object iwm = stubClass.getMethod("asInterface", Class.forName("android.os.IBinder")).invoke(null, binder);
            iwm.getClass().getMethod("setEventDispatching", boolean.class).invoke(iwm, disable);
            System.out.println("[RecoverKit] setEventDispatching(" + disable + ") ok");
        } catch (Exception e) {
            System.err.println("[RecoverKit] setEventDispatching failed: " + e.getMessage());
        }
    }

    private static void trySetInputDispatch(boolean disable) {
        try {
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Object binder = smClass.getMethod("getService", String.class).invoke(null, "window");
            Class<?> stubClass = Class.forName("android.view.IWindowManager$Stub");
            Object iwm = stubClass.getMethod("asInterface", Class.forName("android.os.IBinder")).invoke(null, binder);
            for (String m : new String[]{"setEventDispatchDisabled", "setInputDispatchDisabled"}) {
                try {
                    iwm.getClass().getMethod(m, boolean.class).invoke(iwm, disable);
                    System.out.println("[RecoverKit] " + m + " ok");
                    return;
                } catch (Exception ignored) {}
            }
        } catch (Exception ignored) {}
    }

    private static String execCmd(String cmd) {
        try {
            Process p = Runtime.getRuntime().exec(new String[]{"/system/bin/sh", "-c", cmd});
            java.io.BufferedReader br = new java.io.BufferedReader(
                new java.io.InputStreamReader(p.getInputStream()));
            String line = br.readLine();
            p.waitFor();
            return line;
        } catch (Exception e) {
            return null;
        }
    }
}
