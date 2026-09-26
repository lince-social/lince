pub use ::fiote::speech::{Job, Model, Provider, Request, Settings, Status};
use ::fiote::{acp, speech};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

pub struct Host {
    path: PathBuf,
    state: Mutex<State>,
}

struct State {
    settings: Settings,
    saved: Settings,
    committed: bool,
    connection: Option<Arc<acp::Connection>>,
    providers: Vec<Provider>,
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
        let settings = if path.exists() {
            let bytes = std::fs::read(&path).map_err(|error| error.to_string())?;
            if bytes.len() > 32_768 {
                return Err("Speech settings exceed their size limit.".into());
            }
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
        } else {
            Settings::default()
        };
        Ok(Self {
            path,
            state: Mutex::new(State {
                saved: settings.clone(),
                settings,
                committed: true,
                connection: None,
                providers: Vec::new(),
                job: None,
                used: VecDeque::new(),
            }),
        })
    }
}

#[async_trait::async_trait]
impl transport::speech::Service for Host {
    async fn handle(&self, request: Request) -> Result<Status, String> {
        let mut state = self.state.lock().await;
        let include_job = matches!(&request, Request::Start { .. } | Request::Poll { .. });
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
            if state.job.as_ref().is_some_and(|job| job.result.pending) {
                if let Some(connection) = state.connection.take() {
                    connection.close();
                }
            }
            state.job = None;
        }
        match request {
            Request::Inspect { settings } => {
                idle(&state)?;
                let preview = settings.is_some();
                let settings = settings.unwrap_or_else(|| state.saved.clone());
                let (connection, providers) = speech::discover(&settings).await?;
                if let Some(old) = state.connection.replace(connection) {
                    old.close();
                }
                state.providers = providers;
                state.settings = settings;
                state.committed = !preview;
            }
            Request::Configure {
                settings,
                credential,
            } => {
                idle(&state)?;
                let (connection, providers) = speech::discover(&settings).await?;
                let providers =
                    speech::configure(&connection, &settings, &providers, credential).await?;
                save(&self.path, &settings)?;
                if let Some(old) = state.connection.replace(connection) {
                    old.close();
                }
                state.providers = providers;
                state.saved = settings.clone();
                state.settings = settings;
                state.committed = true;
            }
            Request::Start {
                job,
                settings,
                audio,
            } => {
                if settings != state.settings {
                    return Err("Speech settings changed while recording. No audio was sent. Check the current settings before recording again.".into());
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
                if !state.committed {
                    return Err("Save speech settings before recording.".into());
                }
                speech::validate_audio(&audio.0)?;
                if state
                    .connection
                    .as_ref()
                    .is_none_or(|connection| connection.is_closed())
                {
                    let (connection, providers) = speech::discover(&state.settings).await?;
                    state.connection = Some(connection);
                    state.providers = providers;
                } else {
                    state.providers = speech::catalog(state.connection.as_ref().unwrap()).await?;
                }
                speech::ready(&state.settings, &state.providers)?;
                let connection = state.connection.as_ref().unwrap().clone();
                let provider = state.settings.provider.clone();
                let task = tokio::spawn(speech::transcribe(connection, provider, audio));
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
                let running = state
                    .job
                    .as_mut()
                    .filter(|running| running.result.id == job)
                    .ok_or("This dictation result is no longer available.")?;
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
                    let was_pending = state.job.as_ref().unwrap().result.pending;
                    state.job = None;
                    if was_pending {
                        if let Some(connection) = state.connection.take() {
                            connection.close();
                        }
                    }
                }
            }
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
    let readiness = speech::ready(&state.settings, &state.providers);
    let ready = state.committed && readiness.is_ok();
    let detail = if !state.committed {
        "Save these speech settings before recording.".into()
    } else if let Err(error) = readiness {
        error
    } else {
        "Speech configuration is ready. This check sends no audio and does not verify a billed transcription. Speech usage and cost are not reported by this tool.".into()
    };
    Status {
        settings: state.settings.clone(),
        providers: state.providers.clone(),
        ready,
        detail,
        job: state.job.as_ref().map(|job| job.result.clone()),
    }
}

fn save(path: &Path, settings: &Settings) -> Result<(), String> {
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
mod tests {
    use super::*;
    use transport::speech::Service;
    #[tokio::test]
    #[ignore = "Requires LINCE_TEST_AGENT_BIN; transcribes a local fixture without a Fiote or model"]
    async fn speech_works_without_fiote_deduplicates_and_clears_private_results() {
        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("speech-log");
        let settings = Settings {
            command: std::env::var("LINCE_TEST_AGENT_BIN").unwrap().into(),
            args: vec![
                "--speech-fixture".into(),
                format!("--log={}", log.display()),
            ],
            directory: root.path().into(),
            provider: "local".into(),
            model: "fixture".into(),
            allow_cloud: false,
        };
        let host = Host::open(root.path().join("speech.json")).unwrap();
        let status = host
            .handle(Request::Configure {
                settings: settings.clone(),
                credential: None,
            })
            .await
            .unwrap();
        assert!(status.ready, "{}", status.detail);
        let mut preview = settings.clone();
        preview.allow_cloud = true;
        let status = host
            .handle(Request::Inspect {
                settings: Some(preview.clone()),
            })
            .await
            .unwrap();
        assert_eq!(status.settings, preview);
        assert!(!status.ready);
        let status = host
            .handle(Request::Inspect { settings: None })
            .await
            .unwrap();
        assert_eq!(status.settings, settings);
        assert!(status.ready);
        let job = nucleus::new_uid("speech");
        let request = Request::Start {
            job: job.clone(),
            settings: settings.clone(),
            audio: ::fiote::config::Secret(
                "UklGRjQAAABXQVZFZm10IBAAAAABAAEAgD4AAAB9AAACABAAZGF0YRAAAAABAQEBAQEBAQEBAQEBAQEB"
                    .into(),
            ),
        };
        host.handle(request.clone()).await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), async {
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
            result.job.unwrap().text.as_deref(),
            Some("Fixture transcript")
        );
        assert!(
            host.handle(request.clone())
                .await
                .unwrap()
                .job
                .unwrap()
                .text
                .is_some()
        );
        let status = host
            .handle(Request::Cancel { job: job.clone() })
            .await
            .unwrap();
        assert!(status.job.is_none());
        assert!(host.handle(request).await.is_err());
        assert!(host.handle(Request::Poll { job }).await.is_err());
        let restarted = Host::open(root.path().join("speech.json")).unwrap();
        assert_eq!(
            restarted
                .handle(Request::Inspect { settings: None })
                .await
                .unwrap()
                .settings,
            settings
        );
        let requests = std::fs::read_to_string(log).unwrap();
        assert_eq!(
            requests
                .lines()
                .filter(|line| *line == "_goose/unstable/dictation/transcribe")
                .count(),
            1
        );
        assert!(!requests.contains("session/prompt"));
        assert!(!requests.contains("session/new"));
        assert!(
            !std::fs::read_to_string(root.path().join("speech.json"))
                .unwrap()
                .contains("Fixture transcript")
        );
    }
}
