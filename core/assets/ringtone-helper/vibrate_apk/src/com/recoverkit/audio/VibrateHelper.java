package com.recoverkit.audio;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.os.VibrationEffect;
import android.os.Vibrator;
import android.os.VibratorManager;
import android.util.Log;

import java.io.File;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * BroadcastReceiver that triggers continuous device vibration.
 *
 * Start: am broadcast -a com.recoverkit.action.VIBRATE_START
 * Stop:  am broadcast -a com.recoverkit.action.VIBRATE_STOP
 *
 * Runs vibrate() on a background thread to satisfy the framework's
 * checkNotMainThread() requirement.  Each burst is a 300 ms one-shot
 * vibration followed by a 100 ms gap, repeated until stopped.
 */
public class VibrateHelper extends BroadcastReceiver {

    private static final String TAG = "RK-Vibrate";
    private static final String ACTION_START = "com.recoverkit.action.VIBRATE_START";
    private static final String ACTION_STOP  = "com.recoverkit.action.VIBRATE_STOP";
    private static final String STOP_FLAG    = "/data/local/tmp/recoverkit_stop_vibrate";

    private static volatile boolean sRunning = false;
    private static final AtomicBoolean sStopFlag = new AtomicBoolean(false);
    private static ExecutorService sExecutor;

    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null || intent.getAction() == null) return;
        String action = intent.getAction();
        Log.i(TAG, "onReceive: " + action);

        if (ACTION_STOP.equals(action)) {
            sStopFlag.set(true);
            sRunning = false;
            if (sExecutor != null) {
                sExecutor.shutdownNow();
                sExecutor = null;
            }
            // Also cancel any active vibration immediately
            try {
                VibratorManager vm = (VibratorManager)
                    context.getApplicationContext()
                        .getSystemService(Context.VIBRATOR_MANAGER_SERVICE);
                vm.getDefaultVibrator().cancel();
            } catch (Exception e) {
                Log.w(TAG, "cancel error: " + e.getMessage());
            }
            return;
        }

        if (ACTION_START.equals(action)) {
            if (sRunning) {
                Log.i(TAG, "Already running, ignoring duplicate start");
                return;
            }
            sRunning = true;
            sStopFlag.set(false);

            // Remove stale stop-flag file (may exist from old DEX approach)
            try { new File(STOP_FLAG).delete(); } catch (Exception ignored) {}

            final Context appCtx = context.getApplicationContext();
            sExecutor = Executors.newSingleThreadExecutor();
            sExecutor.execute(new Runnable() {
                @Override
                public void run() {
                    Log.i(TAG, "Vibration thread started");
                    try {
                        VibratorManager vm = (VibratorManager)
                            appCtx.getSystemService(Context.VIBRATOR_MANAGER_SERVICE);
                        Vibrator v = vm.getDefaultVibrator();

                        // 300ms burst at default amplitude
                        VibrationEffect effect =
                            VibrationEffect.createOneShot(300, VibrationEffect.DEFAULT_AMPLITUDE);

                        int count = 0;
                        while (!sStopFlag.get()) {
                            // Also check the file-based stop flag (for Rust-side stop)
                            if (new File(STOP_FLAG).exists()) {
                                Log.i(TAG, "Stop-flag file detected, exiting");
                                break;
                            }
                            try {
                                v.vibrate(effect);
                                count++;
                                if (count % 10 == 1) {
                                    Log.i(TAG, "Vibrated " + count + " times");
                                }
                                // Wait for burst duration + small gap
                                Thread.sleep(400);
                            } catch (InterruptedException ie) {
                                Log.i(TAG, "Sleep interrupted, exiting loop");
                                break;
                            } catch (Exception ve) {
                                Log.w(TAG, "vibrate error: " + ve.getMessage());
                                try { Thread.sleep(500); } catch (InterruptedException ignored) { break; }
                            }
                        }
                        // Cancel any remaining vibration
                        try { v.cancel(); } catch (Exception ignored) {}
                        Log.i(TAG, "Vibration thread exiting after " + count + " bursts");
                    } catch (Exception e) {
                        Log.e(TAG, "Failed to start vibration: " + e.getMessage());
                    } finally {
                        sRunning = false;
                    }
                }
            });
        }
    }
}
