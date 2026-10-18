use ::fiote::communication::decision;
use ::fiote::speech;
pub use ::fiote::speech::endpoint;
pub use ::fiote::speech::{Job, LocalStatus, Mode, Purpose, Request, Status};
pub use ::fiote::speech::{ListeningMode, ListeningStage, ListeningStatus, VoiceRequest};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
pub use transport::speech::Service;

mod listening;

#[cfg(test)]
mod local_tests;

#[cfg(test)]
mod wake_tests;

pub struct Host {
    path: PathBuf,
    state: Mutex<State>,
    vault: Option<Arc<crate::fiote::vault::Vault>>,
    local: Arc<speech::local::Recognizer>,
    voice: speech::voice::Service,
    wake_profile: std::sync::Mutex<Option<speech::wake::Profile>>,
    wake_error: std::sync::Mutex<Option<String>>,
    acquisition_progress: Arc<AtomicU64>,
    models: PathBuf,
    decision_progress: Arc<AtomicU64>,
    listening: listening::Controller,
    fiote: Option<Arc<crate::fiote::Host>>,
    closing: AtomicBool,
}

struct State {
    mode: Mode,
    decision_selection: decision::ModelId,
    decision: Arc<decision::Engine>,
    endpoint: Option<endpoint::Settings>,
    endpoint_ready: bool,
    job: Option<Running>,
    used: VecDeque<String>,
}

struct Running {
    result: Job,
    task: Option<tokio::task::JoinHandle<Result<JobResult, String>>>,
    updated: Instant,
    cancelled: Arc<AtomicBool>,
}

enum JobResult {
    Text(String),
    Decision(decision::Output),
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
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
        let mode = match std::fs::read(path.with_extension("mode.json")) {
            Ok(bytes) if bytes.len() <= 256 => {
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
            }
            Ok(_) => return Err("Speech mode settings exceed their size limit.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if endpoint.is_some() {
                    Mode::Endpoint
                } else {
                    Mode::Local
                }
            }
            Err(error) => return Err(error.to_string()),
        };
        let models = path
            .parent()
            .ok_or("Speech settings need a parent directory.")?
            .join("models");
        let decision_selection = match std::fs::read(path.with_extension("decision.json")) {
            Ok(bytes) if bytes.len() <= 256 => {
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?
            }
            Ok(_) => return Err("Decision selection exceeds its size limit.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                decision::ModelId::default()
            }
            Err(error) => return Err(error.to_string()),
        };
        let (wake_profile, wake_error) = match load_wake_profile(&path.with_extension("wake.json")) {
            Ok(profile) => (profile, None),
            Err(error) => (None, Some(error)),
        };
        Ok(Self {
            vault,
            path,
            state: Mutex::new(State {
                mode,
                decision_selection,
                decision: Arc::new(decision::Engine::selected(
                    models.clone(),
                    decision_selection,
                )),
                endpoint_ready: endpoint
                    .as_ref()
                    .is_some_and(|settings| !settings.requires_key),
                endpoint,
                job: None,
                used: VecDeque::new(),
            }),
            local: Arc::new(speech::local::Recognizer::new(models.clone())?),
            voice: speech::voice::Service::new(models.clone())?,
            wake_profile: std::sync::Mutex::new(wake_profile),
            wake_error: std::sync::Mutex::new(wake_error),
            acquisition_progress: Arc::new(AtomicU64::new(0)),
            models,
            decision_progress: Arc::new(AtomicU64::new(0)),
            listening: listening::Controller::default(),
            fiote: None,
            closing: AtomicBool::new(false),
        })
    }

    pub fn with_fiote(mut self, fiote: Option<Arc<crate::fiote::Host>>) -> Self {
        self.fiote = fiote;
        self
    }

    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        let decision = {
            let mut state = self.state.lock().await;
            state.job = None;
            state.decision.clone()
        };
        self.listening.shutdown().await;
        self.voice.shutdown().await;
        while self.local.busy() || decision.busy() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.local.unload();
        let mut state = self.state.lock().await;
        state.decision = Arc::new(decision::Engine::selected(self.models.clone(), state.decision_selection));
    }

    fn available(&self, request: &Request) -> Result<(), String> {
        if self.closing.load(Ordering::Acquire)
            && !matches!(request, Request::Inspect | Request::Poll { .. } | Request::Cancel { .. } | Request::StopListening { .. } | Request::PauseListening { .. })
        {
            return Err("Speech is shutting down. No new work was started.".into());
        }
        Ok(())
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
        self.available(&request)?;
        match &request {
            Request::Voice { request } => {
                let voice = self.voice.handle(request.clone()).await?;
                let mut result = status(&*self.state.lock().await, self);
                result.voice = voice;
                result.listening = self.listening.status().await;
                return Ok(result);
            }
            Request::StartListening { mode, owner, target, routing } => {
                if *mode == ListeningMode::Wake && self.wake_profile.lock().map_err(|_| "Wake setup is unavailable.")?.is_none() {
                    return Err("Record three Hey Fiote examples or import a compatible wake reference before enabling wake mode. Dictation and addressed requests remain available.".into());
                }
                let state = self.state.lock().await;
                self.available(&request)?;
                idle(&state)?;
                let fiote = self.fiote.clone().ok_or("The local Fiote harness is unavailable.")?;
                self.listening.start(*mode, owner.clone(), target.clone(), *routing, self.local.clone(), state.decision.clone(), fiote).await?;
                let mut result = status(&state, self);
                result.listening = self.listening.status().await;
                return Ok(result);
            }
            Request::StopListening { owner, generation } | Request::PauseListening { owner, generation } | Request::ResumeListening { owner, generation } => {
                match &request {
                    Request::PauseListening { .. } => self.listening.pause(owner, *generation).await?,
                    Request::ResumeListening { .. } => self.listening.resume(owner, *generation).await?,
                    _ => self.listening.stop(owner, *generation).await?,
                }
                let mut result = status(&*self.state.lock().await, self);
                result.listening = self.listening.status().await;
                return Ok(result);
            }
            Request::ListeningSegment { owner, generation, segment, audio } => {
                self.listening.push(owner, *generation, *segment, audio.clone()).await?;
                let mut result = status(&*self.state.lock().await, self);
                result.listening = self.listening.status().await;
                return Ok(result);
            }
            Request::WokenSegment { owner, generation, segment, audio } => {
                self.listening.push_woken(owner, *generation, *segment, audio.clone()).await?;
                let mut result = status(&*self.state.lock().await, self);
                result.listening = self.listening.status().await;
                return Ok(result);
            }
            _ => {}
        }
        let mut state = self.state.lock().await;
        self.available(&request)?;
        if self.listening.active() && !matches!(&request, Request::Inspect | Request::Poll { .. } | Request::Cancel { .. }) {
            return Err("Stop general listening and wait for its local inference before changing models or starting dictation.".into());
        }
        let include_job = matches!(
            &request,
            Request::StartEndpoint { .. }
                | Request::StartLocal { .. }
                | Request::InstallLocal { .. }
                | Request::InstallDecision { .. }
                | Request::Decide { .. }
                | Request::Poll { .. }
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
                    Ok(JobResult::Text(text)) => running.result.text = Some(text),
                    Ok(JobResult::Decision(output)) => running.result.decision = Some(output),
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
            Request::Voice { .. } | Request::StartListening { .. } | Request::StopListening { .. } | Request::PauseListening { .. } | Request::ResumeListening { .. } | Request::ListeningSegment { .. } | Request::WokenSegment { .. } => unreachable!(),
            Request::ConfigureWake { profile } => {
                idle(&state)?;
                if let Some(profile) = &profile { profile.validate()?; }
                save_value(&self.path.with_extension("wake.json"), &profile)?;
                *self.wake_profile.lock().map_err(|_| "Wake setup is unavailable.")? = profile;
                *self.wake_error.lock().map_err(|_| "Wake setup is unavailable.")? = None;
            }
            Request::SelectDecision { model } => {
                idle(&state)?;
                if self.local.busy() || state.decision.busy() {
                    return Err("A local model is still finishing an operation. Stop or wait before switching models.".into());
                }
                save_value(&self.path.with_extension("decision.json"), &model)?;
                state.decision_selection = model;
                state.decision = Arc::new(decision::Engine::selected(self.models.clone(), model));
                self.decision_progress.store(0, Ordering::Release);
            }
            Request::InstallDecision { job, source } => {
                if repeated(&state, &job)? {
                    return Ok(status(&state, self));
                }
                idle(&state)?;
                if self.local.busy() || state.decision.busy() {
                    return Err(
                        "A local model is still finishing an operation. Retry when it is idle."
                            .into(),
                    );
                }
                let engine = state.decision.clone();
                let cancelled = Arc::new(AtomicBool::new(false));
                let control = cancelled.clone();
                let progress = self.decision_progress.clone();
                progress.store(0, Ordering::Release);
                let task = tokio::spawn(async move {
                    engine.acquire(source, control, progress).await?;
                    Ok(JobResult::Text(
                        "Local decision model installed and verified.".into(),
                    ))
                });
                remember(&mut state, &job);
                state.job = Some(Running {
                    result: Job {
                        purpose: Purpose::DecisionSetup,
                        id: job,
                        pending: true,
                        text: None,
                        decision: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                    cancelled,
                });
            }
            Request::Decide { job, input } => {
                if repeated(&state, &job)? {
                    return Ok(status(&state, self));
                }
                idle(&state)?;
                if self.local.busy() {
                    return Err("Local speech is still finishing an operation. The decision has not started.".into());
                }
                if !state.decision.present() {
                    return Err("Download or import a local decision model before using routing or admission.".into());
                }
                let cancelled = Arc::new(AtomicBool::new(false));
                let task = state.decision.submit(input, cancelled.clone())?;
                let task = tokio::spawn(async move {
                    task.await
                        .map_err(|error| error.to_string())?
                        .map(JobResult::Decision)
                });
                remember(&mut state, &job);
                state.job = Some(Running {
                    result: Job {
                        purpose: Purpose::Decision,
                        id: job,
                        pending: true,
                        text: None,
                        decision: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                    cancelled,
                });
            }
            Request::SelectMode { mode } => {
                idle(&state)?;
                save_value(&self.path.with_extension("mode.json"), &mode)?;
                state.mode = mode;
            }
            Request::InstallLocal { job, source } => {
                if repeated(&state, &job)? {
                    return Ok(status(&state, self));
                }
                idle(&state)?;
                if self.local.busy() || state.decision.busy() {
                    return Err("Local speech is still finishing an operation. Retry installation when it is idle.".into());
                }
                let cache = self.local.cache.clone();
                let cancelled = Arc::new(AtomicBool::new(false));
                let control = cancelled.clone();
                let progress = self.acquisition_progress.clone();
                progress.store(0, Ordering::Release);
                let task = tokio::spawn(async move {
                    cache.acquire(source, control, progress).await?;
                    Ok(JobResult::Text(
                        "Local English speech model installed and verified.".into(),
                    ))
                });
                remember(&mut state, &job);
                state.job = Some(Running {
                    result: Job {
                        purpose: Purpose::ModelSetup,
                        id: job,
                        pending: true,
                        text: None,
                        decision: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                    cancelled,
                });
            }
            Request::StartLocal { job, audio } => {
                if state.mode != Mode::Local {
                    return Err(
                        "Speech mode changed while recording. No audio was transcribed.".into(),
                    );
                }
                if repeated(&state, &job)? {
                    return Ok(status(&state, self));
                }
                idle(&state)?;
                if state.decision.busy() {
                    return Err("The decision model is still finishing an operation. Try dictation when it is idle.".into());
                }
                speech::validate_audio(&audio.0)?;
                if !self.local.cache.present() {
                    return Err(
                        "Download or import the local English model before recording.".into(),
                    );
                }
                let cancelled = Arc::new(AtomicBool::new(false));
                let task = self.local.submit(audio, cancelled.clone())?;
                let task = tokio::spawn(async move {
                    task.await
                        .map_err(|error| error.to_string())?
                        .map(JobResult::Text)
                });
                remember(&mut state, &job);
                state.job = Some(Running {
                    result: Job {
                        purpose: Purpose::Transcription,
                        id: job,
                        pending: true,
                        text: None,
                        decision: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                    cancelled,
                });
            }
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
                save_value(&self.path.with_extension("mode.json"), &Mode::Endpoint)?;
                state.mode = Mode::Endpoint;
            }
            Request::UnlockEndpoint { password } => {
                self.vault
                    .as_ref()
                    .ok_or("This speech host has no credential vault.")?
                    .unlock(password)
                    .await?;
            }
            Request::Inspect => {}
            Request::StartEndpoint {
                job,
                settings,
                audio,
            } => {
                if state.mode != Mode::Endpoint {
                    return Err("Speech mode changed while recording. No audio was sent.".into());
                }
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
                    return Ok(status(&state, self));
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
                let task = tokio::spawn(async move {
                    endpoint::transcribe(settings, key, audio)
                        .await
                        .map(JobResult::Text)
                });
                state.used.push_back(job.clone());
                if state.used.len() > 32 {
                    state.used.pop_front();
                }
                state.job = Some(Running {
                    result: Job {
                        purpose: Purpose::Transcription,
                        id: job,
                        pending: true,
                        text: None,
                        decision: None,
                        error: None,
                    },
                    task: Some(task),
                    updated: Instant::now(),
                    cancelled: Arc::new(AtomicBool::new(false)),
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
        let mut result = status(&state, self);
        result.listening = self.listening.status().await;
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

fn repeated(state: &State, job: &str) -> Result<bool, String> {
    if !nucleus::valid_uid(job, "speech") {
        return Err("Invalid dictation request identifier.".into());
    }
    if state
        .job
        .as_ref()
        .is_some_and(|running| running.result.id == job)
    {
        return Ok(true);
    }
    if state.used.iter().any(|previous| previous == job) {
        return Err(
            "This recording has already been submitted. Start a new recording to transcribe again."
                .into(),
        );
    }
    Ok(false)
}

fn remember(state: &mut State, job: &str) {
    state.used.push_back(job.into());
    if state.used.len() > 32 {
        state.used.pop_front();
    }
}

fn status(state: &State, host: &Host) -> Status {
    let local = LocalStatus {
        model: speech::local::MANIFEST.id.into(),
        revision: speech::local::MANIFEST.revision.into(),
        present: host.local.cache.present(),
        loaded: host.local.loaded(),
        busy: host.local.busy(),
        acquired_bytes: host.acquisition_progress.load(Ordering::Acquire),
        required_bytes: speech::local::MANIFEST.artifact.bytes,
    };
    let ready = !host.closing.load(Ordering::Acquire)
        && !host.listening.active()
        && !state.job.as_ref().is_some_and(|job| job.result.pending)
        && !state.decision.busy()
        && match state.mode {
            Mode::Local => local.present && !local.busy,
            Mode::Endpoint => state.endpoint.is_some() && state.endpoint_ready,
        };
    let detail = if host.closing.load(Ordering::Acquire) {
        "Speech is shutting down. No new work can start."
    } else if state.mode == Mode::Local {
        if local.busy {
            "Local English speech is working. Cancellation discards its result; CPU work may finish its current chunk."
        } else if local.loaded {
            "Local English speech is ready. Audio stays on this device; the transcript remains an unsent draft."
        } else if local.present {
            "Local English model is available. The first transcription verifies and loads it; audio stays on this device."
        } else {
            "Download or import the local English model, or configure a transcription endpoint."
        }
    } else if state.endpoint.is_none() {
        "Configure a transcription endpoint before recording."
    } else if !state.endpoint_ready {
        "Unlock the vault and configure the transcription key."
    } else {
        "Speech configuration is ready. This check sends no audio and does not verify a billed transcription. Speech usage and cost are not reported by this tool."
    };
    Status {
        wake_profile: host.wake_profile.lock().unwrap_or_else(|error| error.into_inner()).clone(),
        wake_error: host.wake_error.lock().unwrap_or_else(|error| error.into_inner()).clone(),
        voice: speech::voice::Status::default(),
        mode: state.mode,
        decision_selection: state.decision_selection,
        listening: ListeningStatus::default(),
        local: Some(local),
        decision_model: Some(LocalStatus {
            model: state.decision.information().model,
            revision: state.decision.information().revision,
            present: state.decision.present(),
            loaded: state.decision.loaded(),
            busy: state.decision.busy(),
            acquired_bytes: host.decision_progress.load(Ordering::Acquire),
            required_bytes: state.decision.information().required_bytes,
        }),
        decision_capabilities: Some(state.decision.information()),
        endpoint: state.endpoint.clone(),
        ready,
        detail: detail.into(),
        job: state.job.as_ref().map(|job| job.result.clone()),
    }
}

fn load_wake_profile(path: &Path) -> Result<Option<speech::wake::Profile>, String> {
    use std::io::Read;
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read saved wake references: {error}")),
    };
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() > 524_288 { return Err("Saved wake reference file has an unsupported type or size. Import or record it again.".into()); }
    let mut bytes = Vec::new();
    file.take(524_289).read_to_end(&mut bytes).map_err(|error| error.to_string())?;
    if bytes.len() > 524_288 { return Err("Saved wake references exceed their size limit.".into()); }
    let profile: Option<speech::wake::Profile> = serde_json::from_slice(&bytes).map_err(|_| "Saved wake references are invalid. Import or record them again.")?;
    if let Some(profile) = &profile { profile.validate()?; }
    Ok(profile)
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
mod decision_tests {
    use super::*;
    use decision::{Choice, Information, Input, Model, Output};

    struct FixtureModel;

    #[tokio::test]
    async fn decision_selection_persists_without_loading_or_downloading_weights() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        let host = Host::open(path.clone()).unwrap();
        for (model, name) in [
            (decision::ModelId::EnglishNli, "english-nli"),
            (decision::ModelId::Qwen, "qwen3-decision"),
            (decision::ModelId::Laya, "laya-typed-decisions"),
        ] {
            let selected = host
                .handle(Request::SelectDecision { model })
                .await
                .unwrap();
            assert_eq!(selected.decision_selection, model);
            let selected = selected.decision_model.unwrap();
            assert_eq!(selected.model, name);
            assert!(!selected.loaded);
            assert!(!selected.present);
            let reopened = Host::open(path.clone())
                .unwrap()
                .handle(Request::Inspect)
                .await
                .unwrap();
            assert_eq!(reopened.decision_selection, model);
            assert!(!reopened.decision_capabilities.unwrap().admission_validated);
        }
        assert!(!root.path().join("models").exists());
    }

    #[test]
    fn invalid_saved_decision_selection_does_not_choose_a_different_model() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        for bytes in [b"\"unregistered\"".to_vec(), vec![b'x'; 257]] {
            std::fs::write(path.with_extension("decision.json"), bytes).unwrap();
            assert!(Host::open(path.clone()).is_err());
        }
    }

    struct BlockingModel {
        started: Arc<AtomicBool>,
        release: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Model for BlockingModel {
        fn information(&self) -> Information {
            FixtureModel.information()
        }
        fn present(&self) -> bool {
            true
        }
        fn loaded(&self) -> bool {
            true
        }
        async fn acquire(
            &self,
            _: Option<PathBuf>,
            cancelled: Arc<AtomicBool>,
            _: Arc<AtomicU64>,
        ) -> Result<(), String> {
            let started = self.started.clone();
            let release = self.release.clone();
            tokio::task::spawn_blocking(move || {
                started.store(true, Ordering::Release);
                while !release.load(Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(1));
                }
                fiote::communication::models::check_cancel(&cancelled)
            })
            .await
            .map_err(|error| error.to_string())?
        }
        fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
            self.started.store(true, Ordering::Release);
            while !self.release.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(1));
            }
            FixtureModel.decide(input, cancelled)
        }
    }

    #[tokio::test]
    async fn decision_selection_waits_for_cancelled_native_cpu_work_to_return() {
        let root = tempfile::tempdir().unwrap();
        let mut host = Host::open(root.path().join("speech.json")).unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        host.state.get_mut().decision =
            Arc::new(decision::Engine::from_model(Arc::new(BlockingModel {
                started: started.clone(),
                release: release.clone(),
            })));
        let input = Input {
            request_id: "waiting-switch".into(),
            generation: 1,
            text: "Please fix my tests".into(),
            context: String::new(),
            admission: false,
            routing: true,
            candidates: vec![decision::Candidate {
                record: nucleus::new_uid("r"),
                title: "Coding".into(),
                purpose: "Edits code".into(),
            }],
        };
        let job = nucleus::new_uid("speech");
        host.handle(Request::Decide {
            job: job.clone(),
            input,
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .is_err()
        );
        host.handle(Request::Cancel { job }).await.unwrap();
        assert!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .is_err()
        );
        release.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(2), async {
            while host.state.lock().await.decision.busy() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .unwrap()
            .decision_selection,
            decision::ModelId::Qwen
        );
    }

    #[tokio::test]
    async fn canceled_setup_refuses_model_switch_until_its_file_worker_returns() {
        struct Release(Arc<AtomicBool>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let root = tempfile::tempdir().unwrap();
        let mut host = Host::open(root.path().join("speech.json")).unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let guard = Release(release.clone());
        host.state.get_mut().decision =
            Arc::new(decision::Engine::from_model(Arc::new(BlockingModel {
                started: started.clone(),
                release,
            })));
        let job = nucleus::new_uid("speech");
        host.handle(Request::InstallDecision {
            job: job.clone(),
            source: None,
        })
        .await
        .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .is_err()
        );
        host.handle(Request::Cancel { job: job.clone() })
            .await
            .unwrap();
        tokio::task::yield_now().await;
        assert!(host.handle(Request::Poll { job }).await.is_err());
        assert!(
            host.handle(Request::Inspect)
                .await
                .unwrap()
                .decision_model
                .unwrap()
                .busy
        );
        assert!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .unwrap_err()
            .contains("finishing")
        );
        drop(guard);
        tokio::time::timeout(Duration::from_secs(2), async {
            while host.state.lock().await.decision.busy() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            host.handle(Request::SelectDecision {
                model: decision::ModelId::Qwen
            })
            .await
            .unwrap()
            .decision_selection,
            decision::ModelId::Qwen
        );
    }

    #[tokio::test]
    async fn shutdown_cancels_owned_work_refuses_new_jobs_and_waits_for_native_cpu_return() {
        struct Release(Arc<AtomicBool>);
        impl Drop for Release {
            fn drop(&mut self) { self.0.store(true, Ordering::Release); }
        }
        let root = tempfile::tempdir().unwrap();
        let mut host = Host::open(root.path().join("speech.json")).unwrap();
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let guard = Release(release.clone());
        host.state.get_mut().decision = Arc::new(decision::Engine::from_model(Arc::new(BlockingModel { started: started.clone(), release })));
        let host = Arc::new(host);
        let job = nucleus::new_uid("speech");
        host.handle(Request::InstallDecision { job: job.clone(), source: None }).await.unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            while !started.load(Ordering::Acquire) { tokio::task::yield_now().await; }
        }).await.unwrap();
        let shutdown = { let host = host.clone(); tokio::spawn(async move { host.shutdown().await }) };
        tokio::time::timeout(Duration::from_secs(2), async {
            while !host.closing.load(Ordering::Acquire) { tokio::task::yield_now().await; }
        }).await.unwrap();
        assert!(!shutdown.is_finished());
        assert!(host.handle(Request::InstallDecision { job: nucleus::new_uid("speech"), source: None }).await.unwrap_err().contains("shutting down"));
        assert!(host.handle(Request::SelectDecision { model: decision::ModelId::Qwen }).await.unwrap_err().contains("shutting down"));
        assert!(!host.handle(Request::Inspect).await.unwrap().ready);
        drop(guard);
        tokio::time::timeout(Duration::from_secs(2), shutdown).await.unwrap().unwrap();
        assert!(host.handle(Request::Poll { job }).await.is_err());
        assert!(!host.handle(Request::Inspect).await.unwrap().decision_model.unwrap().loaded);
        host.shutdown().await;
    }

    #[async_trait::async_trait]
    impl Model for FixtureModel {
        fn information(&self) -> Information {
            Information {
                model: "fixture".into(),
                revision: "test".into(),
                required_bytes: 0,
                routing_supported: true,
                routing_validated: true,
                admission_validated: false,
            }
        }
        fn present(&self) -> bool {
            true
        }
        fn loaded(&self) -> bool {
            true
        }
        async fn acquire(
            &self,
            _: Option<PathBuf>,
            _: Arc<AtomicBool>,
            _: Arc<AtomicU64>,
        ) -> Result<(), String> {
            Ok(())
        }
        fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
            fiote::communication::models::check_cancel(cancelled)?;
            Ok(Output {
                request_id: input.request_id.clone(),
                generation: input.generation,
                model: "fixture".into(),
                revision: "test".into(),
                admission: None,
                destination: Some(Choice {
                    value: Some(input.candidates[0].record.clone()),
                    probability: Some(1.0),
                    margin: Some(1.0),
                    reason: "fixture".into(),
                }),
                cold_load_ms: 0,
                inference_ms: 0,
            })
        }
    }

    #[tokio::test]
    async fn decision_jobs_return_structured_results_without_transcription_or_a_fiote() {
        let root = tempfile::tempdir().unwrap();
        let mut host = Host::open(root.path().join("speech.json")).unwrap();
        host.state.get_mut().decision =
            Arc::new(decision::Engine::from_model(Arc::new(FixtureModel)));
        let record = nucleus::new_uid("r");
        let job = nucleus::new_uid("speech");
        let input = Input {
            request_id: "owned-request".into(),
            generation: 7,
            text: "Please fix my tests".into(),
            context: String::new(),
            admission: false,
            routing: true,
            candidates: vec![decision::Candidate {
                record: record.clone(),
                title: "Coding".into(),
                purpose: "Fixes project tests".into(),
            }],
        };
        let request = Request::Decide {
            job: job.clone(),
            input,
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
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let result = status.job.unwrap();
        assert_eq!(result.purpose, Purpose::Decision);
        assert!(result.text.is_none());
        assert!(result.error.is_none());
        let decision = result.decision.unwrap();
        assert_eq!(decision.request_id, "owned-request");
        assert_eq!(decision.generation, 7);
        assert_eq!(
            decision.destination.unwrap().value.as_deref(),
            Some(record.as_str())
        );
        assert!(status.endpoint.is_none());
        assert!(!status.decision_capabilities.unwrap().admission_validated);
        host.handle(Request::Cancel { job: job.clone() })
            .await
            .unwrap();
        assert!(host.handle(request).await.is_err());
        assert!(host.handle(Request::Poll { job }).await.is_err());
    }
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
        assert!(status.detail.contains("local English model"));
    }

    #[tokio::test]
    async fn local_setup_failure_preserves_mode_and_never_uses_an_endpoint() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        let host = Host::open(path.clone()).unwrap();
        assert_eq!(
            host.handle(Request::Inspect).await.unwrap().mode,
            Mode::Local
        );
        let job = nucleus::new_uid("speech");
        host.handle(Request::InstallLocal {
            job: job.clone(),
            source: Some(root.path().join("missing-model")),
        })
        .await
        .unwrap();
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
        assert_eq!(status.job.as_ref().unwrap().purpose, Purpose::ModelSetup);
        assert!(status.job.as_ref().unwrap().error.is_some());
        assert!(!status.ready);
        assert!(status.endpoint.is_none());
        assert!(!status.local.as_ref().unwrap().loaded);
        host.handle(Request::Cancel { job }).await.unwrap();
        assert!(host.handle(Request::StartLocal {
            job: nucleus::new_uid("speech"),
            audio: fiote::config::Secret("UklGRjQAAABXQVZFZm10IBAAAAABAAEAgD4AAAB9AAACABAAZGF0YRAAAAABAQEBAQEBAQEBAQEBAQEBAQEB".into()),
        }).await.unwrap_err().contains("Download or import"));
        assert_eq!(
            Host::open(path)
                .unwrap()
                .handle(Request::Inspect)
                .await
                .unwrap()
                .mode,
            Mode::Local
        );
    }

    #[tokio::test]
    #[ignore = "Requires LINCE_SPEECH_TEST_CACHE and LINCE_SPEECH_TEST_WAV with installed Phonon weights and the public JFK fixture"]
    async fn real_local_model_import_transcription_cancellation_and_reopening_use_the_normal_service()
     {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("speech.json");
        let host = Host::open(path.clone()).unwrap();
        let source = speech::local::Recognizer::new(
            std::env::var_os("LINCE_SPEECH_TEST_CACHE")
                .expect("Choose an installed model cache")
                .into(),
        )
        .unwrap()
        .cache
        .path();
        let wav = std::fs::read(
            std::env::var_os("LINCE_SPEECH_TEST_WAV").expect("Choose the JFK fixture"),
        )
        .unwrap();
        let audio = fiote::config::Secret(STANDARD.encode(wav));
        let installation = nucleus::new_uid("speech");
        host.handle(Request::InstallLocal {
            job: installation.clone(),
            source: Some(source),
        })
        .await
        .unwrap();
        async fn finished(host: &Host, job: &str) -> Status {
            tokio::time::timeout(Duration::from_secs(180), async {
                loop {
                    let status = host
                        .handle(Request::Poll { job: job.into() })
                        .await
                        .unwrap();
                    if !status.job.as_ref().unwrap().pending {
                        return status;
                    }
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            })
            .await
            .unwrap()
        }
        let installed = finished(&host, &installation).await;
        assert!(installed.ready, "{installed:?}");
        assert!(installed.job.unwrap().error.is_none());
        assert!(
            host.handle(Request::Inspect)
                .await
                .unwrap()
                .endpoint
                .is_none()
        );
        let job = nucleus::new_uid("speech");
        let request = Request::StartLocal {
            job: job.clone(),
            audio: audio.clone(),
        };
        host.handle(request.clone()).await.unwrap();
        host.handle(request).await.unwrap();
        let result = finished(&host, &job).await;
        let result = result.job.unwrap();
        assert!(
            result
                .text
                .unwrap()
                .to_ascii_lowercase()
                .contains("ask not what your country can do for you")
        );
        assert!(host.local.loaded());
        let cancelled = nucleus::new_uid("speech");
        host.handle(Request::StartLocal {
            job: cancelled.clone(),
            audio,
        })
        .await
        .unwrap();
        host.handle(Request::Cancel {
            job: cancelled.clone(),
        })
        .await
        .unwrap();
        assert!(host.handle(Request::Poll { job: cancelled }).await.is_err());
        tokio::time::timeout(Duration::from_secs(60), async {
            while host.local.busy() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let reopened = Host::open(path.clone()).unwrap();
        let status = reopened.handle(Request::Inspect).await.unwrap();
        assert!(status.ready);
        assert!(status.job.is_none());
        assert_eq!(status.mode, Mode::Local);
        assert!(!status.local.unwrap().loaded);
        assert!(!path.with_extension("endpoint.json").exists());
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
