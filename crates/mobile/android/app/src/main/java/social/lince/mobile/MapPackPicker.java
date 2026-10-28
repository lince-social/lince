package social.lince.mobile;

import android.app.Activity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.storage.StorageManager;
import android.provider.DocumentsContract;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.util.HashSet;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicBoolean;

final class MapPackPicker {
    private final MainActivity activity;
    private final AtomicLong active = new AtomicLong();
    private final AtomicBoolean copying = new AtomicBoolean();
    private final ConcurrentHashMap<Long, File> staged = new ConcurrentHashMap<>();
    private long choosing;
    private long quota;

    MapPackPicker(MainActivity activity) { this.activity = activity; }

    void choose(long epoch, long limit) {
        if (copying.get()) { MainActivity.nativeMapPack(epoch, "", "The previous folder import is still finishing; try again after it ends"); return; }
        if (choosing != 0) { MainActivity.nativeMapPack(epoch, "", "Finish the current map folder selection first"); return; }
        choosing = epoch;
        quota = Math.max(16 * 1024 * 1024, Math.min(limit, 64L * 1024 * 1024 * 1024));
        active.set(epoch);
        Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT_TREE);
        try { activity.startActivityForResult(intent, 403); }
        catch (RuntimeException error) { choosing = 0; MainActivity.nativeMapPack(epoch, "", "Could not open the map folder picker"); }
    }

    void result(int result, Intent data) {
        final long epoch = choosing;
        final long limit = quota;
        choosing = 0;
        if (epoch == 0 || active.get() != epoch) { return; }
        if (result != Activity.RESULT_OK || data == null || data.getData() == null) {
            MainActivity.nativeMapPack(epoch, "", "Map folder selection cancelled"); return;
        }
        final Uri tree = data.getData();
        if (!copying.compareAndSet(false, true)) { MainActivity.nativeMapPack(epoch, "", "A map folder import is already running"); return; }
        new Thread(() -> {
            File root = new File(activity.getCacheDir(), "map-import-" + epoch + "-" + java.util.UUID.randomUUID());
            staged.put(epoch, root);
            try {
                if (!root.mkdir()) { throw new IOException("Could not stage map files"); }
                StorageManager storage = activity.getSystemService(StorageManager.class);
                long available = Math.max(0, (storage.getAllocatableBytes(storage.getUuidForPath(root)) - 256L * 1024 * 1024) / 2);
                long[] budget = {0, 0, Math.min(limit, available)};
                copy(tree, DocumentsContract.getTreeDocumentId(tree), root, 0, epoch, budget);
                if (active.get() != epoch) { throw new IOException("Map import cancelled"); }
                MainActivity.nativeMapPack(epoch, root.getAbsolutePath(), "");
            } catch (Exception error) {
                remove(root);
                staged.remove(epoch);
                MainActivity.nativeMapPack(epoch, "", "Could not stage map pack: " + error.getMessage());
            } finally {
                copying.set(false);
            }
        }, "lince-map-import").start();
    }

    private void copy(Uri tree, String directory, File destination, int depth, long epoch, long[] budget) throws IOException {
        if (depth > 4) { throw new IOException("Map pack folders exceed four levels"); }
        Uri children = DocumentsContract.buildChildDocumentsUriUsingTree(tree, directory);
        String[] columns = {DocumentsContract.Document.COLUMN_DOCUMENT_ID, DocumentsContract.Document.COLUMN_DISPLAY_NAME, DocumentsContract.Document.COLUMN_MIME_TYPE};
        HashSet<String> names = new HashSet<>();
        try (Cursor cursor = activity.getContentResolver().query(children, columns, null, null, null)) {
            if (cursor == null) { throw new IOException("Map folder unavailable"); }
            while (cursor.moveToNext()) {
                if (active.get() != epoch) { throw new IOException("Map import cancelled"); }
                if (++budget[1] > 50064) { throw new IOException("Too many map files or folders"); }
                String id = cursor.getString(0);
                String name = cursor.getString(1);
                if (name == null || name.isEmpty() || name.length() > 255 || name.equals(".") || name.equals("..") || name.contains("/") || name.contains("\\") || !names.add(name)) { throw new IOException("Invalid or duplicate map filename"); }
                File target = new File(destination, name);
                if (DocumentsContract.Document.MIME_TYPE_DIR.equals(cursor.getString(2))) {
                    if (!target.mkdir()) { throw new IOException("Could not stage map directory"); }
                    copy(tree, id, target, depth + 1, epoch, budget);
                } else {
                    Uri file = DocumentsContract.buildDocumentUriUsingTree(tree, id);
                    try (InputStream source = activity.getContentResolver().openInputStream(file); FileOutputStream output = new FileOutputStream(target)) {
                        if (source == null) { throw new IOException("Map file unavailable"); }
                        byte[] bytes = new byte[65536];
                        int count;
                        long fileBytes = 0;
                        while ((count = source.read(bytes)) != -1) {
                            if (active.get() != epoch) { throw new IOException("Map import cancelled"); }
                            budget[0] += count;
                            fileBytes += count;
                            if (budget[0] > budget[2] || (name.equals("manifest.json") && fileBytes > 4 * 1024 * 1024)) { throw new IOException("Map pack exceeds storage or manifest limits"); }
                            output.write(bytes, 0, count);
                        }
                        output.getFD().sync();
                    }
                }
            }
        }
    }

    void cancel(long epoch) {
        active.compareAndSet(epoch, 0);
        File root = staged.remove(epoch);
        if (root != null) { new Thread(() -> remove(root), "lince-map-cleanup").start(); }
    }

    void close() {
        active.set(0);
        for (Long epoch : staged.keySet()) { cancel(epoch); }
    }

    private static void remove(File file) {
        File[] children = file.listFiles();
        if (children != null) { for (File child : children) { remove(child); } }
        file.delete();
    }
}
