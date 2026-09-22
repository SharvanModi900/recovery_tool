package com.recoverkit.audio;

import android.content.Context;
import android.os.Build;
import android.os.Looper;
import android.os.VibrationEffect;
import android.os.Vibrator;
import java.io.File;
import java.io.FileWriter;
import java.util.Timer;
import java.util.TimerTask;

/**
 * Continuous vibration — uses java.util.Timer instead of Thread
 * because app_process on Android 15 silently blocks Thread.start().
 * Timer uses its own internal thread creation mechanism.
 */
public class VibratorRunner {

    private static final String STOP_FLAG = "/data/local/tmp/recoverkit_stop_vibrate";
    private static final String DEBUG_LOG = "/data/local/tmp/recoverkit_vibrate_debug.log";
    private static final long BURST_MS = 400;
    private static final long GAP_MS = 100;

    static FileWriter dbg;

    static void log(String msg) {
        try {
            if (dbg == null) dbg = new FileWriter(DEBUG_LOG, true);
            dbg.write(msg + "\n");
            dbg.flush();
        } catch (Exception e) { /* ignore */ }
        System.out.println(msg);
        System.out.flush();
    }

    public static void main(String[] args) {
        try {
            new File(DEBUG_LOG).delete();
            new File(STOP_FLAG).delete();

            log("[RecoverKit] Starting...");

            Looper.prepareMainLooper();

            // Get system context
            Class<?> atClass = Class.forName("android.app.ActivityThread");
            Object at = atClass.getMethod("systemMain").invoke(null);
            Context ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
            log("[RecoverKit] Context: " + ctx);

            // Get vibrator
            Vibrator v = null;
            if (Build.VERSION.SDK_INT >= 31) {
                try {
                    Object vm = ctx.getSystemService("vibrator_manager");
                    if (vm != null) {
                        v = (Vibrator) vm.getClass().getMethod("getDefaultVibrator").invoke(vm);
                        log("[RecoverKit] Vibrator via VibratorManager");
                    }
                } catch (Exception e) {
                    log("[RecoverKit] VibratorManager err: " + e.getMessage());
                }
            }
            if (v == null) {
                v = (Vibrator) ctx.getSystemService(Context.VIBRATOR_SERVICE);
            }
            if (v == null) { log("[RecoverKit] ERROR: no vibrator"); System.exit(1); return; }

            final Vibrator fv = v;
            final VibrationEffect effect;
            if (Build.VERSION.SDK_INT >= 26) {
                effect = VibrationEffect.createOneShot(BURST_MS, VibrationEffect.DEFAULT_AMPLITUDE);
            } else {
                effect = null;
            }
            final File stopFile = new File(STOP_FLAG);

            log("[RecoverKit] Creating Timer...");

            final int[] count = {0};
            Timer timer = new Timer("RK-Vibrate", false);
            timer.scheduleAtFixedRate(new TimerTask() {
                @Override
                public void run() {
                    if (stopFile.exists()) {
                        try { fv.cancel(); } catch (Exception ignored) {}
                        stopFile.delete();
                        log("[RecoverKit] Stopped after " + count[0] + " bursts.");
                        System.exit(0);
                        return;
                    }

                    try {
                        if (effect != null) {
                            fv.vibrate(effect);
                        } else {
                            fv.vibrate(BURST_MS);
                        }
                    } catch (Exception e) {
                        log("[RecoverKit] vibrate() err: " + e.getMessage());
                    }

                    count[0]++;
                    if (count[0] % 10 == 1) {
                        log("[RecoverKit] burst #" + count[0]);
                    }
                }
            }, 0, BURST_MS + GAP_MS);

            log("[RecoverKit] Timer scheduled, entering Looper...");

            // Keep process alive
            Looper.loop();

        } catch (Exception e) {
            log("[RecoverKit] FATAL: " + e.getClass().getName() + ": " + e.getMessage());
            System.exit(1);
        }
    }
}
