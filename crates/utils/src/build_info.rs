use std::env::consts::{ARCH, OS};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn revision() -> &'static str {
    option_env!("LINCE_REVISION")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown")
}

pub fn revision_is_stamped() -> bool {
    revision() != "unknown"
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelfApplyBlock {
    None,
    Windows,
    UnknownTarget,
    NixStore,
    NotAppImage,
    ReadOnly,
}

impl SelfApplyBlock {
    pub fn message(self) -> &'static str {
        match self {
            SelfApplyBlock::None => "This build can replace itself.",
            SelfApplyBlock::Windows => {
                "Automatic replacement is not supported on Windows yet. Download the installer from the release page."
            }
            SelfApplyBlock::UnknownTarget => {
                "This platform has no matching release asset. Update by hand."
            }
            SelfApplyBlock::NixStore => {
                "Running from the Nix store. Update with: nix flake update lince && nixos-rebuild switch (or home-manager switch)."
            }
            SelfApplyBlock::NotAppImage => {
                "This build is not an AppImage, so it cannot swap itself. Use your package manager or download the new AppImage."
            }
            SelfApplyBlock::ReadOnly => {
                "The running program's file is read-only. Update through whatever installed it."
            }
        }
    }
}

pub fn self_apply_block() -> SelfApplyBlock {
    self_apply_block_for(false)
}

pub fn self_apply_block_for(server: bool) -> SelfApplyBlock {
    if OS == "windows" {
        return SelfApplyBlock::Windows;
    }
    if target_triple().is_none() {
        return SelfApplyBlock::UnknownTarget;
    }
    let Ok(exe) = std::env::current_exe() else {
        return SelfApplyBlock::ReadOnly;
    };
    if exe.starts_with("/nix/store") {
        return SelfApplyBlock::NixStore;
    }
    if OS == "linux" {
        match std::env::var_os("APPIMAGE").map(PathBuf::from) {
            Some(appimage) => {
                if !is_writable(&appimage)
                    || appimage
                        .parent()
                        .map(|parent| !is_writable(parent))
                        .unwrap_or(true)
                {
                    return SelfApplyBlock::ReadOnly;
                }
                return SelfApplyBlock::None;
            }
            None if !server => return SelfApplyBlock::NotAppImage,
            None => {}
        }
    }
    let Some(parent) = exe.parent() else {
        return SelfApplyBlock::ReadOnly;
    };
    if is_writable(&exe) && is_writable(parent) {
        SelfApplyBlock::None
    } else {
        SelfApplyBlock::ReadOnly
    }
}

pub fn current_exe_replaceable() -> bool {
    self_apply_block() == SelfApplyBlock::None
}

pub fn appimage_path() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from)
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
