use std::env::consts::{ARCH, OS};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn revision() -> &'static str {
    option_env!("LINCE_REVISION")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
}

pub fn short_revision() -> &'static str {
    let full = revision();
    match full.char_indices().nth(12) {
        Some((byte, _)) => &full[..byte],
        None => full,
    }
}

pub fn target_triple() -> Option<&'static str> {
    match (OS, ARCH) {
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-gnu"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("windows", "x86_64") => Some("x86_64-pc-windows-msvc"),
        _ => None,
    }
}

pub fn current_exe_replaceable() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    if exe.starts_with("/nix/store") {
        return false;
    }
    let Some(parent) = exe.parent() else {
        return false;
    };
    is_writable(&exe) && is_writable(parent)
}

fn is_writable(path: &std::path::Path) -> bool {
    match std::fs::metadata(path) {
        Ok(metadata) => !metadata.permissions().readonly(),
        Err(_) => false,
    }
}

pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_revision_caps_at_twelve() {
        assert!(short_revision().len() <= 12);
    }

    #[test]
    fn version_is_the_crate_version() {
        assert_eq!(VERSION, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn target_triple_is_known_here() {
        assert!(target_triple().is_some());
    }

    #[test]
    fn revision_reflects_the_build_env() {
        match option_env!("LINCE_REVISION") {
            Some(value) if !value.trim().is_empty() => assert_eq!(revision(), value.trim()),
            _ => assert_eq!(revision(), "unknown"),
        }
    }
}
