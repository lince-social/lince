use std::{env, path::PathBuf};

pub fn crate_root_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn workspace_root_dir() -> PathBuf {
    crate_root_dir()
        .parent()
        .and_then(|path| path.parent())
        .map(std::path::Path::to_path_buf)
        .unwrap_or_else(crate_root_dir)
}

pub fn static_dir() -> PathBuf {
    crate_root_dir().join("static")
}

pub fn board_state_path() -> PathBuf {
    web_config_dir().join("board-state.json")
}

pub fn package_dir() -> PathBuf {
    web_config_dir().join("widgets")
}

pub fn package_examples_dir() -> PathBuf {
    crate_root_dir().join("view-examples")
}

pub fn server_profiles_path() -> PathBuf {
    web_config_dir().join("servers.json")
}

pub fn web_config_dir() -> PathBuf {
    config_root_dir().join("lince").join("web")
}

fn config_root_dir() -> PathBuf {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| crate_root_dir().join(".config"))
}
