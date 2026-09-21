package com.recoverkit.audio;

import android.os.IBinder;
import java.lang.reflect.Method;

/**
 * Test: call InputManager.setEventDispatchDisabled via Java reflection.
 * This uses the HIDDEN Java API (JNI → nativeSetEventDispatchEnabled),
 * NOT the AIDL binder transaction. Different code path — might bypass
 * the permission check that blocks service call input 5.
 *
 * Usage:
 *   CLASSPATH=/data/local/tmp/recoverkit_test.dex app_process / com.recoverkit.audio.TouchDispatchTest disable
 *   CLASSPATH=/data/local/tmp/recoverkit_test.dex app_process / com.recoverkit.audio.TouchDispatchTest enable
 */
public class TouchDispatchTest {

    public static void main(String[] args) {
        boolean disable = true;
        if (args.length > 0 && "enable".equals(args[0])) {
            disable = false;
        }

        System.err.println("[TouchDispatchTest] UID: " + android.os.Process.myUid());
        System.err.println("[TouchDispatchTest] PID: " + android.os.Process.myPid());
        System.err.println("[TouchDispatchTest] Action: " + (disable ? "DISABLE" : "ENABLE"));

        // Strategy 1: Java reflection — InputManager.setEventDispatchDisabled(boolean)
        try {
            System.err.println("[TouchDispatchTest] Strategy 1: Java reflection on InputManager...");

            // Get system context
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Method systemMain = atClass.getMethod("systemMain");
            Object activityThread = systemMain.invoke(null);
            Method getSystemContext = atClass.getMethod("getSystemContext");
            Object context = getSystemContext.invoke(activityThread);
            System.err.println("[TouchDispatchTest] Got system context: " + context);

            // Get InputManager
            Method getSystemService = context.getClass().getMethod("getSystemService", String.class);
            Object inputManager = getSystemService.invoke(context, "input");
            System.err.println("[TouchDispatchTest] Got InputManager: " + inputManager);
            System.err.println("[TouchDispatchTest] InputManager class: " + inputManager.getClass().getName());

            // Call setEventDispatchDisabled
            Method setDispatch = inputManager.getClass().getMethod("setEventDispatchDisabled", boolean.class);
            System.err.println("[TouchDispatchTest] Found method: " + setDispatch);
            setDispatch.invoke(inputManager, disable);
            System.err.println("[TouchDispatchTest] SUCCESS: setEventDispatchDisabled(" + disable + ") called!");

        } catch (Exception e) {
            System.err.println("[TouchDispatchTest] Strategy 1 FAILED: " + e.getClass().getName() + ": " + e.getMessage());
            e.printStackTrace(System.err);
        }

        // Strategy 2: Direct native call via JNI bridge
        try {
            System.err.println("[TouchDispatchTest] Strategy 2: Direct native InputManager...");
            // The native method is: nativeSetEventDispatchEnabled(boolean)
            // It's accessed through the InputManager instance
            Class<?> imClass = Class.forName("android.hardware.input.InputManager");
            Method[] methods = imClass.getDeclaredMethods();
            System.err.println("[TouchDispatchTest] InputManager methods:");
            for (Method m : methods) {
                if (m.getName().toLowerCase().contains("dispatch") ||
                    m.getName().toLowerCase().contains("event")) {
                    System.err.println("[TouchDispatchTest]   " + m.getName() + " → " + m);
                }
            }
        } catch (Exception e) {
            System.err.println("[TouchDispatchTest] Strategy 2 FAILED: " + e.getMessage());
        }

        // Strategy 3: Try disableInputDevice via Java API for ALL input devices
        try {
            System.err.println("[TouchDispatchTest] Strategy 3: disableInputDevice via Java API...");
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Method systemMain = atClass.getMethod("systemMain");
            Object activityThread = systemMain.invoke(null);
            Method getSystemContext = atClass.getMethod("getSystemContext");
            Object context = getSystemContext.invoke(activityThread);

            Method getSystemService = context.getClass().getMethod("getSystemService", String.class);
            Object inputManager = getSystemService.invoke(context, "input");

            // Get all input device IDs
            Method getDeviceIds = inputManager.getClass().getMethod("getInputDeviceIds");
            int[] deviceIds = (int[]) getDeviceIds.invoke(inputManager);
            System.err.println("[TouchDispatchTest] Found " + deviceIds.length + " input devices");

            Method disableDevice = inputManager.getClass().getMethod("disableInputDevice", int.class);
            Method enableDevice = inputManager.getClass().getMethod("enableInputDevice", int.class);

            for (int id : deviceIds) {
                try {
                    // Get device name for logging
                    Method getInputDevice = inputManager.getClass().getMethod("getInputDevice", int.class);
                    Object device = getInputDevice.invoke(inputManager, id);
                    String name = "unknown";
                    if (device != null) {
                        Method getName = device.getClass().getMethod("getName");
                        name = (String) getName.invoke(device);
                    }

                    if (disable) {
                        disableDevice.invoke(inputManager, id);
                        System.err.println("[TouchDispatchTest] Disabled device " + id + " (" + name + ")");
                    } else {
                        enableDevice.invoke(inputManager, id);
                        System.err.println("[TouchDispatchTest] Enabled device " + id + " (" + name + ")");
                    }
                } catch (Exception e) {
                    System.err.println("[TouchDispatchTest] Device " + id + " failed: " + e.getCause());
                }
            }
        } catch (Exception e) {
            System.err.println("[TouchDispatchTest] Strategy 3 FAILED: " + e.getMessage());
        }

        System.err.println("[TouchDispatchTest] Done. Check 'dumpsys input | grep DispatchEnabled'");
    }
}
