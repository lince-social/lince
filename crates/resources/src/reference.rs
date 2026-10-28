use crate::{Library, Resource, Result, Url, address, resolve};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, path::Path};

#[derive(Clone, Debug, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Reference {
    pub target: String,
    pub base: Option<String>,
    pub locations: Vec<String>,
    pub uid: Option<String>,
    pub hash: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    pub uri: String,
    pub fragment: Option<String>,
    pub uid: Option<String>,
    pub hash: Option<String>,
}

pub fn record_uid(value: &str) -> bool {
    value.strip_prefix("r_").is_some_and(|value| {
        value.len() == 26
            && value.bytes().all(|byte| {
                matches!(byte, b'0'..=b'9' | b'A'..=b'H' | b'J'..=b'K' | b'M'..=b'N' | b'P'..=b'T' | b'V'..=b'Z')
            })
    })
}

pub fn is_id(value: &str) -> bool {
    value.starts_with('@') || record_uid(value)
}

pub fn is_uri(value: &str) -> bool {
    Url::parse(value).is_ok_and(|uri| matches!(uri.scheme(), "file" | "http" | "https"))
}

pub fn is_address(value: &str) -> bool {
    is_uri(value)
        || value.contains(['/', '\\'])
        || value.starts_with('.')
        || value.ends_with(".lingua")
}

impl Reference {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            ..Default::default()
        }
    }

    pub fn at(target: impl Into<String>, base: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            base: Some(base.into()),
            ..Default::default()
        }
    }

    pub fn path(path: &Path) -> Result<Self> {
        let path = std::path::absolute(path).map_err(|error| error.to_string())?;
        let uri = Url::from_file_path(path).map_err(|_| "Invalid local file path")?;
        Ok(Self::new(uri.to_string()))
    }

    pub fn addresses(&self, library: &Library) -> Result<Vec<Address>> {
        if self.locations.len() > 64
            || self.target.len() > 8192
            || self.locations.iter().any(|value| {
                value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control)
            })
            || self.uid.as_deref().is_some_and(|uid| !record_uid(uid))
            || self.hash.as_ref().is_some_and(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err("Reference exceeds its address limit".into());
        }
        let bare_hash = self.base.is_none()
            && self.target.len() == 64
            && self.target.bytes().all(|byte| byte.is_ascii_hexdigit());
        let expected_hash = self
            .hash
            .clone()
            .or_else(|| bare_hash.then(|| self.target.to_ascii_lowercase()));
        let identity = self.target.strip_prefix('@').unwrap_or(&self.target);
        let uid = self
            .uid
            .clone()
            .or_else(|| record_uid(identity).then(|| identity.to_owned()))
            .or_else(|| {
                let primary = if let Some(base) = &self.base {
                    resolve(base, &self.target)
                        .ok()
                        .and_then(|uri| address(&uri).ok())
                } else {
                    address(&self.target).ok()
                };
                primary.and_then(|uri| {
                    uri.fragment()
                        .filter(|fragment| record_uid(fragment))
                        .map(str::to_owned)
                })
            });
        let mut values = Vec::new();
        if !is_id(&self.target) && !bare_hash && !self.target.is_empty() {
            values.push(self.target.clone());
        }
        values.extend(self.locations.clone());
        let known = if let Some(uid) = &uid {
            library.reference_entries(uid)?
        } else {
            Vec::new()
        };
        let revisions: std::collections::BTreeSet<_> = known
            .iter()
            .filter_map(|entry| entry.hash.as_ref())
            .collect();
        if is_id(&self.target)
            && self.locations.is_empty()
            && self.hash.is_none()
            && revisions.len() > 1
        {
            return Err("Several known source revisions define this identity; choose a source URL or an exact content hash".into());
        }
        values.extend(
            known
                .iter()
                .filter(|entry| {
                    self.hash.as_ref().is_none_or(|hash| {
                        entry
                            .hash
                            .as_ref()
                            .is_none_or(|expected| hash.eq_ignore_ascii_case(expected))
                    })
                })
                .map(|entry| entry.uri.clone()),
        );
        if values.is_empty()
            && let Some(hash) = &expected_hash
            && let Some(entry) = library.find(&Resource {
                hash: Some(hash.clone()),
                ..Default::default()
            })?
        {
            values.push(entry.uri);
        }
        let mut output = Vec::new();
        for value in values {
            let uri = if let Some(base) = &self.base {
                resolve(base, &value)
            } else {
                address(&value).map(|uri| uri.to_string())
            };
            let Ok(uri) = uri else {
                continue;
            };
            let mut uri = address(&uri)?;
            if uri.scheme() == "file" && !library.allows_local_files() {
                continue;
            }
            let fragment = uri.fragment().map(str::to_owned);
            uri.set_fragment(None);
            let fragment_uid = fragment.as_ref().filter(|value| record_uid(value));
            if let (Some(expected), Some(actual)) = (&uid, fragment_uid)
                && expected != actual
            {
                return Err(
                    "The address fragment conflicts with the requested Record identity".into(),
                );
            }
            let candidate = Address {
                uri: uri.to_string(),
                uid: uid.clone().or_else(|| fragment_uid.cloned()),
                fragment: fragment.or_else(|| uid.clone()),
                hash: expected_hash.clone().or_else(|| {
                    is_id(&self.target)
                        .then(|| known.first().and_then(|entry| entry.hash.clone()))
                        .flatten()
                }),
            };
            if !output.contains(&candidate) {
                output.push(candidate);
            }
        }
        if output.is_empty() {
            return Err(format!("No source location is known for {}", self.target));
        }
        Ok(output)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Location {
    uri: String,
    hash: Option<String>,
}

impl Library {
    fn reference_entries(&self, uid: &str) -> Result<Vec<Location>> {
        let path = self.root().join("files/references.json");
        let entries: BTreeMap<String, Vec<Location>> = match fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.to_string()),
        };
        Ok(entries.get(uid).cloned().unwrap_or_default())
    }

    pub fn reference_locations(&self, uid: &str) -> Result<Vec<String>> {
        Ok(self
            .reference_entries(uid)?
            .into_iter()
            .map(|entry| entry.uri)
            .collect())
    }

    pub fn remember_reference(&self, uid: &str, uri: &str) -> Result<()> {
        self.remember_definition(uid, uri, None)
    }

    pub fn remember_definition(&self, uid: &str, uri: &str, hash: Option<&str>) -> Result<()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        if !record_uid(uid) {
            return Err("A source location needs a valid authored Record UID".into());
        }
        let mut uri = address(uri)?;
        uri.set_fragment(Some(uid));
        let entry = Location {
            uri: uri.to_string(),
            hash: hash.map(str::to_owned),
        };
        let _guard = LOCK.lock().map_err(|error| error.to_string())?;
        let files = self.root().join("files");
        fs::create_dir_all(&files).map_err(|error| error.to_string())?;
        let path = files.join("references.json");
        let mut entries: BTreeMap<String, Vec<Location>> = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| error.to_string())?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(error) => return Err(error.to_string()),
        };
        let locations = entries.entry(uid.into()).or_default();
        if !locations.contains(&entry) {
            locations.insert(0, entry);
            locations.truncate(64);
        }
        let mut temporary =
            tempfile::NamedTempFile::new_in(files).map_err(|error| error.to_string())?;
        serde_json::to_writer(&mut temporary, &entries).map_err(|error| error.to_string())?;
        temporary.persist(path).map_err(|error| error.to_string())?;
        Ok(())
    }
}
