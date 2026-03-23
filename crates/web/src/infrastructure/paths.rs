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

pub fn widget_builder_prompt_dir() -> PathBuf {
    workspace_root_dir()
        .join("documentation")
        .join("ops")
        .join("ai_widget_builder_prompt")
}

pub fn board_state_path() -> PathBuf {
    web_config_dir().join("board-state.json")
}

pub fn package_dir() -> PathBuf {
    web_config_dir().join("widgets")
}

pub fn sand_dir() -> PathBuf {
    web_config_dir().join("sand")
}

pub fn web_config_dir() -> PathBuf {
    config_root_dir().join("lince").join("web")
}

fn config_root_dir() -> PathBuf {
    dirs::config_dir().unwrap_or_else(|| crate_root_dir().join(".config"))
}
