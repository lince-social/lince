use std::path::PathBuf;

pub fn crate_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn static_dir() -> PathBuf {
    crate_root_dir().join("static")
}

pub fn board_state_path() -> PathBuf {
    web_config_dir().join("board-state.json")
}

pub fn sand_dir() -> PathBuf {
    web_config_dir().join("sand")
}

/// Where uploaded body images live (2026-07-17) — the ONLY directory a
/// record body's `![](...)` can ever point an image at on disk. Files here
/// get opaque generated names; see `presentation::http::media_assets`.
pub fn media_dir() -> PathBuf {
    web_config_dir().join("media")
}

pub fn web_config_dir() -> PathBuf {
    config_root_dir().join("web")
}

fn config_root_dir() -> PathBuf {
    utils::config::lince_data_dir()
        .unwrap_or_else(|| crate_root_dir().join(".config").join("lince"))
}
