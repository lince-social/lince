use crate::{domain::lince_package::slugify, infrastructure::paths};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, sync::Arc};

const DEFAULT_SERVER_ID: &str = "local-dev";
const DEFAULT_SERVER_NAME: &str = "Local Lince";
const DEFAULT_SERVER_BASE_URL: &str = "http://127.0.0.1:6174";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

#[derive(Clone)]
pub struct ServerProfileStore {
    path: Arc<PathBuf>,
}

impl ServerProfileStore {
    pub fn new() -> Result<Self, String> {
        let path = paths::server_profiles_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Nao consegui criar a pasta de configuracao web: {error}"))?;
        }

        let store = Self {
            path: Arc::new(path),
        };
        store.ensure_seeded()?;
        Ok(store)
    }

    pub fn list(&self) -> Result<Vec<ServerProfile>, String> {
        let mut profiles = self.load_profiles()?;
        profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        Ok(profiles)
    }

    pub fn get(&self, server_id: &str) -> Result<Option<ServerProfile>, String> {
        let server_id = server_id.trim();
        if server_id.is_empty() {
            return Ok(None);
        }

        Ok(self
            .load_profiles()?
            .into_iter()
            .find(|profile| profile.id == server_id))
    }

    pub fn upsert(&self, profile: ServerProfile) -> Result<ServerProfile, String> {
        let profile = normalize_profile(profile)?;
        let mut profiles = self.load_profiles()?;

        if let Some(existing) = profiles.iter_mut().find(|entry| entry.id == profile.id) {
            *existing = profile.clone();
        } else {
            profiles.push(profile.clone());
        }

        self.persist_profiles(&profiles)?;
        Ok(profile)
    }

    pub fn delete(&self, server_id: &str) -> Result<bool, String> {
        let server_id = server_id.trim();
        if server_id.is_empty() {
            return Ok(false);
        }

        let mut profiles = self.load_profiles()?;
        let before = profiles.len();
        profiles.retain(|profile| profile.id != server_id);

        if profiles.len() == before {
            return Ok(false);
        }

        self.persist_profiles(&profiles)?;
        Ok(true)
    }

    fn ensure_seeded(&self) -> Result<(), String> {
        if self.path.exists() {
            let profiles = self.load_profiles()?;
            if profiles.is_empty() {
                self.persist_profiles(&[default_server_profile()])?;
            }
            return Ok(());
        }

        self.persist_profiles(&[default_server_profile()])
    }

    fn load_profiles(&self) -> Result<Vec<ServerProfile>, String> {
        match fs::read_to_string(&*self.path) {
            Ok(raw) => {
                let profiles = serde_json::from_str::<Vec<ServerProfile>>(&raw)
                    .map_err(|error| format!("Nao consegui interpretar servers.json: {error}"))?;
                profiles
                    .into_iter()
                    .map(normalize_profile)
                    .collect::<Result<Vec<_>, _>>()
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(vec![default_server_profile()])
            }
            Err(error) => Err(format!("Nao consegui ler servers.json: {error}")),
        }
    }

    fn persist_profiles(&self, profiles: &[ServerProfile]) -> Result<(), String> {
        let mut normalized = profiles
            .iter()
            .cloned()
            .map(normalize_profile)
            .collect::<Result<Vec<_>, _>>()?;
        normalized.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        let raw = serde_json::to_string_pretty(&normalized)
            .map_err(|error| format!("Nao consegui serializar servers.json: {error}"))?;
        let tmp_path = self.path.with_extension("json.tmp");

        fs::write(&tmp_path, raw)
            .map_err(|error| format!("Nao consegui escrever servers.json: {error}"))?;
        fs::rename(&tmp_path, &*self.path)
            .map_err(|error| format!("Nao consegui finalizar servers.json: {error}"))?;
        Ok(())
    }
}

pub fn default_server_profile() -> ServerProfile {
    let base_url = std::env::var("LINCE_API_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER_BASE_URL.to_string());

    ServerProfile {
        id: DEFAULT_SERVER_ID.to_string(),
        name: DEFAULT_SERVER_NAME.to_string(),
        base_url: normalize_base_url(&base_url),
    }
}

fn normalize_profile(profile: ServerProfile) -> Result<ServerProfile, String> {
    let name = profile.name.trim().to_string();
    let base_url = normalize_base_url(&profile.base_url);
    if name.is_empty() {
        return Err("Server profile precisa definir um name.".into());
    }
    if base_url.is_empty() {
        return Err("Server profile precisa definir um base_url.".into());
    }

    let id = {
        let raw_id = profile.id.trim();
        if raw_id.is_empty() {
            slugify(&name)
        } else {
            slugify(raw_id)
        }
    };

    if id.is_empty() {
        return Err("Server profile precisa definir um id valido.".into());
    }

    Ok(ServerProfile { id, name, base_url })
}

fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}
