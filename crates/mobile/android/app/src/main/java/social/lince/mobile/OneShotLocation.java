package social.lince.mobile;

import android.Manifest;
import android.content.pm.PackageManager;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;

final class OneShotLocation implements LocationListener {
    private final MainActivity activity;
    private final long epoch;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final Runnable timeout = () -> fail("Device positioning timed out; choose a point manually");
    private LocationManager manager;
    private boolean closed;

    OneShotLocation(MainActivity activity, long epoch) {
        this.activity = activity;
        this.epoch = epoch;
    }

    void start() {
        if (closed || manager != null) { return; }
        manager = (LocationManager) activity.getSystemService(MainActivity.LOCATION_SERVICE);
        boolean listening = false;
        try {
            if (activity.checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
                    && manager.isProviderEnabled(LocationManager.GPS_PROVIDER)) {
                manager.requestLocationUpdates(LocationManager.GPS_PROVIDER, 1000, 0, this, Looper.getMainLooper());
                listening = true;
            }
            if (activity.checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
                    && manager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)) {
                manager.requestLocationUpdates(LocationManager.NETWORK_PROVIDER, 1000, 0, this, Looper.getMainLooper());
                listening = true;
            }
        } catch (SecurityException error) {
            fail("Device location permission is unavailable");
            return;
        }
        if (listening) { handler.postDelayed(timeout, 30000); }
        else { fail("Enable a location provider or choose a point manually"); }
    }

    void fail(String message) {
        if (!closed) { MainActivity.nativeOnceStatus(epoch, message); }
        close();
    }

    void close() {
        closed = true;
        handler.removeCallbacks(timeout);
        if (manager != null) { manager.removeUpdates(this); manager = null; }
    }

    void suspend() {
        if (manager != null) { fail("Open Lince and locate again"); }
    }

    @Override public void onLocationChanged(Location location) {
        if (closed) { return; }
        long age = (SystemClock.elapsedRealtimeNanos() - location.getElapsedRealtimeNanos()) / 1000000;
        if (age < 0 || age >= 60000) { return; }
        MainActivity.nativeOnceFix(epoch, location.getLatitude(), location.getLongitude(), location.hasAccuracy() ? location.getAccuracy() : -1, age);
        close();
    }

    @Override public void onProviderDisabled(String provider) {
        fail("A location provider was disabled");
    }
}
