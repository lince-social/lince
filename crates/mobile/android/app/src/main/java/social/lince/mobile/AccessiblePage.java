package social.lince.mobile;

import android.graphics.Rect;
import android.os.Bundle;
import android.view.MotionEvent;
import android.view.View;
import android.view.accessibility.AccessibilityEvent;
import android.view.accessibility.AccessibilityNodeInfo;
import android.view.accessibility.AccessibilityNodeProvider;
import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

@SuppressWarnings("deprecation")
final class AccessiblePage extends View {
    private JSONArray items = new JSONArray();
    private int focus = View.NO_ID;
    private int hover = View.NO_ID;
    private final MainActivity activity;

    public AccessiblePage(android.content.Context context) {
        super(context);
        this.activity = (MainActivity) context;
        setImportantForAccessibility(View.IMPORTANT_FOR_ACCESSIBILITY_YES);
        setFocusable(false);
    }

    void update(String data) {
        try {
            JSONArray next = new JSONArray(data);
            String token = item(focus).optString("token");
            items = next;
            int nextFocus = View.NO_ID;
            for (int i = 0; i < items.length(); i++) {
                if (!token.isEmpty() && token.equals(item(i).optString("token"))) { nextFocus = i; break; }
            }
            focus = nextFocus;
            sendAccessibilityEvent(AccessibilityEvent.TYPE_WINDOW_CONTENT_CHANGED);
        } catch (JSONException ignored) {
            items = new JSONArray();
            focus = View.NO_ID;
        }
    }

    private JSONObject item(int id) {
        JSONObject value = items.optJSONObject(id);
        return value == null ? new JSONObject() : value;
    }

    private Rect bounds(int id) {
        JSONArray values = item(id).optJSONArray("bounds");
        if (values == null) { return new Rect(); }
        return new Rect((int) values.optDouble(0), (int) values.optDouble(1),
                (int) values.optDouble(2), (int) values.optDouble(3));
    }

    private void event(int id, int type) {
        if (id < 0 || id >= items.length() || getParent() == null) { return; }
        AccessibilityEvent event = AccessibilityEvent.obtain(type);
        event.setPackageName(activity.getPackageName());
        event.setSource(this, id);
        event.setContentDescription(item(id).optString("label"));
        getParent().requestSendAccessibilityEvent(this, event);
    }

    @Override
    public boolean onHoverEvent(MotionEvent event) {
        int next = View.NO_ID;
        if (event.getAction() != MotionEvent.ACTION_HOVER_EXIT) {
            for (int i = 0; i < items.length(); i++) {
                if (bounds(i).contains((int) event.getX(), (int) event.getY())) { next = i; break; }
            }
        }
        if (next != hover) {
            event(next, AccessibilityEvent.TYPE_VIEW_HOVER_ENTER);
            event(hover, AccessibilityEvent.TYPE_VIEW_HOVER_EXIT);
            hover = next;
        }
        return next != View.NO_ID;
    }

    @Override
    public AccessibilityNodeProvider getAccessibilityNodeProvider() {
        return new AccessibilityNodeProvider() {
            @Override
            public AccessibilityNodeInfo createAccessibilityNodeInfo(int id) {
                if (id == View.NO_ID) {
                    AccessibilityNodeInfo root = AccessibilityNodeInfo.obtain(AccessiblePage.this);
                    onInitializeAccessibilityNodeInfo(root);
                    root.setClassName("android.widget.ScrollView");
                    root.setScrollable(true);
                    root.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                    root.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                    for (int i = 0; i < items.length(); i++) { root.addChild(AccessiblePage.this, i); }
                    return root;
                }
                if (id < 0 || id >= items.length()) { return null; }
                JSONObject item = item(id);
                String kind = item.optString("kind");
                boolean clickable = !kind.equals("text");
                AccessibilityNodeInfo node = AccessibilityNodeInfo.obtain();
                node.setSource(AccessiblePage.this, id);
                node.setParent(AccessiblePage.this);
                node.setPackageName(activity.getPackageName());
                node.setClassName(kind.equals("edit") ? "android.widget.EditText" : clickable ? "android.widget.Button" : "android.widget.TextView");
                node.setText(item.optString("label"));
                node.setPassword(item.optBoolean("password"));
                node.setEnabled(true);
                node.setFocusable(true);
                node.setVisibleToUser(isShown());
                node.setClickable(clickable);
                node.setAccessibilityFocused(focus == id);
                node.addAction(focus == id ? AccessibilityNodeInfo.AccessibilityAction.ACTION_CLEAR_ACCESSIBILITY_FOCUS : AccessibilityNodeInfo.AccessibilityAction.ACTION_ACCESSIBILITY_FOCUS);
                node.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD);
                node.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD);
                if (clickable) { node.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_CLICK); }
                Rect rect = bounds(id);
                node.setBoundsInParent(rect);
                int[] origin = new int[2];
                getLocationOnScreen(origin);
                rect.offset(origin[0], origin[1]);
                node.setBoundsInScreen(rect);
                return node;
            }

            @Override
            public boolean performAction(int id, int action, Bundle arguments) {
                if (action == AccessibilityNodeInfo.ACTION_SCROLL_FORWARD || action == AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD) {
                    MainActivity.nativeAccessible(0, action);
                    return true;
                }
                if (id < 0 || id >= items.length()) { return false; }
                if (action == AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS) {
                    if (focus == id) { return false; }
                    event(focus, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                    focus = id;
                    event(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUSED);
                    return true;
                }
                if (action == AccessibilityNodeInfo.ACTION_CLEAR_ACCESSIBILITY_FOCUS && focus == id) {
                    focus = View.NO_ID;
                    event(id, AccessibilityEvent.TYPE_VIEW_ACCESSIBILITY_FOCUS_CLEARED);
                    return true;
                }
                if (action == AccessibilityNodeInfo.ACTION_CLICK && !item(id).optString("kind").equals("text")) {
                    try { MainActivity.nativeAccessible(Long.parseUnsignedLong(item(id).getString("token")), action); return true; }
                    catch (JSONException | NumberFormatException ignored) { return false; }
                }
                return false;
            }

            @Override
            public AccessibilityNodeInfo findFocus(int type) {
                return type == AccessibilityNodeInfo.FOCUS_ACCESSIBILITY && focus != View.NO_ID ? createAccessibilityNodeInfo(focus) : null;
            }
        };
    }
}
