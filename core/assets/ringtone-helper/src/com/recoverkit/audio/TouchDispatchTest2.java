package com.recoverkit.audio;

import android.os.IBinder;
import android.os.Looper;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * Test 2: Comprehensive search for touch-dispatch control on Vivo Android 15.
 * - Fixes Looper.prepare() for systemMain
 * - Lists ALL methods on InputManager (including inherited)
 * - Searches for dispatch-related methods on related classes
 * - Tries InputManagerEx (OEM extension)
 */
public class TouchDispatchTest2 {

    public static void main(String[] args) {
        boolean disable = true;
        if (args.length > 0 && "enable".equals(args[0])) {
            disable = false;
        }

        System.err.println("UID: " + android.os.Process.myUid());
        System.err.println("Action: " + (disable ? "DISABLE" : "ENABLE"));

        // Must call Looper.prepare() before systemMain
        Looper.prepare();

        // Strategy 1: Fix systemMain + list ALL InputManager methods
        try {
            System.err.println("\n=== Strategy 1: systemMain with Looper ===");
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Method systemMain = atClass.getMethod("systemMain");
            Object activityThread = systemMain.invoke(null);
            Method getSystemContext = atClass.getMethod("getSystemContext");
            Object context = getSystemContext.invoke(activityThread);

            Method getSystemService = context.getClass().getMethod("getSystemService", String.class);
            Object inputManager = getSystemService.invoke(context, "input");
            System.err.println("InputManager class: " + inputManager.getClass().getName());

            // List ALL methods (including inherited)
            System.err.println("\n--- ALL InputManager methods ---");
            Method[] allMethods = inputManager.getClass().getMethods();
            for (Method m : allMethods) {
                String name = m.getName().toLowerCase();
                if (name.contains("dispatch") || name.contains("enable") ||
                    name.contains("disable") || name.contains("frozen") ||
                    name.contains("freeze") || name.contains("touch") ||
                    name.contains("event") || name.contains("input")) {
                    System.err.println("  " + m);
                }
            }

            // Also check declared methods (private/hidden)
            System.err.println("\n--- Declared (hidden) methods ---");
            Method[] declared = inputManager.getClass().getDeclaredMethods();
            for (Method m : declared) {
                System.err.println("  " + m);
            }

        } catch (Exception e) {
            System.err.println("Strategy 1 FAILED: " + e);
            e.printStackTrace(System.err);
        }

        // Strategy 2: Search for InputManagerEx or OEM extensions
        try {
            System.err.println("\n=== Strategy 2: OEM extensions ===");
            String[] candidates = {
                "android.hardware.input.InputManagerEx",
                "com.vivo.input.InputManagerExt",
                "com.android.server.input.InputManagerService",
                "android.view.InputMonitor",
            };
            for (String className : candidates) {
                try {
                    Class<?> cls = Class.forName(className);
                    System.err.println("Found: " + className);
                    Method[] methods = cls.getDeclaredMethods();
                    for (Method m : methods) {
                        String name = m.getName().toLowerCase();
                        if (name.contains("dispatch") || name.contains("enable") ||
                            name.contains("disable") || name.contains("freeze") ||
                            name.contains("touch")) {
                            System.err.println("  " + m);
                        }
                    }
                } catch (ClassNotFoundException e) {
                    System.err.println("Not found: " + className);
                }
            }
        } catch (Exception e) {
            System.err.println("Strategy 2 FAILED: " + e);
        }

        // Strategy 3: Try to get the native InputManager via JNI
        // The native method might still exist even if Java wrapper was removed
        try {
            System.err.println("\n=== Strategy 3: Native InputManager via reflection ===");
            Class<?> imClass = Class.forName("android.hardware.input.InputManager");
            
            // Check for native methods
            Field[] fields = imClass.getDeclaredFields();
            System.err.println("InputManager fields:");
            for (Field f : fields) {
                System.err.println("  " + f);
            }

            // Look for mNativeInputManagerPtr or similar
            for (Field f : fields) {
                if (f.getName().contains("native") || f.getName().contains("Native") ||
                    f.getName().contains("ptr") || f.getName().contains("Ptr")) {
                    f.setAccessible(true);
                    System.err.println("Found native field: " + f.getName() + " = " + f.get(null));
                }
            }
        } catch (Exception e) {
            System.err.println("Strategy 3 FAILED: " + e);
        }

        // Strategy 4: Try service call with different approach
        // Maybe the native inputflinger service has a different interface
        try {
            System.err.println("\n=== Strategy 4: Check inputflinger native service ===");
            // Get the inputflinger binder
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Method getService = smClass.getMethod("getService", String.class);
            
            // Try inputflinger
            IBinder flinger = (IBinder) getService.invoke(null, "inputflinger");
            System.err.println("inputflinger binder: " + flinger);
            System.err.println("inputflinger descriptor: " + flinger.getInterfaceDescriptor());
            
            // Try input
            IBinder inputBinder = (IBinder) getService.invoke(null, "input");
            System.err.println("input binder: " + inputBinder);
            System.err.println("input descriptor: " + inputBinder.getInterfaceDescriptor());
            
        } catch (Exception e) {
            System.err.println("Strategy 4 FAILED: " + e);
        }

        System.err.println("\nDone.");
    }
}
