use std::{
    io::Error,
    path::PathBuf,
    sync::OnceLock,
};

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
