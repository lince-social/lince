use reqwest::Url;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub base_url: String,
    /// A durable local name keeps Record identities when the server moves.
    pub instance_id: String,
    /// Empty means source links open NOMAD's library settings, without guessing a port.
    pub kiwix_url: String,
    pub include_maps: bool,
    pub include_documents: bool,
    pub include_wikipedia: bool,
    pub batch_size: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8080".into(),
            instance_id: String::new(),
            kiwix_url: String::new(),
            include_maps: false,
            include_documents: false,
            include_wikipedia: false,
            batch_size: 50,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        endpoint(&self.base_url)?;
        if !self.kiwix_url.is_empty() {
            endpoint(&self.kiwix_url)?;
        }
        if self.instance_id.len() > 160
            || self.instance_id.chars().any(char::is_control)
            || self.instance_id.trim() != self.instance_id
            || !(1..=100).contains(&self.batch_size)
        {
            return Err(
                "Use an instance name of at most 160 characters and a batch size of 1–100".into(),
            );
        }
        Ok(())
    }

    pub fn scope(&self) -> Result<String, String> {
        if self.instance_id.is_empty() {
            Ok(endpoint(&self.base_url)?.to_string())
        } else {
            Ok(self.instance_id.clone())
        }
    }

    pub fn url(&self, path: &str) -> Result<Url, String> {
        endpoint(&self.base_url)?
            .join(path)
            .map_err(|e| e.to_string())
    }

    pub fn archive_url(&self, name: &str) -> Result<String, String> {
        if self.kiwix_url.is_empty() {
            return Ok(self.url("settings/zim")?.to_string());
        }
        let mut url = endpoint(&self.kiwix_url)?;
        url.path_segments_mut()
            .map_err(|_| "Invalid Kiwix URL")?
            .pop_if_empty()
            .push("content")
            .push(name.strip_suffix(".zim").unwrap_or(name))
            .push("");
        Ok(url.to_string())
    }
}

pub fn endpoint(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value).map_err(|_| "Use an absolute HTTP(S) server URL")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || value.len() > 4096
    {
        return Err("Use an HTTP(S) server URL without credentials, query or fragment".into());
    }
    // Preserve reverse-proxy path prefixes when joining API routes.
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}
