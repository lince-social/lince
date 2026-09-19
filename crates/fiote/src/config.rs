use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use zeroize::Zeroize;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(pub String);

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret([redacted])")
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    OpenAi,
    Anthropic,
    Gemini,
    Ollama,
}

impl ProviderKind {
    pub const ALL: [Self; 4] = [Self::OpenAi, Self::Anthropic, Self::Gemini, Self::Ollama];

    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI compatible",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Gemini",
            Self::Ollama => "Ollama",
        }
    }

    pub fn endpoint(self) -> &'static str {
        match self {
            Self::OpenAi => "https://api.openai.com/v1/",
            Self::Anthropic => "https://api.anthropic.com/v1/",
            Self::Gemini => "https://generativelanguage.googleapis.com/v1beta/",
            Self::Ollama => "http://localhost:11434/",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Settings {
    pub enabled: bool,
    pub provider: ProviderKind,
    pub model: String,
    pub endpoint: String,
    pub directory: PathBuf,
}

impl Settings {
    pub fn validate(&mut self) -> Result<(), String> {
        self.model = self.model.trim().to_string();
        if self.model.is_empty() || self.model.len() > 256 {
            return Err("Choose a model name (at most 256 bytes).".into());
        }
        if self.endpoint.trim().is_empty() {
            self.endpoint = self.provider.endpoint().into();
        }
        let mut url = url::Url::parse(self.endpoint.trim())
            .map_err(|_| "The provider endpoint must be a URL.")?;
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
        self.endpoint = url.to_string();
        if !self.directory.is_absolute() || !self.directory.is_dir() {
            return Err("Choose an existing folder using its full path.".into());
        }
        self.directory = self.directory.canonicalize().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Inspect {
        record: String,
    },
    Configure {
        record: String,
        settings: Settings,
        api_key: Option<Secret>,
    },
    Stop {
        thread: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub record: String,
    pub settings: Settings,
    pub has_key: bool,
    pub running: Vec<String>,
}
