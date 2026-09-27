package social.lince.mobile;

import android.app.AlertDialog;
import android.app.NativeActivity;
import android.os.Bundle;
import android.text.InputType;
import android.view.WindowManager;
import android.view.WindowInsets;
import android.widget.EditText;

public final class MainActivity extends NativeActivity {
    static { System.loadLibrary("lince_mobile"); }
    private AlertDialog editor;
    private EditText editing;
    private long editingId;
    private static native void nativeEdit(long id, String value, boolean committed);
    private static native void nativeBack();
    private static native void nativeInsets(int left, int top, int right, int bottom);

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
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
        if (editor != null && editor.isShowing() && editing != null) {
            nativeEdit(editingId, editing.getText().toString(), true);
            editor.dismiss();
        }
        super.onPause();
    }

    @SuppressWarnings("deprecation")
    public void edit(long id, String title, String value, boolean multiline) {
        runOnUiThread(() -> {
            if (isFinishing() || isDestroyed()) { return; }
            if (editor != null) { editor.dismiss(); }
            EditText input = new EditText(this);
            editing = input;
            editingId = id;
            input.setInputType(InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_CAP_SENTENCES
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
}
