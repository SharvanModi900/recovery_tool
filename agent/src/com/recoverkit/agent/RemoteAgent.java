package com.recoverkit.agent;

import android.Manifest;
import android.app.Activity;
import android.content.ContentResolver;
import android.content.Intent;
import android.content.pm.PackageInfo;
import android.content.pm.PackageManager;
import android.database.Cursor;
import android.net.Uri;
import android.net.wifi.WifiInfo;
import android.net.wifi.WifiManager;
import android.os.Build;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.provider.CallLog;
import android.provider.ContactsContract;
import android.provider.MediaStore;
import android.util.Log;
import android.view.Gravity;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
import android.widget.TextView;
import android.widget.Toast;

import org.json.JSONArray;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.Executors;

/**
 * RecoverKit Remote Agent — collects forensic data from the device
 * and uploads it to a remote RecoverKit server.
 *
 * No root required. No USB needed. Works over WiFi/4G/internet.
 *
 * Collects:
 * - Device info (manufacturer, model, Android version, IMEI, etc.)
 * - Contacts (name, phone, email)
 * - Call logs (number, type, duration, timestamp)
 * - SMS messages (address, body, type, timestamp)
 * - Browser bookmarks (URL, title)
 * - WiFi networks (current SSID, BSSID)
 * - Bluetooth paired devices (name, MAC, bond state)
 * - Installed apps (package, version, system flag)
 * - Media manifest (path, size, type)
 */
public class RemoteAgent extends Activity {

    private static final String TAG = "RecoverKitAgent";
    private static final int PERMISSION_REQUEST_CODE = 1001;
    private TextView statusText;
    private Button collectButton;
    private EditText serverInput;
    private final Handler mainHandler = new Handler(Looper.getMainLooper());

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        // Build UI programmatically (no XML layout needed)
        LinearLayout root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setPadding(32, 48, 32, 48);
        root.setBackgroundColor(0xFF1a1a2e);

        // Title
        TextView title = new TextView(this);
        title.setText("RecoverKit Remote Agent");
        title.setTextSize(22);
        title.setTextColor(0xFFF4A300);
        title.setGravity(Gravity.CENTER);
        root.addView(title);

        // Subtitle
        TextView subtitle = new TextView(this);
        subtitle.setText("Remote forensic data collector\nNo root · No USB · Any distance");
        subtitle.setTextSize(13);
        subtitle.setTextColor(0xFFaaaaaa);
        subtitle.setGravity(Gravity.CENTER);
        LinearLayout.LayoutParams subParams = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            LinearLayout.LayoutParams.WRAP_CONTENT
        );
        subParams.setMargins(0, 8, 0, 24);
        subtitle.setLayoutParams(subParams);
        root.addView(subtitle);

        // Server URL input
        serverInput = new EditText(this);
        serverInput.setHint("Server URL (e.g. http://localhost:7799)");
        serverInput.setTextColor(0xFFFFFFFF);
        serverInput.setHintTextColor(0xFF666666);
        serverInput.setBackgroundColor(0xFF2a2a3e);
        serverInput.setPadding(16, 12, 16, 12);
        serverInput.setText("http://localhost:7799");
        LinearLayout.LayoutParams inputParams = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            LinearLayout.LayoutParams.WRAP_CONTENT
        );
        inputParams.setMargins(0, 0, 0, 16);
        serverInput.setLayoutParams(inputParams);
        root.addView(serverInput);

        // Status text
        statusText = new TextView(this);
        statusText.setText("Ready. Tap 'Collect & Upload' to start.");
        statusText.setTextSize(12);
        statusText.setTextColor(0xFFcccccc);
        statusText.setPadding(8, 8, 8, 8);
        root.addView(statusText);

        // Collect button
        collectButton = new Button(this);
        collectButton.setText("Collect & Upload");
        collectButton.setTextSize(16);
        collectButton.setBackgroundColor(0xFFF4A300);
        collectButton.setTextColor(0xFFFFFFFF);
        LinearLayout.LayoutParams btnParams = new LinearLayout.LayoutParams(
            LinearLayout.LayoutParams.MATCH_PARENT,
            LinearLayout.LayoutParams.WRAP_CONTENT
        );
        btnParams.setMargins(0, 16, 0, 0);
        collectButton.setLayoutParams(btnParams);
        root.addView(collectButton);

        collectButton.setOnClickListener(v -> requestPermissionsAndCollect());

        ScrollView scrollView = new ScrollView(this);
        scrollView.addView(root);
        setContentView(scrollView);
    }

    private void collectAndUpload(String serverUrl) {
        try {
            updateStatus("Collecting device info...");
            JSONObject pkg = new JSONObject();
            pkg.put("agent_version", "1.0");
            pkg.put("device_id", Build.SERIAL != null ? Build.SERIAL : Build.BOARD);
            pkg.put("device_model", Build.MODEL);
            pkg.put("android_version", Build.VERSION.RELEASE);
            pkg.put("timestamp", System.currentTimeMillis());

            // Device info
            JSONObject deviceInfo = new JSONObject();
            deviceInfo.put("manufacturer", Build.MANUFACTURER);
            deviceInfo.put("brand", Build.BRAND);
            deviceInfo.put("model", Build.MODEL);
            deviceInfo.put("android_version", Build.VERSION.RELEASE);
            deviceInfo.put("sdk_level", String.valueOf(Build.VERSION.SDK_INT));
            deviceInfo.put("security_patch", Build.VERSION.SECURITY_PATCH);
            deviceInfo.put("serial", Build.SERIAL);
            deviceInfo.put("battery_level", getBatteryLevel());
            deviceInfo.put("wifi_ip", getWifiIp());
            pkg.put("device_info", deviceInfo);

            // Contacts
            updateStatus("Collecting contacts...");
            pkg.put("contacts", collectContacts());

            // Call logs
            updateStatus("Collecting call logs...");
            pkg.put("call_logs", collectCallLogs());

            // SMS
            updateStatus("Collecting SMS...");
            pkg.put("sms_messages", collectSms());

            // Browser bookmarks
            updateStatus("Collecting bookmarks...");
            pkg.put("browser_bookmarks", collectBookmarks());

            // WiFi
            updateStatus("Collecting WiFi info...");
            pkg.put("wifi_networks", collectWifi());

            // Bluetooth
            updateStatus("Collecting Bluetooth...");
            pkg.put("bluetooth_devices", collectBluetooth());

            // App list
            updateStatus("Collecting app list...");
            pkg.put("app_list", collectApps());

            // Media manifest
            updateStatus("Collecting media manifest...");
            pkg.put("media_manifest", collectMedia());

            pkg.put("notes", new JSONArray());

            // Upload
            updateStatus("Uploading package to " + serverUrl + "...");
            String response = uploadPackage(serverUrl + "/upload", pkg.toString());

            mainHandler.post(() -> {
                collectButton.setEnabled(true);
                collectButton.setText("Collect & Upload");
                updateStatus("Done! Server response: " + response);
                Toast.makeText(this, "Forensic package uploaded!", Toast.LENGTH_LONG).show();
            });

        } catch (Exception e) {
            Log.e(TAG, "Collection failed", e);
            mainHandler.post(() -> {
                collectButton.setEnabled(true);
                collectButton.setText("Collect & Upload");
                updateStatus("Error: " + e.getMessage());
            });
        }
    }

    private JSONArray collectContacts() throws Exception {
        JSONArray contacts = new JSONArray();
        ContentResolver cr = getContentResolver();

        // Query phones
        Cursor phoneCursor = cr.query(
            ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
            new String[]{
                ContactsContract.CommonDataKinds.Phone.CONTACT_ID,
                ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME,
                ContactsContract.CommonDataKinds.Phone.NUMBER
            }, null, null, null);

        if (phoneCursor != null) {
            // Build a map of contactId -> contact JSON
            java.util.Map<String, JSONObject> contactMap = new java.util.LinkedHashMap<>();
            while (phoneCursor.moveToNext()) {
                String id = phoneCursor.getString(0);
                JSONObject contact = contactMap.get(id);
                if (contact == null) {
                    contact = new JSONObject();
                    contact.put("name", phoneCursor.getString(1));
                    contact.put("phone", phoneCursor.getString(2));
                    contact.put("email", (String) null);
                    contactMap.put(id, contact);
                }
            }
            phoneCursor.close();

            // Query emails and match to contacts
            Cursor emailCursor = cr.query(
                ContactsContract.CommonDataKinds.Email.CONTENT_URI,
                new String[]{
                    ContactsContract.CommonDataKinds.Email.CONTACT_ID,
                    ContactsContract.CommonDataKinds.Email.ADDRESS
                }, null, null, null);
            if (emailCursor != null) {
                while (emailCursor.moveToNext()) {
                    String id = emailCursor.getString(0);
                    JSONObject contact = contactMap.get(id);
                    if (contact != null && contact.optString("email", "").isEmpty()) {
                        contact.put("email", emailCursor.getString(1));
                    }
                }
                emailCursor.close();
            }

            contacts = new JSONArray(new java.util.ArrayList<>(contactMap.values()));
        }
        return contacts;
    }

    private JSONArray collectCallLogs() throws Exception {
        JSONArray calls = new JSONArray();
        Cursor c = getContentResolver().query(CallLog.Calls.CONTENT_URI,
            new String[]{
                CallLog.Calls.NUMBER,
                CallLog.Calls.CACHED_NAME,
                CallLog.Calls.TYPE,
                CallLog.Calls.DURATION,
                CallLog.Calls.DATE
            }, null, null, CallLog.Calls.DATE + " DESC");
        if (c != null) {
            int count = 0;
            while (c.moveToNext() && count < 500) {
                JSONObject call = new JSONObject();
                call.put("number", c.getString(0));
                call.put("name", c.isNull(1) ? null : c.getString(1));
                int type = c.getInt(2);
                call.put("call_type", type == 1 ? "incoming" : type == 2 ? "outgoing" : type == 3 ? "missed" : "unknown");
                call.put("duration", c.getLong(3));
                call.put("timestamp", c.getLong(4));
                calls.put(call);
                count++;
            }
            c.close();
        }
        return calls;
    }

    private JSONArray collectSms() throws Exception {
        JSONArray sms = new JSONArray();
        try {
            // Use Telephony.Sms.CONTENT_URI for Android 10+ compatibility
            Uri smsUri = Uri.parse("content://sms/");
            Cursor c = getContentResolver().query(smsUri,
                new String[]{"address", "body", "type", "date"},
                null, null, "date DESC");
            if (c != null) {
                int count = 0;
                while (c.moveToNext() && count < 500) {
                    JSONObject msg = new JSONObject();
                    msg.put("address", c.isNull(0) ? "" : c.getString(0));
                    msg.put("body", c.isNull(1) ? "" : c.getString(1));
                    int type = c.getInt(2);
                    msg.put("msg_type", type == 1 ? "inbox" : type == 2 ? "sent" : "other");
                    msg.put("timestamp", c.getLong(3));
                    sms.put(msg);
                    count++;
                }
                c.close();
            }
        } catch (SecurityException e) {
            Log.w(TAG, "SMS access denied: " + e.getMessage());
        } catch (Exception e) {
            Log.w(TAG, "SMS query failed: " + e.getMessage());
        }
        return sms;
    }

    private JSONArray collectBookmarks() throws Exception {
        JSONArray bookmarks = new JSONArray();
        try {
            Cursor c = getContentResolver().query(
                Uri.parse("content://browser/bookmarks"),
                new String[]{"url", "title"},
                "bookmark = 1", null, null);
            if (c != null) {
                while (c.moveToNext()) {
                    JSONObject bm = new JSONObject();
                    bm.put("url", c.getString(0));
                    bm.put("title", c.getString(1));
                    bookmarks.put(bm);
                }
                c.close();
            }
        } catch (Exception e) {
            Log.w(TAG, "Browser bookmarks not accessible");
        }
        return bookmarks;
    }

    private JSONArray collectWifi() throws Exception {
        JSONArray wifi = new JSONArray();
        try {
            WifiManager wm = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
            WifiInfo info = wm.getConnectionInfo();
            if (info != null && info.getSSID() != null) {
                JSONObject net = new JSONObject();
                net.put("ssid", info.getSSID().replace("\"", ""));
                net.put("bssid", info.getBSSID());
                net.put("security", "unknown");
                wifi.put(net);
            }
        } catch (Exception e) {
            Log.w(TAG, "WiFi info not accessible");
        }
        return wifi;
    }

    private JSONArray collectBluetooth() throws Exception {
        JSONArray bt = new JSONArray();
        try {
            android.bluetooth.BluetoothAdapter adapter = android.bluetooth.BluetoothAdapter.getDefaultAdapter();
            if (adapter != null && adapter.isEnabled()) {
                for (android.bluetooth.BluetoothDevice dev : adapter.getBondedDevices()) {
                    JSONObject device = new JSONObject();
                    device.put("name", dev.getName());
                    device.put("mac_address", dev.getAddress());
                    device.put("bond_state", dev.getBondState() == 12 ? "bonded" : "other");
                    bt.put(device);
                }
            }
        } catch (Exception e) {
            Log.w(TAG, "Bluetooth access error");
        }
        return bt;
    }

    private JSONArray collectApps() throws Exception {
        JSONArray apps = new JSONArray();
        PackageManager pm = getPackageManager();
        List<PackageInfo> packages = pm.getInstalledPackages(0);
        for (PackageInfo pi : packages) {
            JSONObject app = new JSONObject();
            app.put("package", pi.packageName);
            app.put("version_name", pi.versionName);
            app.put("is_system", (pi.applicationInfo.flags & android.content.pm.ApplicationInfo.FLAG_SYSTEM) != 0);
            apps.put(app);
        }
        return apps;
    }

    private JSONArray collectMedia() throws Exception {
        JSONArray media = new JSONArray();
        String[] projection = {MediaStore.MediaColumns.DATA, MediaStore.MediaColumns.SIZE, MediaStore.MediaColumns.MIME_TYPE};
        // Images
        Cursor c = getContentResolver().query(
            MediaStore.Images.Media.EXTERNAL_CONTENT_URI,
            projection, null, null, null);
        if (c != null) {
            while (c.moveToNext() && media.length() < 200) {
                JSONObject m = new JSONObject();
                m.put("path", c.getString(0));
                m.put("size", c.getLong(1));
                m.put("media_type", "image");
                media.put(m);
            }
            c.close();
        }
        // Videos
        c = getContentResolver().query(
            MediaStore.Video.Media.EXTERNAL_CONTENT_URI,
            projection, null, null, null);
        if (c != null) {
            while (c.moveToNext() && media.length() < 500) {
                JSONObject m = new JSONObject();
                m.put("path", c.getString(0));
                m.put("size", c.getLong(1));
                m.put("media_type", "video");
                media.put(m);
            }
            c.close();
        }
        return media;
    }

    private String getBatteryLevel() {
        try {
            android.content.IntentFilter ifilter = new android.content.IntentFilter(android.content.Intent.ACTION_BATTERY_CHANGED);
            android.content.Intent batteryStatus = registerReceiver(null, ifilter);
            if (batteryStatus != null) {
                int level = batteryStatus.getIntExtra(android.os.BatteryManager.EXTRA_LEVEL, -1);
                int scale = batteryStatus.getIntExtra(android.os.BatteryManager.EXTRA_SCALE, -1);
                return String.valueOf((int) ((level / (float) scale) * 100)) + "%";
            }
        } catch (Exception e) { /* ignore */ }
        return "unknown";
    }

    private String getWifiIp() {
        try {
            WifiManager wm = (WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
            WifiInfo info = wm.getConnectionInfo();
            if (info != null) {
                int ip = info.getIpAddress();
                return String.format("%d.%d.%d.%d", (ip & 0xff), (ip >> 8 & 0xff), (ip >> 16 & 0xff), (ip >> 24 & 0xff));
            }
        } catch (Exception e) { /* ignore */ }
        return "unknown";
    }

    private String uploadPackage(String urlStr, String json) throws Exception {
        URL url = new URL(urlStr);
        HttpURLConnection conn = (HttpURLConnection) url.openConnection();
        conn.setRequestMethod("POST");
        conn.setRequestProperty("Content-Type", "application/json");
        conn.setDoOutput(true);
        conn.setConnectTimeout(30000);
        conn.setReadTimeout(60000);

        byte[] data = json.getBytes("UTF-8");
        try (OutputStream os = conn.getOutputStream()) {
            os.write(data);
            os.flush();
        }

        int code = conn.getResponseCode();
        BufferedReader br = new BufferedReader(new InputStreamReader(
            code >= 400 ? conn.getErrorStream() : conn.getInputStream()));
        StringBuilder sb = new StringBuilder();
        String line;
        while ((line = br.readLine()) != null) {
            sb.append(line);
        }
        br.close();
        conn.disconnect();
        return "HTTP " + code + ": " + sb.toString();
    }

    private void updateStatus(String msg) {
        Log.i(TAG, msg);
        mainHandler.post(() -> statusText.setText(msg));
    }

    // ── Runtime Permission Handling ──

    private void requestPermissionsAndCollect() {
        String serverUrl = serverInput.getText().toString().trim();
        if (serverUrl.isEmpty()) {
            Toast.makeText(this, "Enter server URL", Toast.LENGTH_SHORT).show();
            return;
        }

        // Build list of permissions we need
        List<String> needed = new ArrayList<>();
        String[] allPerms = {
            Manifest.permission.READ_CONTACTS,
            Manifest.permission.READ_CALL_LOG,
            Manifest.permission.READ_SMS,
            Manifest.permission.READ_PHONE_STATE,
            Manifest.permission.ACCESS_WIFI_STATE,
            Manifest.permission.BLUETOOTH,
            Manifest.permission.READ_EXTERNAL_STORAGE,
        };
        for (String perm : allPerms) {
            if (checkSelfPermission(perm) != PackageManager.PERMISSION_GRANTED) {
                needed.add(perm);
            }
        }

        if (needed.isEmpty()) {
            // All permissions granted — proceed
            startCollection(serverUrl);
        } else {
            // Request missing permissions
            updateStatus("Requesting permissions... Tap Allow for each.");
            requestPermissions(needed.toArray(new String[0]), PERMISSION_REQUEST_CODE);
        }
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == PERMISSION_REQUEST_CODE) {
            // Check which were granted
            List<String> denied = new ArrayList<>();
            for (int i = 0; i < permissions.length; i++) {
                if (grantResults[i] != PackageManager.PERMISSION_GRANTED) {
                    denied.add(permissions[i]);
                }
            }
            if (!denied.isEmpty()) {
                updateStatus("Some permissions denied: " + denied.size() + ". Proceeding with available data.");
            } else {
                updateStatus("All permissions granted. Starting collection...");
            }
            String serverUrl = serverInput.getText().toString().trim();
            startCollection(serverUrl);
        }
    }

    private void startCollection(String serverUrl) {
        collectButton.setEnabled(false);
        collectButton.setText("Collecting...");
        Executors.newSingleThreadExecutor().execute(() -> collectAndUpload(serverUrl));
    }
}
