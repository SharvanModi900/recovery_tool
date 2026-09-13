package com.recoverkit.audio;

import android.content.Context;
import android.os.Handler;
import android.os.Looper;
import java.io.File;
import java.lang.reflect.Method;

/**
 * Tries to restrict app usage by:
 * 1. Setting screen_off_timeout to minimum (15 seconds)
 * 2. Trying to restrict launcher via ActivityManager forceStopPackage
 * 3. Trying to use Settings.Secure via system context
 */
public class AppRestrictor {
    private static final String UNLOCK_SIG = "/data/local/tmp/recoverkit_unlock_signal";

    public static void main(String[] args) {
        try {
            // Try to reduce OOM kill priority (requires root, but try anyway)
            try {
                new java.io.FileOutputStream("/proc/self/oom_score_adj")
                    .write("-1000".getBytes());
            } catch (Exception ignored) {}

            Looper.prepareMainLooper();

            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object at = atClass.getMethod("systemMain").invoke(null);
            Context ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            System.out.println("[Restrictor] System context: " + ctx.getPackageName());

            String action = args.length > 0 ? args[0] : "restrict";

            if ("restrict".equals(action)) {
                final Object iam = doRestrict(ctx);
                // Use Handler + Looper to keep process alive and call stopAppSwitches() periodically
                final Handler handler = new Handler(Looper.getMainLooper());
                System.out.println("[Restrictor] Starting periodic maintain via Handler...");
                handler.post(new Runnable() {
                    @Override
                    public void run() {
                        // Check for unlock signal
                        if (new File(UNLOCK_SIG).exists()) {
                            System.out.println("[Restrictor] Unlock signal detected, exiting");
                            new File(UNLOCK_SIG).delete();
                            Looper.myLooper().quit();
                            return;
                        }
                        // Call stopAppSwitches()
                        if (iam != null) {
                            try {
                                Method stopSwitches = iam.getClass().getMethod("stopAppSwitches");
                                stopSwitches.invoke(iam);
                            } catch (Exception ignored) {}
                        }
                        // Re-post after 3 seconds
                        handler.postDelayed(this, 3000);
                    }
                });
                System.out.println("[Restrictor] Entering Looper.loop()...");
                Looper.loop();
                System.out.println("[Restrictor] Looper exited");
            } else if ("restore".equals(action)) {
                doRestore(ctx);
            }

        } catch (Exception e) {
            System.out.println("[Restrictor] ERROR: " + e.getMessage());
            e.printStackTrace();
        }
    }

    static Object doRestrict(Context ctx) {
        Object iamResult = null;
        // 1. Try to set screen_off_timeout to 15 seconds via Settings.System
        System.out.println("[Restrictor] Setting screen timeout to 15s...");
        try {
            Class<?> systemClass = Class.forName("android.provider.Settings$System");
            Object contentResolver = ctx.getContentResolver();
            Method putInt = systemClass.getMethod("putInt",
                Class.forName("android.content.ContentResolver"),
                String.class, int.class);
            putInt.invoke(null, contentResolver, "screen_off_timeout", 15000);
            System.out.println("[Restrictor] Screen timeout SET to 15s");
        } catch (Exception e) {
            System.out.println("[Restrictor] Screen timeout failed: " + e.getClass().getName() + ": " + e.getMessage());
            if (e.getCause() != null) System.out.println("  cause: " + e.getCause());
        }

        // 2. Try to use IActivityManager.forceStopPackage on the launcher
        System.out.println("[Restrictor] Trying forceStopPackage on launcher...");
        try {
            Class<?> amClass = Class.forName("android.app.ActivityManager");
            // Get IActivityManager via ActivityManagerNative or ActivityManager.getService()
            Object iam;
            try {
                // Android 8+
                iam = amClass.getMethod("getService").invoke(null);
            } catch (Exception e) {
                // Android 7
                Class<?> amnClass = Class.forName("android.app.ActivityManagerNative");
                iam = amnClass.getMethod("getDefault").invoke(null);
            }
            System.out.println("[Restrictor] Got IActivityManager: " + iam);
            iamResult = iam;

            // List relevant methods
            for (Method m : iam.getClass().getMethods()) {
                String name = m.getName();
                if (name.contains("forceStop") || name.contains("kill") || name.contains("stop")) {
                    System.out.println("  method: " + name + " -> " + java.util.Arrays.toString(m.getParameterTypes()));
                }
            }

            // Try forceStopPackage
            try {
                Method forceStop = iam.getClass().getMethod("forceStopPackage", String.class, int.class);
                forceStop.invoke(iam, "com.android.launcher3", 0);
                System.out.println("[Restrictor] forceStopPackage called!");
            } catch (Exception e) {
                System.out.println("[Restrictor] forceStopPackage failed: " + e.getClass().getName() + ": " + e.getMessage());
            }

            // CRITICAL: stopAppSwitches() prevents ALL app launches
            try {
                Method stopSwitches = iam.getClass().getMethod("stopAppSwitches");
                stopSwitches.invoke(iam);
                System.out.println("[Restrictor] stopAppSwitches CALLED - no apps can launch!");
            } catch (Exception e) {
                System.out.println("[Restrictor] stopAppSwitches failed: " + e.getClass().getName() + ": " + e.getMessage());
            }
        } catch (Exception e) {
            System.out.println("[Restrictor] IActivityManager failed: " + e.getMessage());
        }

        // 3. Try to set lock_screen_only via Settings.Secure
        System.out.println("[Restrictor] Trying to enable lock screen immediately...");
        try {
            Class<?> secureClass = Class.forName("android.provider.Settings$Secure");
            Object contentResolver = ctx.getContentResolver();
            Method putInt = secureClass.getMethod("putInt",
                Class.forName("android.content.ContentResolver"),
                String.class, int.class);
            putInt.invoke(null, contentResolver, "power_button_instantly_locks_the_device", 1);
            System.out.println("[Restrictor] Power button instant lock SET");
        } catch (Exception e) {
            System.out.println("[Restrictor] Instant lock failed: " + e.getClass().getName() + ": " + e.getMessage());
        }

        // 4. Try to disable all user apps via PackageManager
        System.out.println("[Restrictor] Listing user-installed apps...");
        try {
            Object pm = ctx.getPackageManager();
            Method getInstalledPackages = pm.getClass().getMethod("getInstalledPackages", int.class);
            Object packageList = getInstalledPackages.invoke(pm, 0);
            int size = (Integer) packageList.getClass().getMethod("size").invoke(packageList);
            System.out.println("[Restrictor] Total packages: " + size);

            int disabled = 0;
            int skipped = 0;
            for (int i = 0; i < size; i++) {
                Object pi = packageList.getClass().getMethod("get", int.class).invoke(packageList, i);
                String pkgName = (String) pi.getClass().getField("packageName").get(pi);
                // Get applicationInfo safely
                Object appInfo = pi.getClass().getField("applicationInfo").get(pi);
                if (appInfo == null) { skipped++; continue; }
                int flags = 0;
                try {
                    flags = (Integer) appInfo.getClass().getField("flags").get(appInfo);
                } catch (Exception e) { skipped++; continue; }
                // FLAG_SYSTEM = 1 — skip system apps
                if ((flags & 1) != 0) { skipped++; continue; }
                if (pkgName.equals("com.android.launcher3")) { skipped++; continue; }
                // Try to disable user app
                try {
                    Method setAppEnabledSetting = pm.getClass().getMethod(
                        "setApplicationEnabledSetting", String.class, int.class, int.class);
                    // COMPONENT_ENABLED_STATE_DISABLED_USER = 3
                    setAppEnabledSetting.invoke(pm, pkgName, 3, 0);
                    disabled++;
                    if (disabled <= 10) System.out.println("[Restrictor]   disabled: " + pkgName);
                } catch (Exception e) {
                    // silently skip
                }
            }
            System.out.println("[Restrictor] Disabled " + disabled + " user apps (skipped " + skipped + " system)");
        } catch (Exception e) {
            System.out.println("[Restrictor] App disable failed: " + e.getClass().getName() + ": " + e.getMessage());
            if (e.getCause() != null) System.out.println("  cause: " + e.getCause());
        }
        return iamResult;
    }

    static void doRestore(Context ctx) {
        // 0. Resume app switches FIRST (before re-enabling apps)
        System.out.println("[Restrictor] Resuming app switches...");
        try {
            Class<?> amClass = Class.forName("android.app.ActivityManager");
            Object iam;
            try {
                iam = amClass.getMethod("getService").invoke(null);
            } catch (Exception e) {
                Class<?> amnClass = Class.forName("android.app.ActivityManagerNative");
                iam = amnClass.getMethod("getDefault").invoke(null);
            }
            Method resumeSwitches = iam.getClass().getMethod("resumeAppSwitches");
            resumeSwitches.invoke(iam);
            System.out.println("[Restrictor] resumeAppSwitches CALLED");
        } catch (Exception e) {
            System.out.println("[Restrictor] resumeAppSwitches failed: " + e.getMessage());
        }

        // 1. Restore screen timeout to default (30 seconds)
        System.out.println("[Restrictor] Restoring screen timeout...");
        try {
            Class<?> secureClass = Class.forName("android.provider.Settings$Secure");
            Object contentResolver = ctx.getContentResolver();
            Method putInt = secureClass.getMethod("putInt",
                Class.forName("android.content.ContentResolver"),
                String.class, int.class);
            putInt.invoke(null, contentResolver, "screen_off_timeout", 30000);
            System.out.println("[Restrictor] Screen timeout restored");
        } catch (Exception e) {
            System.out.println("[Restrictor] Restore timeout failed: " + e.getMessage());
        }

        // 2. Re-enable all user apps
        System.out.println("[Restrictor] Re-enabling user apps...");
        try {
            Object pm = ctx.getPackageManager();
            Method getInstalledPackages = pm.getClass().getMethod("getInstalledPackages", int.class);
            Object packageList = getInstalledPackages.invoke(pm, 0);
            int size = (Integer) packageList.getClass().getMethod("size").invoke(packageList);

            int enabled = 0;
            for (int i = 0; i < size; i++) {
                Object pi = packageList.getClass().getMethod("get", int.class).invoke(packageList, i);
                String pkgName = (String) pi.getClass().getField("packageName").get(pi);
                try {
                    Method setAppEnabledSetting = pm.getClass().getMethod(
                        "setApplicationEnabledSetting", String.class, int.class, int.class);
                    // COMPONENT_ENABLED_STATE_DEFAULT = 0
                    setAppEnabledSetting.invoke(pm, pkgName, 0, 0);
                    enabled++;
                } catch (Exception ignored) {}
            }
            System.out.println("[Restrictor] Re-enabled " + enabled + " apps");
        } catch (Exception e) {
            System.out.println("[Restrictor] Re-enable failed: " + e.getMessage());
        }
    }
}
