package com.recoverkit.audio;

import android.content.Context;
import android.graphics.PixelFormat;
import android.os.Looper;
import android.view.Gravity;
import android.view.View;
import android.view.WindowManager;
import java.lang.reflect.Method;

/**
 * TouchBlocker: Creates a fullscreen overlay that consumes ALL touch events.
 * Tests multiple window types from shell UID 2000 to find one that works.
 *
 * Usage:
 *   CLASSPATH=...dex app_process / com.recoverkit.audio.TouchBlocker
 *   (exits when /data/local/tmp/recoverkit_unlock_signal appears)
 */
public class TouchBlocker {

    static final String UNLOCK_SIGNAL = "/data/local/tmp/recoverkit_unlock_signal";
    static final String BLOCKER_PID = "/data/local/tmp/recoverkit_blocker.pid";

    public static void main(String[] args) {
        System.err.println("[TouchBlocker] UID: " + android.os.Process.myUid());
        System.err.println("[TouchBlocker] PID: " + android.os.Process.myPid());

        // Write PID file
        try {
            java.io.FileWriter fw = new java.io.FileWriter(BLOCKER_PID);
            fw.write(String.valueOf(android.os.Process.myPid()));
            fw.close();
        } catch (Exception e) { /* ignore */ }
        new java.io.File(UNLOCK_SIGNAL).delete();

        // app_process already has a main looper — do NOT call Looper.prepare()
        // Just get the context and start testing

        Context context = null;
        try {
            // Same pattern as ScreenOverlay/AppRestrictor — works from app_process
            Looper.prepareMainLooper();
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object activityThread = atClass.getMethod("systemMain").invoke(null);
            Context sysCtx = (Context) atClass.getMethod("getSystemContext").invoke(activityThread);
            System.err.println("[TouchBlocker] System context package: " + sysCtx.getPackageName());
            
            // CRITICAL: Use shell's own package context so UID matches process UID 2000
            // System context has package "android" (UID 1000) but process is UID 2000
            // This mismatch causes "Killed" when adding windows
            context = sysCtx.createPackageContext("com.android.shell", 0);
            System.err.println("[TouchBlocker] Shell context: " + context.getClass().getName());
            System.err.println("[TouchBlocker] Shell package: " + context.getPackageName());
            System.err.println("[TouchBlocker] UID: " + android.os.Process.myUid());
        } catch (Exception e) {
            System.err.println("[TouchBlocker] Context init failed: " + e.getMessage());
            e.printStackTrace(System.err);
        }

        if (context == null) {
            System.err.println("[TouchBlocker] No context available, exiting");
            return;
        }

        // Test window types — ordered from safest (no perm needed) to most restricted
        int[][] windowTypes = {
            {1, 1},        // TYPE_APPLICATION (no special perm — safest to try first)
            {2038, 1},     // TYPE_APPLICATION_OVERLAY (needs SYSTEM_ALERT_WINDOW)
            {2002, 1},     // TYPE_SYSTEM_DIALOG
            {2027, 1},     // TYPE_ACCESSIBILITY_OVERLAY
            {2005, 1},     // TYPE_APPLICATION_MEDIA
            {1002, 1},     // TYPE_APPLICATION_PANEL
            {2006, 1},     // TYPE_APPLICATION_SUB_PANEL
        };

        String[] typeNames = {
            "TYPE_APPLICATION(1)", "TYPE_APPLICATION_OVERLAY(2038)",
            "TYPE_SYSTEM_DIALOG(2002)", "TYPE_ACCESSIBILITY_OVERLAY(2027)",
            "TYPE_APPLICATION_MEDIA(2005)", "TYPE_APPLICATION_PANEL(1002)",
            "TYPE_APPLICATION_SUB_PANEL(2006)",
        };

        WindowManager wm = (WindowManager) context.getSystemService(Context.WINDOW_SERVICE);
        
        for (int i = 0; i < windowTypes.length; i++) {
            int type = windowTypes[i][0];
            String name = typeNames[i];
            
            try {
                System.err.println("\n[TouchBlocker] Testing " + name + "...");
                
                // Create fullscreen touch-consuming view
                View blockerView = new View(context) {
                    @Override
                    public boolean onTouchEvent(android.view.MotionEvent event) {
                        return true; // Consume ALL touches
                    }
                };
                blockerView.setClickable(true);

                WindowManager.LayoutParams params = new WindowManager.LayoutParams();
                params.type = type;
                params.width = WindowManager.LayoutParams.MATCH_PARENT;
                params.height = WindowManager.LayoutParams.MATCH_PARENT;
                params.flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                        | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                        | WindowManager.LayoutParams.FLAG_LAYOUT_NO_LIMITS
                        | WindowManager.LayoutParams.FLAG_NOT_TOUCH_MODAL;
                params.format = PixelFormat.TRANSLUCENT;
                params.gravity = Gravity.CENTER;
                params.setTitle("TouchBlocker");

                wm.addView(blockerView, params);
                System.err.println("[TouchBlocker] *** SUCCESS with " + name + "! ***");
                System.err.println("[TouchBlocker] Overlay is ACTIVE — consuming all touches");

                // Wait for unlock signal
                while (!new java.io.File(UNLOCK_SIGNAL).exists()) {
                    Thread.sleep(1000);
                }

                System.err.println("[TouchBlocker] Unlock signal detected, removing overlay...");
                wm.removeView(blockerView);
                System.err.println("[TouchBlocker] Done.");
                new java.io.File(BLOCKER_PID).delete();
                return;

            } catch (Exception e) {
                String msg = e.getMessage();
                if (msg != null && msg.length() > 200) {
                    msg = msg.substring(0, 200) + "...";
                }
                System.err.println("[TouchBlocker] " + name + " FAILED: " + msg);
            }
        }

        System.err.println("\n[TouchBlocker] ALL window types failed from system context.");
        System.err.println("[TouchBlocker] Trying with shell package context...");

        // Try with shell's own package context
        try {
            Context shellCtx = context.createPackageContext("com.android.shell", 0);
            System.err.println("[TouchBlocker] Shell context package: " + shellCtx.getPackageName());
            
            WindowManager wm2 = (WindowManager) shellCtx.getSystemService(Context.WINDOW_SERVICE);
            
            // Try TYPE_APPLICATION_OVERLAY with shell context
            View blockerView = new View(shellCtx) {
                @Override
                public boolean onTouchEvent(android.view.MotionEvent event) {
                    return true;
                }
            };
            blockerView.setClickable(true);

            WindowManager.LayoutParams params = new WindowManager.LayoutParams();
            params.type = 2038; // TYPE_APPLICATION_OVERLAY
            params.width = WindowManager.LayoutParams.MATCH_PARENT;
            params.height = WindowManager.LayoutParams.MATCH_PARENT;
            params.flags = WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                    | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN;
            params.format = PixelFormat.TRANSLUCENT;
            params.gravity = Gravity.CENTER;

            wm2.addView(blockerView, params);
            System.err.println("[TouchBlocker] *** SUCCESS with shell context + TYPE_APPLICATION_OVERLAY! ***");

            while (!new java.io.File(UNLOCK_SIGNAL).exists()) {
                Thread.sleep(1000);
            }
            wm2.removeView(blockerView);
            new java.io.File(BLOCKER_PID).delete();
            return;

        } catch (Exception e) {
            System.err.println("[TouchBlocker] Shell context also failed: " + e.getMessage());
        }

        System.err.println("[TouchBlocker] All approaches exhausted.");
        new java.io.File(BLOCKER_PID).delete();
    }
}
