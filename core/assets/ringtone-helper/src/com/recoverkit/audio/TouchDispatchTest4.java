package com.recoverkit.audio;

import android.os.IBinder;
import android.os.Looper;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * Test 4: Investigate IVivoInputManagerGlobal — Vivo's global input control.
 * Also try disableInputDevice directly via Java API (might work from system context).
 */
public class TouchDispatchTest4 {

    public static void main(String[] args) {
        boolean disable = true;
        if (args.length > 0 && "enable".equals(args[0])) {
            disable = false;
        }

        System.err.println("UID: " + android.os.Process.myUid());
        System.err.println("Action: " + (disable ? "DISABLE" : "ENABLE"));
        Looper.prepare();

        try {
            // Get system context
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Method systemMain = atClass.getMethod("systemMain");
            Object activityThread = systemMain.invoke(null);
            Method getSystemContext = atClass.getMethod("getSystemContext");
            Object context = getSystemContext.invoke(activityThread);

            // Get InputManagerGlobal
            Class<?> globalClass = Class.forName("android.hardware.input.InputManagerGlobal");
            Method getInstance = globalClass.getMethod("getInstance");
            Object global = getInstance.invoke(null);
            System.err.println("InputManagerGlobal: " + global);

            // Get IVivoInputManagerGlobal
            Method getVivo = globalClass.getMethod("getVivoInjectInstance");
            Object vivoGlobal = getVivo.invoke(global);
            System.err.println("VivoInputManagerGlobal: " + vivoGlobal);

            if (vivoGlobal != null) {
                Class<?> vivoCls = vivoGlobal.getClass();
                System.err.println("VivoInputManagerGlobal class: " + vivoCls.getName());

                // List ALL methods
                System.err.println("\n--- ALL VivoInputManagerGlobal methods ---");
                Method[] allMethods = vivoCls.getMethods();
                for (Method m : allMethods) {
                    System.err.println("  " + m);
                }

                // List declared methods
                System.err.println("\n--- Declared methods ---");
                Method[] declared = vivoCls.getDeclaredMethods();
                for (Method m : declared) {
                    System.err.println("  " + m);
                }

                // List fields
                System.err.println("\n--- Fields ---");
                Field[] fields = vivoCls.getDeclaredFields();
                for (Field f : fields) {
                    System.err.println("  " + f);
                }

                // List interfaces
                System.err.println("\n--- Interfaces ---");
                Class<?>[] interfaces = vivoCls.getInterfaces();
                for (Class<?> iface : interfaces) {
                    System.err.println("  " + iface.getName());
                    Method[] ifaceMethods = iface.getDeclaredMethods();
                    for (Method m : ifaceMethods) {
                        System.err.println("    " + m);
                    }
                }
            }

            // Now try disableInputDevice for ALL devices
            System.err.println("\n=== Trying disableInputDevice ===");
            Method getSystemService = context.getClass().getMethod("getSystemService", String.class);
            Object inputManager = getSystemService.invoke(context, "input");
            
            Method getDeviceIds = inputManager.getClass().getMethod("getInputDeviceIds");
            int[] deviceIds = (int[]) getDeviceIds.invoke(inputManager);
            System.err.println("Found " + deviceIds.length + " input devices");

            Method getInputDevice = inputManager.getClass().getMethod("getInputDevice", int.class);
            Method disableDevice = inputManager.getClass().getMethod("disableInputDevice", int.class);
            Method enableDevice = inputManager.getClass().getMethod("enableInputDevice", int.class);

            for (int id : deviceIds) {
                try {
                    Object device = getInputDevice.invoke(inputManager, id);
                    String name = "unknown";
                    if (device != null) {
                        Method getName = device.getClass().getMethod("getName");
                        name = (String) getName.invoke(device);
                    }

                    if (disable) {
                        disableDevice.invoke(inputManager, id);
                        System.err.println("DISABLED device " + id + " (" + name + ") — SUCCESS!");
                    } else {
                        enableDevice.invoke(inputManager, id);
                        System.err.println("ENABLED device " + id + " (" + name + ") — SUCCESS!");
                    }
                } catch (Exception e) {
                    Throwable cause = e.getCause() != null ? e.getCause() : e;
                    System.err.println("Device " + id + " FAILED: " + cause.getClass().getName() + ": " + cause.getMessage());
                }
            }

            // Check result
            Thread.sleep(500);
            System.err.println("\n=== Checking dispatch state ===");
            // Can't run dumpsys from here, but let's verify by checking device state
            for (int id : deviceIds) {
                try {
                    Object device = getInputDevice.invoke(inputManager, id);
                    if (device != null) {
                        Method getName = device.getClass().getMethod("getName");
                        String name = (String) getName.invoke(device);
                        System.err.println("Device " + id + " (" + name + ") still present");
                    } else {
                        System.err.println("Device " + id + " is GONE (disabled)");
                    }
                } catch (Exception e) {
                    System.err.println("Device " + id + " check failed: " + e.getMessage());
                }
            }

        } catch (Exception e) {
            System.err.println("FAILED: " + e);
            e.printStackTrace(System.err);
        }

        System.err.println("\nDone. Run: adb shell dumpsys input | grep DispatchEnabled");
    }
}
