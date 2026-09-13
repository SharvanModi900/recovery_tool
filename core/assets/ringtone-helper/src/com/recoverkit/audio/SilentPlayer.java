package com.recoverkit.audio;

import android.content.Context;
import android.media.AudioManager;
import android.media.ToneGenerator;
import android.os.Looper;
import java.io.File;

/**
 * Looping silent audio player — generates tones on STREAM_ALARM in an
 * infinite loop until a stop-flag file appears on the device.
 *
 * Designed to be executed on-device via:
 *   CLASSPATH=/path/to/dex app_process / com.recoverkit.audio.SilentPlayer
 *
 * The process runs in the background and checks for the stop flag file
 * (/data/local/tmp/recoverkit_stop) every iteration.  When the file
 * appears, the loop breaks and the process exits cleanly.
 */
public class SilentPlayer {

    /** Path of the stop-flag file.  Create it to stop playback. */
    private static final String STOP_FLAG = "/data/local/tmp/recoverkit_stop";

    /** Duration of each tone burst in ms. */
    private static final int TONE_DURATION_MS = 1200;

    /** Gap between tone bursts in ms. */
    private static final int GAP_MS = 300;

    public static void main(String[] args) {
        System.out.println("[RecoverKit] SilentPlayer starting (looping mode)...");

        // Remove any stale stop-flag from a previous run.
        new File(STOP_FLAG).delete();

        try {
            Looper.prepareMainLooper();

            // ── Bootstrap a system Context via ActivityThread ────────────
            Context ctx = null;
            try {
                Class<?> atClass = Class.forName("android.app.ActivityThread");
                Object at = atClass.getMethod("systemMain").invoke(null);
                ctx = (Context) atClass.getMethod("getSystemContext").invoke(at);
                System.out.println("[RecoverKit] Got system context: " + ctx);
            } catch (Exception e) {
                System.err.println("[RecoverKit] Warning: no system context (" + e.getMessage() + ")");
            }

            // ── Max volume via AudioManager ─────────────────────────────
            if (ctx != null) {
                try {
                    AudioManager am = (AudioManager) ctx.getSystemService(Context.AUDIO_SERVICE);
                    if (am != null) {
                        int maxAlarm = am.getStreamMaxVolume(AudioManager.STREAM_ALARM);
                        am.setStreamVolume(AudioManager.STREAM_ALARM, maxAlarm, 0);
                        System.out.println("[RecoverKit] Alarm volume -> " + maxAlarm);

                        int maxRing = am.getStreamMaxVolume(AudioManager.STREAM_RING);
                        am.setStreamVolume(AudioManager.STREAM_RING, maxRing, 0);
                        System.out.println("[RecoverKit] Ring volume -> " + maxRing);

                        int focus = am.requestAudioFocus(
                            null,
                            AudioManager.STREAM_ALARM,
                            AudioManager.AUDIOFOCUS_GAIN
                        );
                        System.out.println("[RecoverKit] Audio focus (alarm): " + focus
                            + (focus == AudioManager.AUDIOFOCUS_REQUEST_GRANTED ? " (GRANTED)" : " (DENIED)"));
                    }
                } catch (Exception e) {
                    System.err.println("[RecoverKit] AudioManager error: " + e.getMessage());
                }
            }

            // ── Loop: play tones until stop-flag appears ─────────────────
            System.out.println("[RecoverKit] Starting ToneGenerator on STREAM_ALARM...");
            ToneGenerator tg = new ToneGenerator(AudioManager.STREAM_ALARM, 100);

            int[] tones = {
                ToneGenerator.TONE_SUP_RINGTONE,
                ToneGenerator.TONE_CDMA_ALERT_NETWORK_LITE,
                ToneGenerator.TONE_SUP_RINGTONE,
                ToneGenerator.TONE_CDMA_ALERT_CALL_GUARD,
            };
            int toneIndex = 0;
            File stopFile = new File(STOP_FLAG);

            while (!stopFile.exists()) {
                int tone = tones[toneIndex % tones.length];
                tg.startTone(tone, TONE_DURATION_MS);
                Thread.sleep(TONE_DURATION_MS + GAP_MS);
                toneIndex++;
            }

            // ── Cleanup ─────────────────────────────────────────────────
            tg.stopTone();
            tg.release();
            System.out.println("[RecoverKit] Tone playback stopped (flag file detected)");

            if (ctx != null) {
                try {
                    AudioManager am = (AudioManager) ctx.getSystemService(Context.AUDIO_SERVICE);
                    if (am != null) am.abandonAudioFocus(null);
                } catch (Exception ignored) {}
            }

            // Remove the stop-flag so next start works cleanly.
            stopFile.delete();
            System.exit(0);
        } catch (Exception e) {
            System.err.println("[RecoverKit] Error: " + e.getMessage());
            e.printStackTrace();
            System.exit(1);
        }
    }
}
