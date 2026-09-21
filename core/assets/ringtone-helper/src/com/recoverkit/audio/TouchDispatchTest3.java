package com.recoverkit.audio;

import android.os.IBinder;
import android.os.Looper;
import java.lang.reflect.Field;
import java.lang.reflect.Method;

/**
 * Test 3: Investigate IVivoInputManager and InputManagerGlobal
 * for Vivo-specific touch dispatch control on Android 15.
 */
public class TouchDispatchTest3 {

    public static void main(String[] args) {
        System.err.println("UID: " + android.os.Process.myUid());
        Looper.prepare();

        // Strategy 1: IVivoInputManager — Vivo's custom input interface
        try {
            System.err.println("\n=== Strategy 1: IVivoInputManager ===");
            Class<?> vivoClass = Class.forName("android.hardware.input.IVivoInputManager");
            System.err.println("Found IVivoInputManager!");
            
            // List all methods
            Method[] methods = vivoClass.getDeclaredMethods();
            System.err.println("Methods (" + methods.length + "):");
            for (Method m : methods) {
                System.err.println("  " + m);
            }
            
            // Also check the Stub inner class
            try {
                Class<?> stubClass = Class.forName("android.hardware.input.IVivoInputManager$Stub");
                System.err.println("\nStub methods:");
                Method[] stubMethods = stubClass.getDeclaredMethods();
                for (Method m : stubMethods) {
                    System.err.println("  " + m);
                }
            } catch (Exception e) {
                System.err.println("No Stub class: " + e.getMessage());
            }
            
        } catch (ClassNotFoundException e) {
            System.err.println("IVivoInputManager not found as class");
            // Try as interface on InputManager
            try {
                System.err.println("Trying to get IVivoInputManager from InputManager instance...");
                Class<?> imClass = Class.forName("android.hardware.input.InputManager");
                Field vivoField = null;
                for (Field f : imClass.getDeclaredFields()) {
                    if (f.getName().contains("Vivo") || f.getName().contains("vivo")) {
                        System.err.println("Found field: " + f);
                        vivoField = f;
                    }
                }
                
                if (vivoField != null) {
                    // Get the InputManager instance
                    Method getInstance = imClass.getMethod("getInstance");
                    Object imInstance = getInstance.invoke(null);
                    
                    vivoField.setAccessible(true);
                    Object vivoManager = vivoField.get(imInstance);
                    System.err.println("VivoInputManager instance: " + vivoManager);
                    
                    if (vivoManager != null) {
                        Class<?> vivoCls = vivoManager.getClass();
                        System.err.println("VivoInputManager class: " + vivoCls.getName());
                        
                        Method[] vMethods = vivoCls.getMethods();
                        System.err.println("VivoInputManager methods (" + vMethods.length + "):");
                        for (Method m : vMethods) {
                            System.err.println("  " + m);
                        }
                    }
                }
            } catch (Exception e2) {
                System.err.println("Failed to get VivoInputManager: " + e2);
                e2.printStackTrace(System.err);
            }
        }

        // Strategy 2: InputManagerGlobal — might have dispatch control
        try {
            System.err.println("\n=== Strategy 2: InputManagerGlobal ===");
            Class<?> globalClass = Class.forName("android.hardware.input.InputManagerGlobal");
            System.err.println("Found InputManagerGlobal!");
            
            // List all methods
            Method[] methods = globalClass.getDeclaredMethods();
            System.err.println("Methods (" + methods.length + "):");
            for (Method m : methods) {
                System.err.println("  " + m);
            }
            
            // List fields
            Field[] fields = globalClass.getDeclaredFields();
            System.err.println("Fields:");
            for (Field f : fields) {
                System.err.println("  " + f);
            }
            
        } catch (ClassNotFoundException e) {
            System.err.println("InputManagerGlobal not found");
        }

        // Strategy 3: Try to get and use IVivoInputManager
        try {
            System.err.println("\n=== Strategy 3: Use IVivoInputManager ===");
            
            // Get system context
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Method systemMain = atClass.getMethod("systemMain");
            Object activityThread = systemMain.invoke(null);
            Method getSystemContext = atClass.getMethod("getSystemContext");
            Object context = getSystemContext.invoke(activityThread);
            
            // Get InputManager
            Method getSystemService = context.getClass().getMethod("getSystemService", String.class);
            Object inputManager = getSystemService.invoke(context, "input");
            
            // Get the VivoInputManager field
            Class<?> imClass = inputManager.getClass();
            for (Field f : imClass.getDeclaredFields()) {
                if (f.getName().contains("Vivo") || f.getName().contains("vivo")) {
                    f.setAccessible(true);
                    Object vivoManager = f.get(inputManager);
                    System.err.println("VivoInputManager: " + vivoManager);
                    
                    if (vivoManager != null) {
                        // Try to call methods on it
                        Class<?> vivoCls = vivoManager.getClass();
                        System.err.println("VivoInputManager class: " + vivoCls.getName());
                        
                        // Get interface methods
                        Class<?>[] interfaces = vivoCls.getInterfaces();
                        for (Class<?> iface : interfaces) {
                            System.err.println("Implements: " + iface.getName());
                            Method[] ifaceMethods = iface.getDeclaredMethods();
                            for (Method m : ifaceMethods) {
                                System.err.println("  " + m);
                            }
                        }
                        
                        // Try all methods
                        Method[] allMethods = vivoCls.getMethods();
                        for (Method m : allMethods) {
                            String name = m.getName().toLowerCase();
                            if (name.contains("dispatch") || name.contains("enable") ||
                                name.contains("disable") || name.contains("touch") ||
                                name.contains("freeze") || name.contains("block") ||
                                name.contains("set") || name.contains("input")) {
                                System.err.println("Interesting method: " + m);
                            }
                        }
                    }
                }
            }
        } catch (Exception e) {
            System.err.println("Strategy 3 FAILED: " + e);
            e.printStackTrace(System.err);
        }

        System.err.println("\nDone.");
    }
}
