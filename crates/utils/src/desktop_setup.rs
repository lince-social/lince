use crate::config::lince_data_dir;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::{Error, ErrorKind},
    path::PathBuf,
};

const SETUP_FILE_NAME: &str = "desktop-install-setup.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DesktopInstallSetup {
    #[serde(default)]
    pub start_on_login: Option<bool>,
    #[serde(default)]
    pub start_silent: Option<bool>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub auth_enabled: Option<bool>,
    #[serde(default)]
    pub initial_admin_password: Option<String>,
}

pub fn setup_file_path() -> Result<PathBuf, Error> {
    let data_dir =
        lince_data_dir().ok_or_else(|| Error::other("Unable to resolve Lince data directory"))?;
    Ok(data_dir.join(SETUP_FILE_NAME))
}

pub fn read_staged_setup() -> Result<Option<DesktopInstallSetup>, Error> {
    let path = setup_file_path()?;
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)?;
    toml::from_str(&raw)
        .map(Some)
        .map_err(|error| Error::new(ErrorKind::InvalidData, error))
}

pub fn write_staged_setup(setup: &DesktopInstallSetup) -> Result<(), Error> {
    let path = setup_file_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let raw = toml::to_string_pretty(setup).map_err(Error::other)?;
    write_private_file(path, raw)
}

pub fn remove_staged_setup() -> Result<(), Error> {
    let path = setup_file_path()?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub fn detected_language_default() -> Option<String> {
    detected_locale_is_portuguese_or_brazil().then(|| "pt-BR".to_string())
}

pub fn detected_locale_is_portuguese_or_brazil() -> bool {
    let locale = ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"]
        .into_iter()
        .filter_map(|key| env::var(key).ok())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    locale.contains("pt")
        || locale.contains("portuguese")
        || locale.contains("portugues")
        || locale.contains("brasil")
        || locale.contains("brazil")
}

#[cfg(unix)]
fn write_private_file(path: PathBuf, raw: String) -> Result<(), Error> {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt};

    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(raw.as_bytes())
}

#[cfg(not(unix))]
fn write_private_file(path: PathBuf, raw: String) -> Result<(), Error> {
    fs::write(path, raw)
}
