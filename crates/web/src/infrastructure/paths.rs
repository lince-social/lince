use std::path::PathBuf;

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
    crate_root_dir().join(".lince-board-state.json")
}

pub fn package_dir() -> PathBuf {
    crate_root_dir().join("lince-views")
}

pub fn package_examples_dir() -> PathBuf {
    crate_root_dir().join("view-examples")
}
