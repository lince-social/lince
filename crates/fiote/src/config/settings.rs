//! Provider configuration, credentials, and validation.

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;
use zeroize::Zeroize;

use crate::runtime::MAX_CONTEXT_BYTES;

const MIN_CONTEXT_BYTES: usize = 8 * 1024;
const MAX_ACCOUNT_BYTES: usize = 160;
const MAX_MODEL_BYTES: usize = 256;

/// A serialized credential with redacted debug output and zeroization on drop.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// The persisted provider identifier.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProviderKind(pub String);

/// Settings for a Fiote's model and working environment.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    /// Whether this Fiote can use the provider.
    pub enabled: bool,
    /// Provider that interprets the remaining settings.
    pub provider: ProviderKind,
    /// Authentication method selected for the provider.
    #[serde(default)]
    pub auth_method: String,
    /// The model identifier sent to the provider.
    pub model: String,
    /// The provider base URL, with a trailing slash after validation.
    pub endpoint: String,
    /// The working directory override. An empty path inherits from the parent.
    pub directory: PathBuf,
    /// Account identifier for a native provider connection.
    #[serde(default)]
    pub account: String,
    /// Optional provider reasoning effort.
    #[serde(default)]
    pub reasoning: Option<String>,
    /// Whether to request the provider's fast mode.
    #[serde(default)]
    pub fast: bool,
    /// Optional limit on retained context bytes.
    #[serde(default)]
    pub context_budget_bytes: Option<usize>,
}

impl Settings {
    /// Validates and normalizes the model, endpoint, and working directory in that order.
    /// Earlier normalizations are retained if a later check fails.
    ///
    /// # Errors
    /// Reports invalid context limits, account details, effort, model, endpoint, or directory.
    pub fn validate(&mut self) -> Result<(), String> {
        if self
            .context_budget_bytes
            .is_some_and(|budget| !(MIN_CONTEXT_BYTES..=MAX_CONTEXT_BYTES).contains(&budget))
        {
            return Err("Choose a context budget between 8 KiB and 512 KiB.".into());
        }
        if self.account.len() > MAX_ACCOUNT_BYTES || self.account.chars().any(char::is_control) {
            return Err("Invalid native account identifier.".into());
        }
        if self.reasoning.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "none" | "minimal" | "low" | "medium" | "high" | "xhigh"
            )
        }) {
            return Err("Choose an available reasoning effort.".into());
        }
        self.model.truncate(self.model.trim_end().len());
        let leading = self.model.len() - self.model.trim_start().len();
        if leading != 0 {
            self.model.drain(..leading);
        }
        if self.model.len() > MAX_MODEL_BYTES {
            return Err("Choose a model name (at most 256 bytes).".into());
        }
        let endpoint = self.endpoint.trim();
        if !endpoint.is_empty() {
            let mut url =
                Url::parse(endpoint).map_err(|_| "The provider endpoint must be a URL.")?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("Keep credentials in the API key field, not in the endpoint.".into());
            }
            let local = url
                .host_str()
                .is_some_and(|host| matches!(host, "localhost" | "127.0.0.1" | "[::1]"));
            if url.scheme() != "https" && !(url.scheme() == "http" && local) {
                return Err("Use HTTPS, or HTTP for a provider on localhost.".into());
            }
            if !url.path().ends_with('/') {
                url.set_path(&format!("{}/", url.path()));
            }
            self.endpoint = url.into();
        }
        if !self.directory.as_os_str().is_empty() {
            if !self.directory.is_absolute() || !self.directory.is_dir() {
                return Err("Choose an existing working directory using its full path, or leave it empty to inherit from the Fiote parent.".into());
            }
            self.directory = self
                .directory
                .canonicalize()
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}
