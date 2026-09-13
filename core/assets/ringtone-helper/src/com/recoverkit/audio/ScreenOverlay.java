package com.recoverkit.audio;

import android.content.Context;
import android.graphics.Color;
import android.graphics.PixelFormat;
import android.os.Looper;
import android.view.Gravity;
import android.view.MotionEvent;
import android.view.View;
import android.view.WindowManager;
import android.widget.TextView;
import java.io.File;
import java.lang.reflect.Method;

/**
 * Draws a fullscreen overlay on top of EVERYTHING using system context.
 * This blocks all touch events and hides the home screen/apps.
 */
public class ScreenOverlay {
    private static final String UNLOCK_SIG = "/data/local/tmp/recoverkit_unlock_signal";

    public static void main(String[] args) {
        try {
            Looper.prepareMainLooper();

            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object at = atClass.getMethod("systemMain").invoke(null);
            Context ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            System.out.println("[Overlay] System context: " + ctx.getPackageName());

            String action = args.length > 0 ? args[0] : "show";

            if ("show".equals(action)) {
                showOverlay(ctx);
                Looper.loop();
            } else if ("hide".equals(action)) {
                // Signal the show process to exit
                new File(UNLOCK_SIG).createNewFile();
                System.out.println("[Overlay] Hide signal sent");
            }

        } catch (Exception e) {
            System.out.println("[Overlay] ERROR: " + e.getMessage());
            e.printStackTrace();
        }
    }

    static void showOverlay(Context ctx) {
        try {
            // Try to get IWindowManager directly via ServiceManager (bypass Context UID check)
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Object wmBinder = smClass.getMethod("getService", String.class).invoke(null, "window");
            System.out.println("[Overlay] Got IWindowManager binder: " + wmBinder);

            // Create IWindowManager proxy via Stub.asInterface()
            Class<?> iwmClass = Class.forName("android.view.IWindowManager$Stub");
            Object iwm = iwmClass.getMethod("asInterface", Class.forName("android.os.IBinder")).invoke(null, wmBinder);
            System.out.println("[Overlay] Got IWindowManager proxy: " + iwm);

            // Create fullscreen black view
            View overlay = new View(ctx);
            overlay.setBackgroundColor(Color.BLACK);

            // Block ALL touch events
            overlay.setOnTouchListener(new View.OnTouchListener() {
                @Override
                public boolean onTouch(View v, MotionEvent event) {
                    return true; // consume all touches
                }
            });

            // Try TYPE_TOAST (2) first - less restrictive than TYPE_SYSTEM_OVERLAY
            // If that fails, try TYPE_SYSTEM_ERROR (11)
            int[] windowTypes = {2, 11, 2038}; // TYPE_TOAST, TYPE_SYSTEM_ERROR, TYPE_APPLICATION_OVERLAY
            String[] typeNames = {"TYPE_TOAST", "TYPE_SYSTEM_ERROR", "TYPE_APPLICATION_OVERLAY"};

            WindowManager.LayoutParams params = new WindowManager.LayoutParams(
                WindowManager.LayoutParams.MATCH_PARENT,
                WindowManager.LayoutParams.MATCH_PARENT,
                2, // TYPE_TOAST
                WindowManager.LayoutParams.FLAG_NOT_FOCUSABLE
                    | WindowManager.LayoutParams.FLAG_LAYOUT_IN_SCREEN
                    | WindowManager.LayoutParams.FLAG_SHOW_WHEN_LOCKED
                    | WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON
                    | WindowManager.LayoutParams.FLAG_FULLSCREEN,
                PixelFormat.OPAQUE
            );
            params.gravity = Gravity.CENTER;
            params.setTitle("RecoverKit Lock");

            // Try using IWindowManager.addWindow directly
            try {
                // IWindowManager.addWindow(IWindowSession, IWindow, int, LayoutParams, Rect, Rect, Rect, Configuration, Resources)
                // This is complex, let's try the simpler approach first
                
                // Try using the regular WindowManager but with TYPE_TOAST
                WindowManager regularWm = (WindowManager) ctx.getSystemService(Context.WINDOW_SERVICE);
                regularWm.addView(overlay, params);
                System.out.println("[Overlay] TYPE_TOAST overlay ADDED successfully!");
            } catch (Exception e) {
                System.out.println("[Overlay] TYPE_TOAST failed: " + e.getMessage());
                
                // Try TYPE_SYSTEM_ERROR
                params.type = 11;
                try {
                    WindowManager regularWm = (WindowManager) ctx.getSystemService(Context.WINDOW_SERVICE);
                    regularWm.addView(overlay, params);
                    System.out.println("[Overlay] TYPE_SYSTEM_ERROR overlay ADDED!");
                } catch (Exception e2) {
                    System.out.println("[Overlay] TYPE_SYSTEM_ERROR also failed: " + e2.getMessage());
                    
                    // Try via IWindowManager directly
                    try {
                        // IWindowManager.addWindow takes many params, let's use reflection
                        Method addWindow = iwm.getClass().getMethod("addWindow",
                            Class.forName("android.view.IWindowSession"),
                            Class.forName("android.view.IWindow"),
                            int.class,
                            WindowManager.LayoutParams.class,
                            Class.forName("android.graphics.Rect"),
                            Class.forName("android.graphics.Rect"),
                            Class.forName("android.graphics.Rect"),
                            Class.forName("android.content.res.Configuration"),
                            Class.forName("android.content.res.Resources"));
                        System.out.println("[Overlay] Found addWindow method, but need IWindowSession...");
                    } catch (Exception e3) {
                        System.out.println("[Overlay] IWindowManager.addWindow not accessible: " + e3.getMessage());
                    }
                }
            }

            // Watch for unlock signal and remove overlay
            final View overlayRef = overlay;
            new Thread(new Runnable() {
                @Override
                public void run() {
                    while (true) {
                        if (new File(UNLOCK_SIG).exists()) {
                            System.out.println("[Overlay] Unlock signal detected, removing overlay");
                            new File(UNLOCK_SIG).delete();
                            try {
                                WindowManager rwm = (WindowManager) ctx.getSystemService(Context.WINDOW_SERVICE);
                                rwm.removeView(overlayRef);
                                System.out.println("[Overlay] Overlay REMOVED");
                            } catch (Exception e) {
                                System.out.println("[Overlay] Remove failed: " + e.getMessage());
                            }
                            Looper.myLooper().quit();
                            return;
                        }
                        try { Thread.sleep(2000); } catch (Exception ignored) {}
                    }
                }
            }).start();
            System.out.println("[Overlay] Watchdog thread started");

        } catch (Exception e) {
            System.out.println("[Overlay] Failed: " + e.getClass().getName() + ": " + e.getMessage());
            if (e.getCause() != null) System.out.println("  cause: " + e.getCause());
        }
    }
}
