use std::{ffi::OsString, path::Path};

#[cfg(target_os = "linux")]
#[path = "maintenance/browser.rs"]
mod browser;
#[path = "maintenance/fiote.rs"]
mod fiote;

pub(crate) type Result<T> = std::result::Result<T, String>;

pub(crate) fn dispatch(root: &Path, args: &[OsString]) -> Result<()> {
    match args.first().and_then(|arg| arg.to_str()) {
        Some("fiote") => fiote::dispatch(root, &args[1..]),
        #[cfg(target_os = "linux")]
        Some("browser") => browser::run(&args[1..]),
        #[cfg(target_os = "linux")]
        Some("browser-helper") => browser::helper(&args[1..]),
        _ => Err("Usage: cargo xtask maintenance fiote <export|validate|render> [asset-directory]; Park browser validation uses cargo xtask park validate browser".into()),
    }
}
