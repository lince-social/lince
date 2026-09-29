package social.lince.mobile;

import android.app.AlertDialog;
import android.app.NativeActivity;
import android.os.Bundle;
import android.text.InputType;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.widget.EditText;

@SuppressWarnings("deprecation")
public final class MainActivity extends NativeActivity {
    static { System.loadLibrary("lince_mobile"); }
    private AlertDialog editor;
    private EditText editing;
    private long editingId;
    private static native void nativeEdit(long id, String value, boolean committed);
    private static native void nativeBack();
    private static native void nativeInsets(int left, int top, int right, int bottom);
    private static native void nativeCameraFrame(byte[] pixels, int width, int height);
    private static native void nativeCameraError(String message);
    private static native void nativeWake();
    private static native void nativeAttachment(long id, String name, String mime, byte[] bytes);
    private static native void nativeAttachmentError(long id, String message);
    private static native void nativeFileNotice(String message);
    private static native void nativeSmoke(String command);
    private static native void nativeAccessibility(boolean enabled);
    static native void nativeAccessible(long token, int action);
    private AccessiblePage accessible;
    private android.view.accessibility.AccessibilityManager accessibilityManager;
    private android.view.accessibility.AccessibilityManager.AccessibilityStateChangeListener accessibilityListener;
    private long choosingFile;
    private byte[] savingFile;
    private android.net.wifi.WifiManager.MulticastLock multicast;
    private boolean discovering;
    private long discoveryUntil;
    private boolean resumed;
    private android.hardware.Camera camera;
    private AlertDialog scanner;
    private long lastFrame;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        if (android.os.Build.VERSION.SDK_INT >= 33) { setRecentsScreenshotEnabled(false); }
        accessibilityManager = (android.view.accessibility.AccessibilityManager) getSystemService(ACCESSIBILITY_SERVICE);
        accessible = new AccessiblePage(this);
        addContentView(accessible, new android.view.ViewGroup.LayoutParams(-1, -1));
        accessibilityListener = enabled -> {
            accessible.setVisibility(enabled ? android.view.View.VISIBLE : android.view.View.GONE);
            nativeAccessibility(enabled);
        };
        accessibilityManager.addAccessibilityStateChangeListener(accessibilityListener);
        accessibilityListener.onAccessibilityStateChanged(accessibilityManager.isEnabled());
        getWindow().getDecorView().setOnApplyWindowInsetsListener((view, insets) -> {
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets safe = insets.getInsets(WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout());
                nativeInsets(safe.left, safe.top, safe.right, safe.bottom);
            } else {
                legacyInsets(insets);
            }
            return insets;
        });
        getWindow().getDecorView().requestApplyInsets();
        if (android.os.Build.VERSION.SDK_INT >= 33) {
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(0, MainActivity::nativeBack);
        }
        smoke(getIntent());
    }

    public void accessibilitySnapshot(String value) {
        runOnUiThread(() -> accessible.update(value));
    }

    public void restartProfile() {
        runOnUiThread(() -> {
            android.content.Intent restart = new android.content.Intent(this, RestartActivity.class);
            restart.putExtra("old_pid", android.os.Process.myPid());
            restart.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK);
            startActivity(restart);
            android.os.Process.killProcess(android.os.Process.myPid());
        });
    }

    public void openLink(String value) {
        runOnUiThread(() -> {
            android.net.Uri uri = android.net.Uri.parse(value);
            if (!"https".equals(uri.getScheme()) && !"http".equals(uri.getScheme())) { return; }
            try { startActivity(new android.content.Intent(android.content.Intent.ACTION_VIEW, uri)); }
            catch (android.content.ActivityNotFoundException error) { nativeFileNotice("Install a browser to open this link"); }
        });
    }

    @Override
    protected void onDestroy() {
        if (accessibilityManager != null && accessibilityListener != null) {
            accessibilityManager.removeAccessibilityStateChangeListener(accessibilityListener);
        }
        nativeAccessibility(false);
        closeScanner();
        if (multicast != null && multicast.isHeld()) { multicast.release(); }
        super.onDestroy();
    }

    @Override
    protected void onNewIntent(android.content.Intent intent) {
        super.onNewIntent(intent);
        smoke(intent);
    }

    private void smoke(android.content.Intent intent) {
        if (BuildConfig.LINCE_SMOKE && intent != null) {
            String command = intent.getStringExtra("lince_smoke");
            if (command != null) { nativeSmoke(command); }
        }
    }

    @Override
    @SuppressWarnings("deprecation")
    public void onBackPressed() {
        nativeBack();
    }

    @SuppressWarnings("deprecation")
    private static void legacyInsets(WindowInsets insets) {
        nativeInsets(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
    }

    @Override
    protected void onPause() {
        resumed = false;
        updateMulticast();
        closeScanner();
        if (editor != null && editor.isShowing() && editing != null) {
            nativeEdit(editingId, editing.getText().toString(), true);
            editor.dismiss();
        }
        super.onPause();
    }

    @Override
    protected void onResume() {
        super.onResume();
        resumed = true;
        updateMulticast();
    }

    public void discovery(boolean enabled, long milliseconds) {
        runOnUiThread(() -> {
            discovering = enabled;
            discoveryUntil = android.os.SystemClock.elapsedRealtime() + Math.max(0, milliseconds);
            updateMulticast();
            if (enabled) { getWindow().getDecorView().postDelayed(this::updateMulticast, Math.max(1, milliseconds)); }
        });
    }

    private void updateMulticast() {
        if (android.os.SystemClock.elapsedRealtime() >= discoveryUntil) { discovering = false; }
        if (multicast == null && discovering && resumed) {
            android.net.wifi.WifiManager wifi = (android.net.wifi.WifiManager) getApplicationContext().getSystemService(WIFI_SERVICE);
            if (wifi == null) { return; }
            multicast = wifi.createMulticastLock("lince-discovery");
            multicast.setReferenceCounted(false);
        }
        if (multicast == null) { return; }
        if (discovering && resumed && !multicast.isHeld()) { multicast.acquire(); }
        if ((!discovering || !resumed) && multicast.isHeld()) { multicast.release(); }
    }

    public void scanQr() {
        runOnUiThread(() -> {
            if (checkSelfPermission(android.Manifest.permission.CAMERA) != android.content.pm.PackageManager.PERMISSION_GRANTED) {
                requestPermissions(new String[] {android.Manifest.permission.CAMERA}, 41);
                return;
            }
            openScanner();
        });
    }

    @Override
    public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request != 41) { return; }
        if (results.length > 0 && results[0] == android.content.pm.PackageManager.PERMISSION_GRANTED) {
            openScanner();
        } else {
            nativeCameraError("Camera permission denied. You can still paste the device enrolment code.");
        }
    }

    private void openScanner() {
        if (isFinishing() || isDestroyed()) { return; }
        closeScanner();
        android.view.SurfaceView preview = new android.view.SurfaceView(this);
        scanner = new AlertDialog.Builder(this).setTitle("Scan device enrolment QR")
                .setView(preview).setNegativeButton("Cancel", (dialog, which) -> closeScanner()).create();
        scanner.setOnDismissListener(dialog -> {
            releaseCamera();
            nativeWake();
        });
        preview.getHolder().addCallback(new android.view.SurfaceHolder.Callback() {
            public void surfaceCreated(android.view.SurfaceHolder holder) {
                try {
                    camera = android.hardware.Camera.open();
                    android.hardware.Camera.Parameters parameters = camera.getParameters();
                    android.hardware.Camera.Size size = parameters.getSupportedPreviewSizes().stream()
                            .filter(candidate -> candidate.width <= 1280 && candidate.height <= 720)
                            .max(java.util.Comparator.comparingInt(candidate -> candidate.width * candidate.height))
                            .orElseThrow(() -> new IllegalStateException("No supported camera size"));
                    parameters.setPreviewSize(size.width, size.height);
                    parameters.setPreviewFormat(android.graphics.ImageFormat.NV21);
                    if (parameters.getSupportedFocusModes().contains(android.hardware.Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE)) {
                        parameters.setFocusMode(android.hardware.Camera.Parameters.FOCUS_MODE_CONTINUOUS_PICTURE);
                    }
                    camera.setParameters(parameters);
                    camera.setDisplayOrientation(90);
                    camera.setPreviewDisplay(holder);
                    camera.setPreviewCallback((data, source) -> {
                        long now = android.os.SystemClock.elapsedRealtime();
                        if (now - lastFrame < 400) { return; }
                        lastFrame = now;
                        nativeCameraFrame(data, size.width, size.height);
                    });
                    camera.startPreview();
                } catch (Exception error) {
                    closeScanner();
                    nativeCameraError("Could not open the camera. Paste the device enrolment code or try again.");
                }
            }
            public void surfaceChanged(android.view.SurfaceHolder holder, int format, int width, int height) {}
            public void surfaceDestroyed(android.view.SurfaceHolder holder) { releaseCamera(); }
        });
        scanner.show();
        preview.getLayoutParams().height = (int) (340 * getResources().getDisplayMetrics().density);
        preview.requestLayout();
        AlertDialog active = scanner;
        preview.postDelayed(() -> { if (scanner == active) { closeScanner(); } }, 30000);
    }

    private void releaseCamera() {
        if (camera != null) {
            android.hardware.Camera active = camera;
            camera = null;
            try {
                active.setPreviewCallback(null);
                active.stopPreview();
            } catch (RuntimeException error) {
                nativeCameraError("Camera disconnected. You can retry or paste the enrolment code.");
            } finally {
                active.release();
            }
        }
    }

    public void closeScanner() {
        runOnUiThread(() -> {
            releaseCamera();
            if (scanner != null) { scanner.dismiss(); scanner = null; }
        });
    }

    @SuppressWarnings("deprecation")
    public void edit(long id, String title, String value, boolean multiline, boolean password) {
        runOnUiThread(() -> {
            if (isFinishing() || isDestroyed()) { return; }
            if (editor != null) { editor.dismiss(); }
            EditText input = new EditText(this);
            editing = input;
            editingId = id;
            input.setInputType(password ? InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD
                    : InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
                    | (multiline ? InputType.TYPE_TEXT_FLAG_MULTI_LINE : 0));
            input.setSingleLine(!multiline);
            input.setFilters(new android.text.InputFilter[] {
                new android.text.InputFilter.LengthFilter(multiline ? 262144 : 65536)
            });
            input.setText(value);
            input.setSelection(input.length());
            int padding = (int) (16 * getResources().getDisplayMetrics().density);
            input.setPadding(padding, padding, padding, padding);
            editor = new AlertDialog.Builder(this)
                    .setTitle(title)
                    .setView(input)
                    .setPositiveButton("Keep edit", (dialog, which) -> nativeEdit(id, input.getText().toString(), true))
                    .setNegativeButton("Cancel", (dialog, which) -> nativeEdit(id, "", false))
                    .setOnCancelListener(dialog -> nativeEdit(id, "", false))
                    .create();
            editor.setOnShowListener(dialog -> {
                input.requestFocus();
                editor.getWindow().setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_VISIBLE
                        | WindowManager.LayoutParams.SOFT_INPUT_ADJUST_RESIZE);
            });
            editor.show();
        });
    }

    public void chooseFile(long id) {
        runOnUiThread(() -> {
            choosingFile = id;
            android.content.Intent intent = new android.content.Intent(android.content.Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(android.content.Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            try { startActivityForResult(intent, 401); }
            catch (RuntimeException error) { choosingFile = 0; nativeAttachmentError(id, "Could not open the document picker"); }
        });
    }

    public void saveFile(String name, String mime, byte[] bytes) {
        runOnUiThread(() -> {
            if (savingFile != null) { nativeFileNotice("Finish the current save first"); return; }
            if (bytes.length > 4 * 1024 * 1024) { nativeFileNotice("Attachment exceeds 4 MiB"); return; }
            savingFile = bytes;
            android.content.Intent intent = new android.content.Intent(android.content.Intent.ACTION_CREATE_DOCUMENT);
            intent.addCategory(android.content.Intent.CATEGORY_OPENABLE);
            intent.setType(mime);
            intent.putExtra(android.content.Intent.EXTRA_TITLE, name);
            try { startActivityForResult(intent, 402); }
            catch (RuntimeException error) { savingFile = null; nativeFileNotice("Could not open the save picker"); }
        });
    }

    @Override
    protected void onActivityResult(int request, int result, android.content.Intent data) {
        super.onActivityResult(request, result, data);
        if (request == 401) {
            long id = choosingFile;
            choosingFile = 0;
            if (id == 0) { return; }
            if (result != RESULT_OK || data == null || data.getData() == null) { nativeAttachmentError(id, "File selection cancelled"); return; }
            android.net.Uri uri = data.getData();
            new Thread(() -> {
                try (java.io.InputStream source = getContentResolver().openInputStream(uri)) {
                    if (source == null) { throw new java.io.IOException("File unavailable"); }
                    java.io.ByteArrayOutputStream bytes = new java.io.ByteArrayOutputStream();
                    byte[] buffer = new byte[32768];
                    int count;
                    while ((count = source.read(buffer)) != -1) {
                        if (bytes.size() + count > 4 * 1024 * 1024) { throw new java.io.IOException("Choose a file smaller than 4 MiB"); }
                        bytes.write(buffer, 0, count);
                    }
                    String name = "attachment";
                    try (android.database.Cursor cursor = getContentResolver().query(uri, new String[]{android.provider.OpenableColumns.DISPLAY_NAME}, null, null, null)) {
                        if (cursor != null && cursor.moveToFirst() && !cursor.isNull(0)) { name = cursor.getString(0); }
                    }
                    if (name.length() > 255) { name = name.substring(0, 255); }
                    String mime = getContentResolver().getType(uri);
                    if (mime == null || mime.length() > 128) { mime = "application/octet-stream"; }
                    nativeAttachment(id, name, mime, bytes.toByteArray());
                } catch (Exception error) { nativeAttachmentError(id, "Could not read the file: " + error.getMessage()); }
            }, "lince-file-import").start();
        } else if (request == 402) {
            byte[] bytes = savingFile;
            savingFile = null;
            if (bytes == null) { return; }
            if (result != RESULT_OK || data == null || data.getData() == null) { nativeFileNotice("File save cancelled"); return; }
            android.net.Uri uri = data.getData();
            new Thread(() -> {
                try (java.io.OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
                    if (output == null) { throw new java.io.IOException("Destination unavailable"); }
                    output.write(bytes);
                    nativeFileNotice("Attachment saved");
                } catch (Exception error) { nativeFileNotice("Could not save the attachment: " + error.getMessage()); }
            }, "lince-file-export").start();
        }
    }
}
