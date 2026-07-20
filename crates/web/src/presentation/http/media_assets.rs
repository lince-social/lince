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
    tokio::fs::write(dir.join(&name), bytes)
        .await
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?;
    Ok(format!("/host/media/{name}"))
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
pub(crate) async fn pick_and_store_image()
-> Result<Option<String>, (axum::http::StatusCode, String)> {
    let Some(handle) = rfd::AsyncFileDialog::new()
        .add_filter("Image", &["png", "jpg", "jpeg", "gif", "webp"])
        .pick_file()
        .await
    else {
        return Ok(None);
    };
    let bytes = handle.read().await;
    store_media_bytes(&bytes).await.map(Some)
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
}
