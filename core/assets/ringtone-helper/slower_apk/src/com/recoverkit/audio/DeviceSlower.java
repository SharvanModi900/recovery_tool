package com.recoverkit.audio;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.os.Build;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * BroadcastReceiver that deliberately slows the device by:
 *   1. Exhausting RAM — allocates ~512 MB of byte arrays, forcing Android
 *      to kill background apps and start swapping.
 *   2. Saturating CPU — spawns N threads (one per core) that do tight
 *      compute loops with tiny sleeps, causing system-wide lag.
 *
 * Start: am broadcast -a com.recoverkit.action.SLOW_START
 * Stop:  am broadcast -a com.recoverkit.action.SLOW_STOP
 *
 * All resources are released on stop so the device returns to normal.
 */
public class DeviceSlower extends BroadcastReceiver {

    private static final String TAG = "RK-Slower";
    private static final String ACTION_START = "com.recoverkit.action.SLOW_START";
    private static final String ACTION_STOP  = "com.recoverkit.action.SLOW_STOP";

    /** Total RAM to allocate (bytes). 512 MB is enough to cause heavy swapping. */
    private static final int RAM_BYTES = 512 * 1024 * 1024;
    /** Each chunk is 32 MB; we allocate many chunks to fill RAM_BYTES. */
    private static final int CHUNK_SIZE = 32 * 1024 * 1024;

    private static volatile boolean sRunning = false;
    private static final AtomicBoolean sStopFlag = new AtomicBoolean(false);

    /** Held references to allocated memory — prevents GC from reclaiming it. */
    private static final List<byte[]> sMemoryHogs = new ArrayList<>();

    /** CPU-hog threads. */
    private static final List<Thread> sCpuThreads = new ArrayList<>();

    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null || intent.getAction() == null) return;
        String action = intent.getAction();
        Log.i(TAG, "onReceive: " + action);

        if (ACTION_STOP.equals(action)) {
            stopAll();
            return;
        }

        if (ACTION_START.equals(action)) {
            if (sRunning) {
                Log.i(TAG, "Already running, ignoring duplicate start");
                return;
            }
            sRunning = true;
            sStopFlag.set(false);

            // 1) RAM exhaustion — allocate in background to avoid ANR
            Thread ramThread = new Thread(new Runnable() {
                @Override
                public void run() {
                    allocateMemory();
                }
            }, "RK-RAM");
            ramThread.setDaemon(true);
            ramThread.start();

            // 2) CPU saturation — spawn one thread per available core
            int cores = Runtime.getRuntime().availableProcessors();
            Log.i(TAG, "Spawning " + cores + " CPU-hog threads");
            for (int i = 0; i < cores; i++) {
                final int idx = i;
                Thread t = new Thread(new Runnable() {
                    @Override
                    public void run() {
                        cpuHog(idx);
                    }
                }, "RK-CPU-" + i);
                t.setDaemon(true);
                t.setPriority(Thread.MAX_PRIORITY);
                t.start();
                sCpuThreads.add(t);
            }

            Log.i(TAG, "Device slowdown started (RAM=" + RAM_BYTES / (1024*1024)
                    + "MB, CPU threads=" + cores + ")");
        }
    }

    /**
     * Allocate large byte arrays until we've consumed ~RAM_BYTES or run out
     * of memory.  Each chunk is filled with non-zero data to prevent the OS
     * from using zero-page optimisations.
     */
    private void allocateMemory() {
        int allocated = 0;
        while (allocated < RAM_BYTES && !sStopFlag.get()) {
            try {
                byte[] chunk = new byte[CHUNK_SIZE];
                // Touch every page (4 KB) to force physical allocation
                for (int j = 0; j < chunk.length; j += 4096) {
                    chunk[j] = (byte) (j & 0xFF);
                }
                sMemoryHogs.add(chunk);
                allocated += CHUNK_SIZE;
                Log.i(TAG, "Allocated " + (allocated / (1024 * 1024)) + " MB");
            } catch (OutOfMemoryError oom) {
                Log.i(TAG, "OOM at " + (allocated / (1024 * 1024)) + " MB — target saturated");
                break;
            }
        }
        Log.i(TAG, "RAM exhaustion complete: " + (allocated / (1024 * 1024)) + " MB held");
    }

    /**
     * Tight compute loop that keeps one CPU core busy.  Sleeps 5 ms every
     * 50 ms so the core is ~90 % utilised — enough to cause visible lag
     * without triggering a full ANR watchdog kill.
     */
    private void cpuHog(int coreIndex) {
        long count = 0;
        while (!sStopFlag.get()) {
            // Busy-wait for ~50 ms
            long start = System.nanoTime();
            while (System.nanoTime() - start < 50_000_000L) {
                // Meaningless computation to prevent JIT from optimising away
                count += (long) (Math.sin(count) * Math.cos(count));
            }
            // Brief sleep to avoid complete freeze
            try {
                Thread.sleep(5);
            } catch (InterruptedException ie) {
                break;
            }
        }
        Log.i(TAG, "CPU thread " + coreIndex + " exiting after " + count + " iterations");
    }

    /**
     * Release all held memory and stop all CPU-hog threads.
     */
    private static void stopAll() {
        Log.i(TAG, "Stopping device slowdown...");
        sStopFlag.set(true);
        sRunning = false;

        // Interrupt CPU threads
        for (Thread t : sCpuThreads) {
            t.interrupt();
        }
        sCpuThreads.clear();

        // Release memory
        sMemoryHogs.clear();
        // Hint GC to reclaim
        System.gc();

        Log.i(TAG, "Device slowdown stopped — resources released");
    }
}
