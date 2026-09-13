package com.recoverkit.audio;

import android.content.Context;
import android.os.IBinder;
import android.os.Looper;
import java.lang.reflect.Method;

/**
 * Tests system-context APIs for device lockdown on Android 15.
 * Tries: goToSleep, setInteractive, freezeDisplay, etc.
 */
public class SystemApiTest {
    public static void main(String[] args) {
        try {
            Looper.prepareMainLooper();

            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object at = atClass.getMethod("systemMain").invoke(null);
            Context ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            System.out.println("[Test] System context: " + ctx.getPackageName());

            // 1. Test goToSleep with detailed error
            System.out.println("\n=== goToSleep ===");
            try {
                Object pm = ctx.getSystemService("power");
                System.out.println("  PowerManager: " + pm);
                Method goToSleep = pm.getClass().getMethod("goToSleep", long.class);
                goToSleep.invoke(pm, System.currentTimeMillis());
                System.out.println("  SUCCESS!");
            } catch (Exception e) {
                System.out.println("  FAILED: " + e.getClass().getName() + ": " + e.getMessage());
                if (e.getCause() != null) {
                    System.out.println("  Cause: " + e.getCause().getClass().getName() + ": " + e.getCause().getMessage());
                }
            }

            // 2. Test IInputManager.setInteractive
            System.out.println("\n=== IInputManager.setInteractive ===");
            try {
                Class<?> smClass = Class.forName("android.os.ServiceManager");
                IBinder binder = (IBinder) smClass.getMethod("getService", String.class).invoke(null, "input");
                System.out.println("  input binder: " + binder);
                if (binder != null) {
                    Class<?> stubClass = Class.forName("android.hardware.input.IInputManager$Stub");
                    Object iim = stubClass.getMethod("asInterface", Class.forName("android.os.IBinder")).invoke(null, binder);
                    System.out.println("  IInputManager: " + iim);

                    // List methods
                    for (Method m : iim.getClass().getMethods()) {
                        if (m.getName().contains("Interactive") || m.getName().contains("Touch") || m.getName().contains("Input")) {
                            System.out.println("  method: " + m.getName() + " -> " + java.util.Arrays.toString(m.getParameterTypes()));
                        }
                    }

                    // Try setInteractive(false)
                    try {
                        Method setInteractive = iim.getClass().getMethod("setInteractive", boolean.class);
                        setInteractive.invoke(iim, false);
                        System.out.println("  setInteractive(false) SUCCESS!");
                    } catch (Exception e) {
                        System.out.println("  setInteractive FAILED: " + e.getClass().getName() + ": " + e.getMessage());
                        if (e.getCause() != null) {
                            System.out.println("  Cause: " + e.getCause().getClass().getName() + ": " + e.getCause().getMessage());
                        }
                    }
                }
            } catch (Exception e) {
                System.out.println("  FAILED: " + e.getMessage());
            }

            // 3. Test IWindowManager methods
            System.out.println("\n=== IWindowManager ===");
            try {
                Class<?> smClass = Class.forName("android.os.ServiceManager");
                IBinder binder = (IBinder) smClass.getMethod("getService", String.class).invoke(null, "window");
                System.out.println("  window binder: " + binder);
                if (binder != null) {
                    Class<?> stubClass = Class.forName("android.view.IWindowManager$Stub");
                    Object iwm = stubClass.getMethod("asInterface", Class.forName("android.os.IBinder")).invoke(null, binder);
                    System.out.println("  IWindowManager: " + iwm);

                    // List interesting methods
                    for (Method m : iwm.getClass().getMethods()) {
                        String name = m.getName();
                        if (name.contains("Freeze") || name.contains("Display") || name.contains("Event") || name.contains("Input") || name.contains("Lock")) {
                            System.out.println("  method: " + name + " -> " + java.util.Arrays.toString(m.getParameterTypes()));
                        }
                    }
                }
            } catch (Exception e) {
                System.out.println("  FAILED: " + e.getMessage());
            }

            // 4. Test PowerManager.wakeUp
            System.out.println("\n=== wakeUp ===");
            try {
                Object pm = ctx.getSystemService("power");
                Method wakeUp = pm.getClass().getMethod("wakeUp", long.class);
                wakeUp.invoke(pm, System.currentTimeMillis());
                System.out.println("  SUCCESS!");
            } catch (Exception e) {
                System.out.println("  FAILED: " + e.getClass().getName() + ": " + e.getMessage());
                if (e.getCause() != null) {
                    System.out.println("  Cause: " + e.getCause().getClass().getName() + ": " + e.getCause().getMessage());
                }
            }

        } catch (Exception e) {
            System.out.println("[Test] ERROR: " + e.getMessage());
            e.printStackTrace();
        }
    }
}
