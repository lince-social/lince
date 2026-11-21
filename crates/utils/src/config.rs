// non-lean-yet
// Scope: static LINCE_DATA_DIR_OVERRIDE, fn set_lince_data_dir_override, fn lince_data_dir, fn
//   lince_cache_dir.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   19.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 5.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 5, 22,
//   29.

use std::{io::Error, path::PathBuf, sync::OnceLock};

static LINCE_DATA_DIR_OVERRIDE: OnceLock<PathBuf> = OnceLock::new();

pub fn set_lince_data_dir_override(path: PathBuf) -> Result<(), Error> {
    if let Some(existing) = LINCE_DATA_DIR_OVERRIDE.get() {
        if existing == &path {
            return Ok(());
        }

        return Err(Error::other(format!(
            "Lince data directory override already set to {}",
            existing.display()
        )));
    }

    LINCE_DATA_DIR_OVERRIDE
        .set(path)
        .map_err(|_| Error::other("Failed to set Lince data directory override"))
}

pub fn lince_data_dir() -> Option<PathBuf> {
    LINCE_DATA_DIR_OVERRIDE
        .get()
        .cloned()
        .or_else(|| dirs::config_dir().map(|dir| dir.join("lince")))
}

pub fn lince_cache_dir() -> Option<PathBuf> {
    LINCE_DATA_DIR_OVERRIDE
        .get()
        .map(|dir| dir.join("cache"))
        .or_else(|| dirs::cache_dir().map(|dir| dir.join("lince")))
}
