pub mod endpoint;

use crate::{acp, config::Secret};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub directory: PathBuf,
    pub provider: String,
    pub model: String,
    pub allow_cloud: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            command: Default::default(),
            args: Vec::new(),
            directory: std::env::current_dir().unwrap_or_default(),
            provider: String::new(),
            model: String::new(),
            allow_cloud: false,
        }
    }
}

impl Settings {
    pub fn config(&self) -> Result<acp::Config, String> {
        if self.provider.len() > 128 || self.model.len() > 256 {
            return Err("Speech provider or model name is too long.".into());
        }
        let mut config = acp::Config {
            require_vault: false,
            command: self.command.clone(),
            args: self.args.clone(),
            directory: self.directory.clone(),
            additional_directories: Vec::new(),
            environment: BTreeMap::new(),
            session_meta: Default::default(),
            options: BTreeMap::new(),
        };
        config.validate()?;
        Ok(config)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub id: String,
    pub description: String,
    pub configured: bool,
    pub local: bool,
    pub selected_model: String,
    pub models: Vec<Model>,
    pub credential_key: Option<String>,
    pub model_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Model {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    ConfigureEndpoint {
        settings: endpoint::Settings,
        credential: Option<Secret>,
        password: Option<Secret>,
    },
    UnlockEndpoint {
        password: Secret,
    },
    Inspect {
        settings: Option<Settings>,
    },
    Configure {
        settings: Settings,
        credential: Option<Secret>,
    },
    Start {
        job: String,
        settings: Settings,
        audio: Secret,
    },
    StartEndpoint {
        job: String,
        settings: endpoint::Settings,
        audio: Secret,
    },
    Poll {
        job: String,
    },
    Cancel {
        job: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub endpoint: Option<endpoint::Settings>,
    pub settings: Settings,
    pub providers: Vec<Provider>,
    pub ready: bool,
    pub detail: String,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub pending: bool,
    pub text: Option<String>,
    pub error: Option<String>,
}

pub async fn discover(
    settings: &Settings,
) -> Result<(Arc<acp::Connection>, Vec<Provider>), String> {
    let connection = acp::Connection::open(&settings.config()?).await?;
    if connection
        .info
        .agent_info
        .as_ref()
        .is_none_or(|info| info.name != "goose")
    {
        return Err(
            "This speech adapter needs Goose's dictation interface. No audio was sent.".into(),
        );
    }
    let providers = catalog(&connection).await?;
    Ok((connection, providers))
}

pub async fn catalog(connection: &acp::Connection) -> Result<Vec<Provider>, String> {
    let value = connection
        .extension("_goose/unstable/dictation/config", json!({}))
        .await?;
    if value.to_string().len() > 262_144 {
        return Err("The speech catalog exceeds its size limit.".into());
    }
    let entries = value["providers"]
        .as_object()
        .ok_or("The agent did not return speech providers.")?;
    if entries.len() > 32 {
        return Err("The speech catalog contains too many providers.".into());
    }
    let mut providers = Vec::new();
    for (id, value) in entries {
        let models: Vec<_> = value["availableModels"]
            .as_array()
            .into_iter()
            .flatten()
            .take(256)
            .filter_map(|model| {
                Some(Model {
                    id: model["id"].as_str()?.into(),
                    label: model["label"].as_str().unwrap_or_default().into(),
                })
            })
            .collect();
        providers.push(Provider {
            id: id.clone(),
            description: value["description"].as_str().unwrap_or(id).into(),
            configured: value["configured"] == true,
            local: id == "local",
            selected_model: value["selectedModel"]
                .as_str()
                .or_else(|| value["defaultModel"].as_str())
                .unwrap_or_default()
                .into(),
            models,
            credential_key: value["configKey"].as_str().map(str::to_string),
            model_key: value["modelConfigKey"].as_str().map(str::to_string),
        });
    }
    Ok(providers)
}

pub fn selected<'a>(
    settings: &Settings,
    providers: &'a [Provider],
) -> Result<&'a Provider, String> {
    let provider = providers
        .iter()
        .find(|provider| provider.id == settings.provider)
        .ok_or("Choose an available speech provider.")?;
    if !provider.local && !settings.allow_cloud {
        return Err("Allow cloud transcription explicitly before sending microphone audio to this provider.".into());
    }
    Ok(provider)
}

pub fn ready(settings: &Settings, providers: &[Provider]) -> Result<(), String> {
    let provider = selected(settings, providers)?;
    if !provider.configured {
        return Err(
            "This speech provider needs its own credentials or a downloaded local model.".into(),
        );
    }
    if !settings.model.is_empty() && settings.model != provider.selected_model {
        return Err("The speech model has changed in the agent. Save and check these speech settings again.".into());
    }
    Ok(())
}

pub async fn configure(
    connection: &acp::Connection,
    settings: &Settings,
    providers: &[Provider],
    credential: Option<Secret>,
) -> Result<Vec<Provider>, String> {
    let provider = selected(settings, providers)?;
    if let Some(credential) = credential {
        if credential.0.is_empty() || credential.0.len() > 8192 {
            return Err("Enter a speech credential of at most 8 KiB.".into());
        }
        let key = provider.credential_key.as_ref().ok_or("This provider manages credentials in Goose's provider settings. Configure them there, then check speech again.")?;
        connection
            .extension(
                "_goose/unstable/config/upsert",
                json!({"key":key,"value":credential.0,"isSecret":true}),
            )
            .await?;
    }
    if !settings.model.is_empty() && settings.model != provider.selected_model {
        let key = provider.model_key.as_ref().ok_or("This provider does not expose a separate speech model setting. Choose its current model or leave it empty.")?;
        if !provider
            .models
            .iter()
            .any(|model| model.id == settings.model)
        {
            return Err("Choose one of this speech provider's advertised models.".into());
        }
        connection
            .extension(
                "_goose/unstable/config/upsert",
                json!({"key":key,"value":settings.model,"isSecret":false}),
            )
            .await?;
    }
    catalog(connection).await
}

pub fn validate_audio(audio: &str) -> Result<(), String> {
    if audio.len() > 5_200_000 {
        return Err("Dictation is limited to two minutes of mono speech.".into());
    }
    let bytes = BASE64
        .decode(audio)
        .map_err(|_| "Invalid dictation audio.")?;
    let reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|_| "Dictation requires a PCM WAV recording.")?;
    let spec = reader.spec();
    if spec.channels != 1
        || spec.sample_rate != 16_000
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
        || reader.duration() == 0
        || reader.duration() > 1_920_000
    {
        return Err(
            "Dictation requires 16 kHz mono PCM, between one sample and two minutes.".into(),
        );
    }
    let expected = reader.duration();
    let count = reader
        .into_samples::<i16>()
        .try_fold(0u32, |count, sample| sample.map(|_| count + 1))
        .map_err(|_| "The microphone recording is incomplete.")?;
    if count != expected {
        return Err("The microphone recording is incomplete.".into());
    }
    Ok(())
}

pub async fn transcribe(
    connection: Arc<acp::Connection>,
    provider: String,
    audio: Secret,
) -> Result<String, String> {
    let value = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        connection.extension_wait(
            "_goose/unstable/dictation/transcribe",
            json!({"audio":audio.0,"mimeType":"audio/wav","provider":provider}),
        ),
    )
    .await
    .map_err(|_| "Transcription timed out.".to_string())??;
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .ok_or("The speech provider returned no transcript.")?
        .trim();
    if text.len() > 65_536 {
        return Err("The transcript exceeds the message size limit.".into());
    }
    if text.is_empty() {
        return Err("No speech was recognized. Your draft is unchanged.".into());
    }
    Ok(text.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cloud_needs_explicit_choice_and_model_readiness() {
        let provider = Provider {
            id: "cloud".into(),
            description: String::new(),
            configured: true,
            local: false,
            selected_model: "speech".into(),
            models: Vec::new(),
            credential_key: None,
            model_key: None,
        };
        let mut settings = Settings {
            provider: "cloud".into(),
            model: "speech".into(),
            ..Settings::default()
        };
        assert!(ready(&settings, std::slice::from_ref(&provider)).is_err());
        settings.allow_cloud = true;
        assert!(ready(&settings, std::slice::from_ref(&provider)).is_ok());
        settings.model = "chat".into();
        assert!(ready(&settings, &[provider]).is_err());
    }
}
