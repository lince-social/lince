use ::fiote::speech;
pub use ::fiote::speech::endpoint;
pub use ::fiote::speech::{Job, Request, Status};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
pub use transport::speech::Service;

pub struct Host {
    path: PathBuf,
    state: Mutex<State>,
    vault: Option<Arc<crate::fiote::vault::Vault>>,
}

struct State {
    endpoint: Option<endpoint::Settings>,
    endpoint_ready: bool,
    job: Option<Running>,
    used: VecDeque<String>,
}

struct Running {
    result: Job,
    task: Option<tokio::task::JoinHandle<Result<String, String>>>,
    updated: Instant,
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

impl Host {
    pub fn open(path: PathBuf) -> Result<Self, String> {
        Self::open_with_vault(path, None)
    }

    pub(crate) fn open_with_vault(
        path: PathBuf,
        vault: Option<Arc<crate::fiote::vault::Vault>>,
    ) -> Result<Self, String> {
        let endpoint: Option<endpoint::Settings> =
            match std::fs::read(path.with_extension("endpoint.json")) {
                Ok(bytes) => {
                    if bytes.len() > 32_768 {
                        return Err("Transcription settings exceed their size limit.".into());
                    }
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(error) => return Err(error.to_string()),
            };
        Ok(Self {
            vault,
            path,
            state: Mutex::new(State {
                endpoint_ready: endpoint
                    .as_ref()
                    .is_some_and(|settings| !settings.requires_key),
                endpoint,
                job: None,
                used: VecDeque::new(),
            }),
        })
    }

    async fn endpoint_key(
        &self,
        endpoint: &endpoint::Settings,
    ) -> Result<Option<::fiote::config::Secret>, String> {
        if !endpoint.requires_key {
            return Ok(None);
        }
        match &self.vault {
            Some(vault) if !vault.status().await?.1 => vault.key(&endpoint.slot()).await,
            _ => Ok(None),
        }
    }
}

#[async_trait::async_trait]
impl transport::speech::Service for Host {
    async fn handle(&self, request: Request) -> Result<Status, String> {
        let mut state = self.state.lock().await;
        let include_job = matches!(
            &request,
            Request::StartEndpoint { .. } | Request::Poll { .. }
        );
        if let Some(running) = state.job.as_mut() {
            if running.task.as_ref().is_some_and(|task| task.is_finished()) {
                let result = running
                    .task
                    .take()
                    .unwrap()
                    .await
                    .map_err(|error| error.to_string())
                    .and_then(|result| result);
                running.result.pending = false;
                running.updated = Instant::now();
                match result {
                    Ok(text) => running.result.text = Some(text),
                    Err(error) => running.result.error = Some(error),
                }
            }
        }
        if state
            .job
            .as_ref()
            .is_some_and(|job| job.updated.elapsed() > Duration::from_secs(600))
        {
            state.job = None;
        }
        match request {
            Request::ConfigureEndpoint {
                mut settings,
                credential,
                password,
            } => {
                idle(&state)?;
                settings.validate()?;
                if let Some(password) = password {
                    self.vault
                        .as_ref()
                        .ok_or("This speech host has no credential vault.")?
                        .unlock(password)
                        .await?;
                }
                if let Some(credential) = credential {
                    if credential.0.is_empty() || credential.0.len() > 8192 {
                        return Err("Enter a transcription key of at most 8 KiB.".into());
                    }
                    self.vault
                        .as_ref()
                        .ok_or("This speech host has no credential vault.")?
                        .put(settings.slot(), credential)
                        .await?;
                }
                if settings.requires_key && self.endpoint_key(&settings).await?.is_none() {
                    return Err("Unlock the vault and save the transcription key before configuring this endpoint.".into());
                }
                save_value(&self.path.with_extension("endpoint.json"), &Some(&settings))?;
                state.endpoint = Some(settings);
                state.endpoint_ready = true;
            }
            Request::UnlockEndpoint { password } => {
                self.vault
                    .as_ref()
                    .ok_or("This speech host has no credential vault.")?
                    .unlock(password)
                    .await?;
            }
            Request::Inspect => {
                idle(&state)?;
            }
            Request::StartEndpoint {
                job,
                settings,
                audio,
            } => {
                if state.endpoint.as_ref() != Some(&settings) {
                    return Err("Transcription endpoint settings changed while recording. No audio was sent. Check the current endpoint before recording again.".into());
                }
                if !nucleus::valid_uid(&job, "speech") {
                    return Err("Invalid dictation request identifier.".into());
                }
                if state
                    .job
                    .as_ref()
                    .is_some_and(|running| running.result.id == job)
                {
                    return Ok(status(&state));
                }
                if state.used.contains(&job) {
                    return Err("This recording has already been submitted. Start a new recording to transcribe again.".into());
                }
                idle(&state)?;
                speech::validate_audio(&audio.0)?;
                let key = self.endpoint_key(&settings).await?;
                if settings.requires_key && key.is_none() {
                    return Err(
                        "Unlock the transcription credential vault. No audio was sent.".into(),
                    );
                }
                let task = tokio::spawn(endpoint::transcribe(settings, key, audio));
                state.used.push_back(job.clone());
                if state.used.len() > 32 {
                    state.used.pop_front();
                }
                state.job = Some(Running {
                    result: Job {
                        id: job,
                        pending: true,
                        text: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                });
            }
            Request::Poll { job } => {
                if state
                    .job
                    .as_ref()
                    .is_none_or(|running| running.result.id != job)
                {
                    return Err("This dictation result is no longer available.".into());
                }
            }
            Request::Cancel { job } => {
                if !nucleus::valid_uid(&job, "speech") {
                    return Err("Invalid dictation request identifier.".into());
                }
                if !state.used.contains(&job) {
                    state.used.push_back(job.clone());
                    if state.used.len() > 32 {
                        state.used.pop_front();
                    }
                }
                if state
                    .job
                    .as_ref()
                    .is_some_and(|running| running.result.id == job)
                {
                    state.job = None;
                }
            }
        }
        if let Some(endpoint) = &state.endpoint {
            state.endpoint_ready =
                !endpoint.requires_key || self.endpoint_key(endpoint).await?.is_some();
        }
        let mut result = status(&state);
        if !include_job {
            result.job = None;
        }
        Ok(result)
    }
}

fn idle(state: &State) -> Result<(), String> {
    if state.job.as_ref().is_some_and(|job| job.result.pending) {
        Err("Finish or cancel the current dictation first.".into())
    } else {
        Ok(())
    }
}

fn status(state: &State) -> Status {
    let ready = state.endpoint.is_some() && state.endpoint_ready;
    let detail = if state.endpoint.is_none() {
        "Configure a transcription endpoint before recording."
    } else if !state.endpoint_ready {
        "Unlock the vault and configure the transcription key."
    } else {
        "Speech configuration is ready. This check sends no audio and does not verify a billed transcription. Speech usage and cost are not reported by this tool."
    };
    Status {
        endpoint: state.endpoint.clone(),
        ready,
        detail: detail.into(),
        job: state.job.as_ref().map(|job| job.result.clone()),
    }
}

fn save_value(path: &Path, settings: &impl serde::Serialize) -> Result<(), String> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or("Speech settings need a parent directory.")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec(settings).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod endpoint_tests {
    use super::*;
    use transport::speech::Service;

    #[tokio::test]
    async fn unconfigured_speech_reports_setup_without_starting_an_agent() {
        let root = tempfile::tempdir().unwrap();
        let host = Host::open(root.path().join("speech.json")).unwrap();
        let status = host.handle(Request::Inspect).await.unwrap();
        assert!(!status.ready);
        assert!(status.endpoint.is_none());
        assert!(status.job.is_none());
        assert!(status.detail.contains("Configure a transcription endpoint"));
    }

    #[tokio::test]
    async fn endpoint_transcription_deduplicates_and_clears_private_results() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = endpoint::Settings {
            url: format!(
                "http://{}/audio/transcriptions",
                listener.local_addr().unwrap()
            ),
            model: "fixture".into(),
            allow_cloud: false,
            requires_key: false,
        };
        let requests = Arc::new(AtomicUsize::new(0));
        let observed = requests.clone();
        let server = tokio::spawn(async move {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut chunk = [0; 4096];
                    let length = stream.read(&mut chunk).await.unwrap();
                    assert!(length > 0);
                    bytes.extend_from_slice(&chunk[..length]);
                    if let Some(offset) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                        let header = String::from_utf8(bytes[..offset].to_vec()).unwrap();
                        let length = header
                            .lines()
                            .find_map(|line| {
                                line.to_ascii_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|value| value.parse::<usize>().ok())
                            })
                            .unwrap();
                        if bytes.len() >= offset + 4 + length {
                            assert!(header.starts_with("POST /audio/transcriptions "));
                            break;
                        }
                    }
                    assert!(bytes.len() < 65_536);
                }
                observed.fetch_add(1, Ordering::SeqCst);
                let body = "{\"text\":\"Fixture transcript\"}";
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
            }
        });
        let host = Host::open(path.clone()).unwrap();
        host.handle(Request::ConfigureEndpoint {
            settings: endpoint.clone(),
            credential: None,
            password: None,
        })
        .await
        .unwrap();
        let job = nucleus::new_uid("speech");
        let request = Request::StartEndpoint {
            job: job.clone(),
            settings: endpoint.clone(),
            audio: ::fiote::config::Secret("UklGRjQAAABXQVZFZm10IBAAAAABAAEAgD4AAAB9AAACABAAZGF0YRAAAAABAQEBAQEBAQEBAQEBAQEBAQEB".into()),
        };
        host.handle(request.clone()).await.unwrap();
        host.handle(request.clone()).await.unwrap();
        let status = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let status = host
                    .handle(Request::Poll { job: job.clone() })
                    .await
                    .unwrap();
                if !status.job.as_ref().unwrap().pending {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            status.job.unwrap().text.as_deref(),
            Some("Fixture transcript")
        );
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(host.handle(Request::Inspect).await.unwrap().job.is_none());
        assert!(
            host.handle(Request::Cancel { job: job.clone() })
                .await
                .unwrap()
                .job
                .is_none()
        );
        assert!(host.handle(request).await.is_err());
        assert!(host.handle(Request::Poll { job }).await.is_err());
        let restarted = Host::open(path.clone()).unwrap();
        let status = restarted.handle(Request::Inspect).await.unwrap();
        assert!(status.ready);
        assert_eq!(status.endpoint, Some(endpoint));
        assert!(status.job.is_none());
        assert!(
            !std::fs::read_to_string(path.with_extension("endpoint.json"))
                .unwrap()
                .contains("Fixture transcript")
        );
        server.abort();
    }

    #[tokio::test]
    async fn endpoint_configuration_is_independent_of_harness_and_contains_no_key() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        let host = Host::open(path.clone()).unwrap();
        let endpoint = speech::endpoint::Settings {
            url: "http://127.0.0.1:9876/audio/transcriptions".into(),
            model: "local-speech".into(),
            allow_cloud: false,
            requires_key: false,
        };
        let status = host
            .handle(Request::ConfigureEndpoint {
                settings: endpoint.clone(),
                credential: None,
                password: None,
            })
            .await
            .unwrap();
        assert!(status.ready);
        assert_eq!(status.endpoint, Some(endpoint.clone()));
        let mut stale = endpoint.clone();
        stale.model = "another-model".into();
        assert!(
            host.handle(Request::StartEndpoint {
                job: nucleus::new_uid("speech"),
                settings: stale,
                audio: fiote::config::Secret("invalid".into()),
            })
            .await
            .unwrap_err()
            .contains("settings changed while recording")
        );
        let host = Host::open(path).unwrap();
        let status = host.handle(Request::Inspect).await.unwrap();
        assert!(status.ready);
        assert_eq!(status.endpoint, Some(endpoint));
    }
}
