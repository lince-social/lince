package social.lince.mobile;

public final class RestartActivity extends android.app.Activity {
    @Override
    protected void onCreate(android.os.Bundle state) {
        super.onCreate(state);
        int previous = getIntent().getIntExtra("old_pid", -1);
        if (previous <= 0 || previous == android.os.Process.myPid()) { finishAndRemoveTask(); return; }
        android.os.Handler handler = new android.os.Handler(android.os.Looper.getMainLooper());
        long deadline = android.os.SystemClock.elapsedRealtime() + 5000;
        handler.post(new Runnable() {
            @Override
            public void run() {
                if (new java.io.File("/proc/" + previous).exists() && android.os.SystemClock.elapsedRealtime() < deadline) {
                    handler.postDelayed(this, 25);
                    return;
                }
                android.content.Intent launch = new android.content.Intent();
                launch.setComponent(new android.content.ComponentName(getPackageName(), "social.lince.mobile.MainActivity"));
                launch.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK | android.content.Intent.FLAG_ACTIVITY_CLEAR_TASK);
                startActivity(launch);
                finishAndRemoveTask();
                android.os.Process.killProcess(android.os.Process.myPid());
            }
        });
    }
}
