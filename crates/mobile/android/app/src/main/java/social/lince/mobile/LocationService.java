package social.lince.mobile;

import android.Manifest;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.location.Location;
import android.location.LocationListener;
import android.location.LocationManager;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.SystemClock;

public final class LocationService extends Service implements LocationListener {
    private static final String CHANNEL = "lince-location";
    private static final int NOTICE = 73;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private LocationManager manager;
    private long epoch;
    private long deadline;
    private final Runnable expire = new Runnable() {
        @Override public void run() {
            long remaining = deadline - SystemClock.elapsedRealtime();
            if (remaining <= 0) { stopSelf(); }
            else { handler.postDelayed(this, Math.min(remaining, 1000)); }
        }
    };

    @Override public IBinder onBind(Intent intent) { return null; }

    @Override public int onStartCommand(Intent intent, int flags, int id) {
        if (intent == null || "stop".equals(intent.getAction())) { stopSelf(); return START_NOT_STICKY; }
        long requestedEpoch = intent.getLongExtra("epoch", 0);
        if (requestedEpoch <= 0) { stopSelf(); return START_NOT_STICKY; }
        epoch = requestedEpoch;
        deadline = SystemClock.elapsedRealtime() + Math.max(1, Math.min(intent.getLongExtra("duration", 1), 86400000));
        NotificationManager notifications = (NotificationManager) getSystemService(NOTIFICATION_SERVICE);
        notifications.createNotificationChannel(new NotificationChannel(CHANNEL, "Live location", NotificationManager.IMPORTANCE_LOW));
        Intent stop = new Intent(this, LocationService.class).setAction("stop");
        PendingIntent stopAction = PendingIntent.getService(this, 0, stop, PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_IMMUTABLE | PendingIntent.FLAG_UPDATE_CURRENT);
        Notification notice = new Notification.Builder(this, CHANNEL)
                .setSmallIcon(android.R.drawable.ic_menu_mylocation)
                .setContentTitle("Lince live location is active")
                .setContentText("Sharing with your selected recipients until stopped or expired")
                .setOngoing(true).setContentIntent(open)
                .addAction(new Notification.Action.Builder(null, "Stop location", stopAction).build()).build();
        startForeground(NOTICE, notice);
        handler.removeCallbacks(expire);
        handler.post(expire);
        if (manager != null) { return START_NOT_STICKY; }
        manager = (LocationManager) getSystemService(LOCATION_SERVICE);
        boolean listening = false;
        try {
            if (checkSelfPermission(Manifest.permission.ACCESS_FINE_LOCATION) == PackageManager.PERMISSION_GRANTED
                    && manager.isProviderEnabled(LocationManager.GPS_PROVIDER)) {
                manager.requestLocationUpdates(LocationManager.GPS_PROVIDER, 3000, 0, this, Looper.getMainLooper());
                listening = true;
            }
            if (checkSelfPermission(Manifest.permission.ACCESS_COARSE_LOCATION) == PackageManager.PERMISSION_GRANTED
                    && manager.isProviderEnabled(LocationManager.NETWORK_PROVIDER)) {
                manager.requestLocationUpdates(LocationManager.NETWORK_PROVIDER, 3000, 0, this, Looper.getMainLooper());
                listening = true;
            }
        } catch (SecurityException error) { MainActivity.nativeLocationStatus(epoch, "Device location permission was revoked"); }
        if (!listening) { MainActivity.nativeLocationStatus(epoch, "Enable a location provider or choose a manual source"); stopSelf(); }
        return START_NOT_STICKY;
    }

    @Override public void onLocationChanged(Location location) {
        long age = (SystemClock.elapsedRealtimeNanos() - location.getElapsedRealtimeNanos()) / 1000000;
        if (age < 0 || age >= 60000 || SystemClock.elapsedRealtime() >= deadline) { return; }
        MainActivity.nativeLocationFix(epoch, location.getLatitude(), location.getLongitude(), location.hasAccuracy() ? location.getAccuracy() : -1, age);
    }

    @Override public void onProviderDisabled(String provider) {
        MainActivity.nativeLocationStatus(epoch, "A device location provider was disabled");
    }

    @Override public void onTaskRemoved(Intent intent) { stopSelf(); }

    @Override public void onDestroy() {
        handler.removeCallbacks(expire);
        if (manager != null) { manager.removeUpdates(this); }
        if (epoch != 0) { MainActivity.nativeLocationStop(epoch); }
        stopForeground(STOP_FOREGROUND_REMOVE);
        super.onDestroy();
    }
}
