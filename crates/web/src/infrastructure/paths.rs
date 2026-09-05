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

pub fn media_dir() -> PathBuf {
    web_config_dir().join("media")
}

pub fn dna_dir() -> PathBuf {
    web_config_dir().join("dna").join("sand")
}

pub fn web_config_dir() -> PathBuf {
    config_root_dir().join("web")
}

fn config_root_dir() -> PathBuf {
    utils::config::lince_data_dir()
        .unwrap_or_else(|| crate_root_dir().join(".config").join("lince"))
}
