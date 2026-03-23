use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Error,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct BootstrapConfig {
    pub auth_enabled: bool,
    pub secret: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BootstrapConfigFile {
    #[serde(default)]
    auth: AuthConfigFile,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AuthConfigFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret: Option<String>,
}

pub fn load_or_init_bootstrap_config() -> Result<BootstrapConfig, Error> {
    let path = bootstrap_config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file_existed = path.exists();
    let mut raw = if file_existed {
        let content = fs::read_to_string(&path)?;
        toml::from_str::<BootstrapConfigFile>(&content).map_err(Error::other)?
    } else {
        BootstrapConfigFile::default()
    };

    let mut changed = !file_existed;
    let secret = match raw
        .auth
        .secret
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(secret) => secret.to_string(),
        None => {
            let next = uuid::Uuid::new_v4().simple().to_string();
            raw.auth.secret = Some(next.clone());
            changed = true;
            next
        }
    };

    if changed {
        persist_bootstrap_config(&path, &raw)?;
    }

    Ok(BootstrapConfig {
        auth_enabled: raw.auth.enabled.unwrap_or(false),
        secret,
    })
}

fn bootstrap_config_path() -> Result<PathBuf, Error> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| Error::other("Unable to resolve user config directory"))?;
    Ok(config_dir.join("lince").join("lince.toml"))
}

fn persist_bootstrap_config(path: &Path, config: &BootstrapConfigFile) -> Result<(), Error> {
    let raw = toml::to_string_pretty(config).map_err(Error::other)?;
    fs::write(path, raw)?;
    Ok(())
}
