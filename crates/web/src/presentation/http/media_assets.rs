//! Locally-hosted images for a record body (2026-07-17). There is
//! deliberately NO route that serves an arbitrary disk path (that would let
//! any web page open in the same browser read files off this machine through
//! a bare `<img src>` — see the maneirisms doc). Instead: an upload endpoint
//! sniffs the bytes against an allowlist of raster formats (never trusting
//! the client's filename or extension), writes them under
//! `<lince_data_dir>/web/media/` with an OPAQUE generated name, and the
//! serving route only ever answers that exact name shape back — no
//! client-controlled path component reaches the filesystem in either
//! direction.

/// Magic-byte sniff, not filename/extension trust. Returns the extension the
/// file will be stored/served under.
pub(crate) fn sniff_image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

pub(crate) const MAX_MEDIA_UPLOAD_BYTES: usize = 12 * 1024 * 1024;

/// Sniffs `bytes`, generates an opaque name, and writes it under
/// `media_dir()`. Returns the servable `/host/media/<name>` path. Shared by
/// the multipart upload endpoint and the native-picker endpoint below — same
/// allowlist, same opaque naming, regardless of which surface it came from.
pub(crate) async fn store_media_bytes(
    pool: &store::sqlx::SqlitePool,
    bytes: &[u8],
) -> Result<String, (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;
    if bytes.len() > MAX_MEDIA_UPLOAD_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            "image too large (max 12MB)".to_string(),
        ));
    }
    let ext = sniff_image_ext(bytes).ok_or_else(|| {
        (
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "not a recognized image (png/jpg/gif/webp)".to_string(),
        )
    })?;
    let name = format!("{}.{ext}", uuid::Uuid::new_v4());
    let dir = crate::infrastructure::paths::media_dir();
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let path = dir.join(&name);
    tokio::fs::write(&path, bytes)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    enforce_budget_preserving(pool, Some(&path)).await?;
    if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
        return Err((
            StatusCode::INSUFFICIENT_STORAGE,
            "image is larger than the media share of this Cell's storage budget".to_string(),
        ));
    }
    Ok(format!("/host/media/{name}"))
}

/// Bring the media directory back inside its fixed share of this Cell's
/// storage budget. Newest access wins; an image larger than the whole share is
/// evicted too, because keeping it would make the stated ceiling untrue.
pub(crate) async fn enforce_budget(
    pool: &store::sqlx::SqlitePool,
) -> Result<(), (axum::http::StatusCode, String)> {
    enforce_budget_preserving(pool, None).await
}

async fn enforce_budget_preserving(
    pool: &store::sqlx::SqlitePool,
    newest: Option<&std::path::Path>,
) -> Result<(), (axum::http::StatusCode, String)> {
    use axum::http::StatusCode;

    let total = store::budget::total(pool)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    let Some(quota) = store::budget::share(total, store::budget::Area::Media) else {
        return Ok(());
    };
    evict_dir_to_quota(&crate::infrastructure::paths::media_dir(), quota, newest)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))
}

/// The filesystem is the media index: size is the charged amount and mtime is
/// last use. Eviction and recency updates are serialized; every upload enforces
/// after its own write, so the last concurrent writer also closes the budget.
static MEDIA_STORE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn evict_dir_to_quota(
    dir: &std::path::Path,
    quota: i64,
    newest: Option<&std::path::Path>,
) -> std::io::Result<()> {
    let _guard = MEDIA_STORE.lock().await;
    let mut read = match tokio::fs::read_dir(dir).await {
        Ok(read) => read,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error),
    };
    let mut entries = Vec::new();
    while let Some(entry) = read.next_entry().await? {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !valid_media_filename(&name) {
            continue;
        }
        let metadata = entry.metadata().await?;
        if !metadata.is_file() {
            continue;
        }
        entries.push((
            entry.path(),
            i64::try_from(metadata.len()).unwrap_or(i64::MAX),
            metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        ));
    }
    // `evict_plan` consumes newest first. The path tiebreak keeps eviction
    // deterministic on filesystems whose timestamp resolution is coarse.
    entries.sort_by(|left, right| {
        (newest == Some(right.0.as_path()))
            .cmp(&(newest == Some(left.0.as_path())))
            .then_with(|| right.2.cmp(&left.2))
            .then_with(|| right.0.cmp(&left.0))
    });
    let charged: Vec<(usize, i64)> = entries
        .iter()
        .enumerate()
        .map(|(index, (_, bytes, _))| (index, *bytes))
        .collect();
    for index in store::budget::evict_plan(&charged, quota) {
        match tokio::fs::remove_file(&entries[index].0).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

/// Mark a successfully served image as recently used for the next eviction.
pub(crate) async fn touch(path: std::path::PathBuf) {
    let _guard = MEDIA_STORE.lock().await;
    let _ = tokio::task::spawn_blocking(move || {
        let file = std::fs::OpenOptions::new().write(true).open(path)?;
        file.set_modified(std::time::SystemTime::now())
    })
    .await;
}

/// The `/image` slash block's file picker (2026-07-18). Runs the system file
/// dialog RIGHT HERE in the plain web server process via `rfd`'s
/// "xdg-portal" feature (ashpd — a pure-Rust D-Bus client to
/// xdg-desktop-portal, ZERO GTK/WebKit involvement), then stores the pick
/// exactly like an upload. This assumes the browser and the Cell are on the
/// same machine — the same "local cell" assumption this codebase already
/// documents for local-disk images — so the dialog opens where the SERVER
/// runs, which is correct for the local desktop/single-user case this
/// feature targets.
///
/// Why not go through WebKitGTK's own `<input type=file>` / Tauri's IPC
/// instead: WebKitGTK has no custom file-chooser handler registered in this
/// app, so `<input type=file>.click()` fell back to WebKitGTK's OWN built-in
/// `GtkFileChooserWidget`, which aborts the whole process on teardown if the
/// `org.gtk.Settings.FileChooser` GSettings schema isn't on the launching
/// environment's `XDG_DATA_DIRS` (confirmed via `coredumpctl`: SIGABRT in
/// `_gtk_file_chooser_get_settings_for_widget` → `g_settings_set_property` →
/// a GLib FATAL log → `abort()`, cascading into the WebKitWebProcess crashing
/// ~3s later). Tauri's own dialog plugin / a custom Tauri command hits a
/// DIFFERENT wall instead: the webview loads a real `http://` URL, not
/// `tauri://`, so Tauri v2's capability/ACL system blocks IPC to it without
/// an explicit remote-domain capability grant. Running the picker here, as a
/// plain HTTP endpoint the sand already knows how to call, avoids both.
///
/// Feature-gated (`native-picker`, only enabled by `lince-desktop`): `rfd`'s
/// xdg-portal backend pulls in `wayland-sys`, which needs pkg-config +
/// wayland dev headers at build time — the plain `lince` CLI must keep
/// building with just cargo + rustc.
#[cfg(feature = "native-picker")]
pub(crate) async fn pick_and_store_image(
    pool: &store::sqlx::SqlitePool,
) -> Result<Option<String>, (axum::http::StatusCode, String)> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let bytes = handle.read().await;
    store_media_bytes(pool, &bytes).await.map(Some)
}

pub(crate) fn content_type_for_ext(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

/// A generated media filename is always `<uuid-v4>.<ext>`. Reject anything
/// else before it ever touches the filesystem — this is the traversal guard:
/// no `/`, no `..`, no client-chosen name, structurally (not just checked).
pub(crate) fn valid_media_filename(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(ext, "png" | "jpg" | "gif" | "webp")
        && stem.len() == 36
        && stem.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_known_formats_by_magic_bytes() {
        assert_eq!(sniff_image_ext(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
        assert_eq!(sniff_image_ext(b"\xFF\xD8\xFFrest"), Some("jpg"));
        assert_eq!(sniff_image_ext(b"GIF89arest"), Some("gif"));
        let mut webp = b"RIFF....WEBPrest".to_vec();
        webp[4..8].copy_from_slice(b"1234");
        assert_eq!(sniff_image_ext(&webp), Some("webp"));
    }

    #[test]
    fn rejects_non_image_bytes_regardless_of_claimed_extension() {
        assert_eq!(sniff_image_ext(b"<svg xmlns=..."), None);
        assert_eq!(
            sniff_image_ext(b"<html><script>evil()</script></html>"),
            None
        );
        assert_eq!(sniff_image_ext(b""), None);
    }

    #[test]
    fn valid_media_filename_accepts_only_uuid_dot_ext() {
        assert!(valid_media_filename(
            "3fa85f64-5717-4562-b3fc-2c963f66afa6.png"
        ));
        assert!(valid_media_filename(
            "3fa85f64-5717-4562-b3fc-2c963f66afa6.webp"
        ));
    }

    #[test]
    fn valid_media_filename_rejects_traversal_and_other_shapes() {
        assert!(!valid_media_filename("../../../etc/passwd"));
        assert!(!valid_media_filename("evil.svg"));
        assert!(!valid_media_filename("evil.html"));
        assert!(!valid_media_filename("no-extension"));
        assert!(!valid_media_filename(
            "3fa85f64-5717-4562-b3fc-2c963f66afa6/../x.png"
        ));
        assert!(!valid_media_filename(
            "not-a-uuid-at-all-but-36-chars-longg.png"
        ));
    }

    #[tokio::test]
    async fn media_eviction_keeps_the_most_recent_files_inside_the_share() {
        let dir = std::env::temp_dir().join(format!("lince-media-budget-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("test dir");
        let mut paths = Vec::new();
        for n in 0..4_u64 {
            let path = dir.join(format!("00000000-0000-4000-8000-{n:012}.png"));
            tokio::fs::write(&path, vec![n as u8; 30])
                .await
                .expect("image");
            std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("open")
                .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(n))
                .expect("mtime");
            paths.push(path);
        }

        evict_dir_to_quota(&dir, 65, None).await.expect("evict");

        assert!(!tokio::fs::try_exists(&paths[0]).await.expect("oldest"));
        assert!(!tokio::fs::try_exists(&paths[1]).await.expect("older"));
        assert!(tokio::fs::try_exists(&paths[2]).await.expect("newer"));
        assert!(tokio::fs::try_exists(&paths[3]).await.expect("newest"));
        std::fs::remove_dir_all(dir).expect("cleanup");
    }

    #[tokio::test]
    async fn a_just_uploaded_file_wins_a_coarse_timestamp_tie() {
        let dir = std::env::temp_dir().join(format!("lince-media-tie-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("test dir");
        let old = dir.join("ffffffff-ffff-4fff-8fff-ffffffffffff.png");
        let uploaded = dir.join("00000000-0000-4000-8000-000000000000.png");
        for path in [&old, &uploaded] {
            tokio::fs::write(path, vec![0_u8; 40]).await.expect("image");
            std::fs::File::options()
                .write(true)
                .open(path)
                .expect("open")
                .set_modified(std::time::SystemTime::UNIX_EPOCH)
                .expect("mtime");
        }

        evict_dir_to_quota(&dir, 40, Some(&uploaded))
            .await
            .expect("evict");

        assert!(tokio::fs::try_exists(&uploaded).await.expect("uploaded"));
        assert!(!tokio::fs::try_exists(&old).await.expect("old"));
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
    /// The other half of "each area evicts within its share": filling and
    /// evicting MEDIA must leave quarantine exactly as it was. A single global
    /// cap is what this rules out — under one, whichever area grew fastest
    /// would evict the others, and the evidence a peer is misbehaving is the
    /// thing you least want a burst of images to delete.
    #[tokio::test]
    async fn evicting_media_leaves_quarantine_untouched() {
        let store = store::Store::open_memory().await.expect("store");
        store::budget::set_total(&store.pool, 2000)
            .await
            .expect("total");
        for _ in 0..3 {
            store::organs::quarantine(&store.pool, "organ-a", "bad-sig", "the evidence")
                .await
                .expect("quarantine");
        }
        let before = store::organs::quarantined_for(&store.pool, "organ-a", 50)
            .await
            .expect("read")
            .len();
        assert!(before > 0, "there is evidence to protect");

        let dir = std::env::temp_dir().join(format!("lince-media-iso-{}", uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(&dir).await.expect("test dir");
        for n in 0..6_u64 {
            let path = dir.join(format!("00000000-0000-4000-8000-{n:012}.png"));
            tokio::fs::write(&path, vec![n as u8; 300])
                .await
                .expect("image");
            std::fs::File::options()
                .write(true)
                .open(&path)
                .expect("open")
                .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(n))
                .expect("mtime");
        }
        let quota = store::budget::share(2000, store::budget::Area::Media).expect("share");
        evict_dir_to_quota(&dir, quota, None).await.expect("evict");

        let mut left = 0;
        let mut read = tokio::fs::read_dir(&dir).await.expect("read dir");
        while read.next_entry().await.expect("entry").is_some() {
            left += 1;
        }
        assert!(left < 6, "media evicted inside its own share");

        assert_eq!(
            store::organs::quarantined_for(&store.pool, "organ-a", 50)
                .await
                .expect("read")
                .len(),
            before,
            "quarantine is a different share and nothing in it moved"
        );
        std::fs::remove_dir_all(dir).expect("cleanup");
    }
}
