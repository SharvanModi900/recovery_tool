package com.recoverkit.audio;

import android.os.IBinder;
import android.os.Parcel;
import android.os.Looper;
import java.lang.reflect.Method;

/**
 * Test 5: Try inputflinger native service with PROPER AIDL Parcel format.
 * Previous attempts used 'service call' which doesn't write the interface token.
 * The native service expects: interface_token + data.
 *
 * Also try: writing the correct IInputFlinger AIDL transactions.
 */
public class TouchDispatchTest5 {

    public static void main(String[] args) {
        boolean disable = true;
        if (args.length > 0 && "enable".equals(args[0])) {
            disable = false;
        }

        System.err.println("UID: " + android.os.Process.myUid());
        System.err.println("Action: " + (disable ? "DISABLE dispatch" : "ENABLE dispatch"));

        try {
            // Get the inputflinger binder
            Class<?> smClass = Class.forName("android.os.ServiceManager");
            Method getService = smClass.getMethod("getService", String.class);
            IBinder flinger = (IBinder) getService.invoke(null, "inputflinger");
            System.err.println("inputflinger binder: " + flinger);
            String descriptor = flinger.getInterfaceDescriptor();
            System.err.println("inputflinger descriptor: " + descriptor);

            // Try IInputFlinger AIDL transaction codes
            // Based on AOSP android15-release:
            // TX 1: setInputDispatchEnabled(bool)
            // TX 2: setInputFilterEnabled(bool)
            // TX 3: getInputReaderStats()
            // TX 4: getInputDispatcherStats()
            
            int[] txCodes = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10};
            
            for (int tx : txCodes) {
                try {
                    Parcel data = Parcel.obtain();
                    Parcel reply = Parcel.obtain();
                    
                    data.writeInterfaceToken(descriptor);
                    data.writeInt(disable ? 1 : 0);
                    
                    boolean result = flinger.transact(tx, data, reply, 0);
                    System.err.println("TX " + tx + ": transact returned " + result);
                    
                    // Check reply
                    int replyData = reply.dataSize();
                    System.err.println("  Reply size: " + replyData);
                    if (replyData > 0) {
                        try {
                            int replyInt = reply.readInt();
                            System.err.println("  Reply int: " + replyInt);
                        } catch (Exception e) {
                            System.err.println("  Reply read error: " + e.getMessage());
                        }
                        try {
                            reply.readException();
                            System.err.println("  No exception in reply");
                        } catch (Exception e) {
                            System.err.println("  Exception: " + e.getMessage());
                        }
                    }
                    
                    data.recycle();
                    reply.recycle();
                    
                    Thread.sleep(200);
                    
                } catch (Exception e) {
                    System.err.println("TX " + tx + " error: " + e.getMessage());
                }
            }

            // Now check dispatch state
            Thread.sleep(500);
            System.err.println("\nDone. Check: adb shell dumpsys input | grep DispatchEnabled");
            
        } catch (Exception e) {
            System.err.println("FAILED: " + e);
            e.printStackTrace(System.err);
        }
    }
}
