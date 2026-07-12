//! Content-type helper for the `/sand/{*path}` route served by
//! `serve_cell_api_only` in `lib.rs`.
//!
//! Sands are no longer embedded here as source. They are built in Rust
//! (`sand::render_official_widgets` for single sands, `render_official_groups`
//! for sand groups) and written to `<lince_data_dir>/web/sand/` at boot, then
//! served flat at `/sand/{*path}`. This module only classifies content types
//! for that route now.

pub(crate) fn guess_content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if path.ends_with(".json") {
        "application/json"
    } else if path.ends_with(".toml") {
        "application/toml"
    } else {
        "application/octet-stream"
    }
}
