use crate::{
    communication::models::{self, Artifact, Cache, Manifest},
    config::Secret,
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub const REVISION: &str = "5f9280c761a5c147bb682321f3f019482fab3e7b";
pub const ARTIFACTS: [Artifact; 3] = [
    Artifact {
        name: "kokoro.btm",
        bytes: 84_641_620,
        sha256: "b770fd2d35f56b9b95e95d99d9954746cdc4a435223a4f41842fa12160a2419f",
    },
    Artifact {
        name: "voices.btv",
        bytes: 26_113_620,
        sha256: "b3d63861c4412e94a5b589abeaa1f486625fd91249d582d546b50e282e20f065",
    },
    Artifact {
        name: "lexicon.btl",
        bytes: 4_842_496,
        sha256: "5afd10491265c6002bfb4a2d914d5b666961a0374fb5baa116fb37c68b4e0a0d",
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Inspect,
    Install {
        job: String,
        source: Option<PathBuf>,
    },
    Speak {
        job: String,
        text: String,
    },
    Poll {
        job: String,
    },
    Cancel {
        job: String,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub present: bool,
    pub busy: bool,
    pub acquired_bytes: u64,
    pub required_bytes: u64,
    pub job: Option<Job>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub pending: bool,
    pub audio: Option<Secret>,
    pub error: Option<String>,
}

struct Running {
    result: Job,
    cancelled: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<Result<Option<Secret>, String>>>,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub struct Service {
    caches: [Cache; 3],
    engine: Arc<Mutex<Option<best_tts::Engine>>>,
    state: tokio::sync::Mutex<Option<Running>>,
    progress: Arc<AtomicU64>,
    closing: AtomicBool,
    active: Arc<AtomicBool>,
    used: tokio::sync::Mutex<std::collections::VecDeque<String>>,
}

impl Service {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let caches = ARTIFACTS
            .map(|artifact| {
                Cache::new(
                    root.clone(),
                    Manifest {
                        id: "kokoro-english",
                        repository: "studio-ransom/best-tts-rust",
                        revision: REVISION,
                        license: "Apache-2.0; MIT lexicon",
                        artifact,
                    },
                )
            })
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| "Invalid voice manifest.")?;
        Ok(Self {
            caches,
            engine: Arc::new(Mutex::new(None)),
            state: Default::default(),
            progress: Arc::new(AtomicU64::new(0)),
            closing: AtomicBool::new(false),
            active: Arc::new(AtomicBool::new(false)),
            used: Default::default(),
        })
    }

    pub async fn handle(&self, request: Request) -> Result<Status, String> {
        let mut state = self.state.lock().await;
        if let Some(running) = state.as_mut()
            && running.task.as_ref().is_some_and(|task| task.is_finished())
        {
            let result = running
                .task
                .take()
                .unwrap()
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            running.result.pending = false;
            if running.cancelled.load(Ordering::Acquire) {
                running.result.audio = None;
                running.result.error = Some("Voice operation cancelled.".into());
            } else {
                match result {
                    Ok(audio) => running.result.audio = audio,
                    Err(error) => running.result.error = Some(error),
                }
            }
        }
        match request {
            Request::Inspect => {}
            Request::Poll { job } => {
                if state
                    .as_ref()
                    .is_none_or(|running| running.result.id != job)
                {
                    return Err("This spoken-reply operation is no longer current.".into());
                }
            }
            Request::Cancel { job } => {
                if !nucleus::valid_uid(&job, "speech") {
                    return Err("Invalid voice cancellation identity.".into());
                }
                let mut used = self.used.lock().await;
                if !used.contains(&job) {
                    used.push_back(job.clone());
                }
                if used.len() > 128 {
                    used.pop_front();
                }
                if let Some(running) = state.as_mut().filter(|running| running.result.id == job) {
                    running.cancelled.store(true, Ordering::Release);
                    running.result.audio = None;
                    running.result.error = Some(
                        "Voice operation cancelled; its current synthesis chunk may still finish."
                            .into(),
                    );
                }
            }
            request => {
                if self.closing.load(Ordering::Acquire) {
                    return Err("Spoken replies are shutting down.".into());
                }
                if state.as_ref().is_some_and(|running| running.result.pending)
                    || self.active.load(Ordering::Acquire)
                {
                    return Err(
                        "A voice operation is still finishing. Wait or Stop speaking.".into(),
                    );
                }
                let (job, source, text) = match request {
                    Request::Install { job, source } => (job, source, None),
                    Request::Speak { job, text } => (job, None, Some(text)),
                    _ => unreachable!(),
                };
                if !nucleus::valid_uid(&job, "speech") {
                    return Err("Use a valid speech operation identity.".into());
                }
                let mut used = self.used.lock().await;
                if used.contains(&job) {
                    return Err("This voice operation was already started or cancelled. Start a new operation.".into());
                }
                if let Some(text) = &text {
                    if text.trim().is_empty()
                        || text.chars().count() > 300
                        || text.chars().any(|c| c.is_control() && !c.is_whitespace())
                    {
                        return Err(
                            "Speak one nonempty text chunk of at most 300 characters.".into()
                        );
                    }
                    if !self.caches.iter().all(Cache::present) {
                        return Err("Download or import the supported English voice first.".into());
                    }
                }
                let cancelled = Arc::new(AtomicBool::new(false));
                used.push_back(job.clone());
                if used.len() > 128 {
                    used.pop_front();
                }
                let control = cancelled.clone();
                let caches = self.caches.clone();
                let engine = self.engine.clone();
                let progress = self.progress.clone();
                let active = self.active.clone();
                active.store(true, Ordering::Release);
                let task = tokio::spawn(async move {
                    struct Working(Arc<AtomicBool>);
                    impl Drop for Working {
                        fn drop(&mut self) {
                            self.0.store(false, Ordering::Release);
                        }
                    }
                    let _working = Working(active);
                    if let Some(text) = text {
                        tokio::task::spawn_blocking(move || {
                            synthesize(&caches, &engine, &control, &text)
                        })
                        .await
                        .map_err(|error| error.to_string())?
                        .map(Some)
                    } else {
                        progress.store(0, Ordering::Release);
                        let mut completed = 0;
                        for cache in caches {
                            models::check_cancel(&control)?;
                            let acquired = Arc::new(AtomicU64::new(0));
                            if let Some(source) = &source {
                                cache
                                    .acquire(
                                        Some(source.join(cache.manifest.artifact.name)),
                                        control.clone(),
                                        acquired.clone(),
                                    )
                                    .await?;
                            } else {
                                let url = format!(
                                    "https://raw.githubusercontent.com/studio-ransom/best-tts-rust/{REVISION}/assets/{}",
                                    cache.manifest.artifact.name
                                );
                                cache.download(&url, control.clone(), acquired).await?;
                            }
                            completed += cache.manifest.artifact.bytes;
                            progress.store(completed, Ordering::Release);
                        }
                        *engine
                            .lock()
                            .map_err(|_| "Voice model lock is unavailable.")? = None;
                        Ok(None)
                    }
                });
                *state = Some(Running {
                    result: Job {
                        id: job,
                        pending: true,
                        audio: None,
                        error: None,
                    },
                    cancelled,
                    task: Some(task),
                });
            }
        }
        Ok(Status {
            present: self.caches.iter().all(Cache::present),
            busy: state.as_ref().is_some_and(|running| running.result.pending)
                || self.active.load(Ordering::Acquire),
            acquired_bytes: self.progress.load(Ordering::Acquire),
            required_bytes: ARTIFACTS.iter().map(|artifact| artifact.bytes).sum(),
            job: state.as_ref().map(|running| running.result.clone()),
        })
    }

    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        if let Some(running) = self.state.lock().await.as_ref() {
            running.cancelled.store(true, Ordering::Release);
        }
        while self.active.load(Ordering::Acquire) {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        *self.state.lock().await = None;
        *self
            .engine
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
    }
}

fn synthesize(
    caches: &[Cache; 3],
    engine: &Mutex<Option<best_tts::Engine>>,
    cancelled: &AtomicBool,
    text: &str,
) -> Result<Secret, String> {
    models::check_cancel(cancelled)?;
    let mut engine = engine
        .lock()
        .map_err(|_| "Voice model lock is unavailable.")?;
    if engine.is_none() {
        for cache in caches {
            cache.verified_file()?;
        }
        *engine = Some(
            best_tts::Engine::from_dir(caches[0].directory())
                .map_err(|error| error.to_string())?
                .with_threads(
                    std::thread::available_parallelism()
                        .map_or(1, usize::from)
                        .min(4),
                ),
        );
    }
    models::check_cancel(cancelled)?;
    let audio = engine
        .as_ref()
        .unwrap()
        .say(text, "af_heart")
        .map_err(|error| error.to_string())?;
    models::check_cancel(cancelled)?;
    if audio.sample_rate != 24_000
        || audio.samples.is_empty()
        || audio.samples.len() > 24_000 * 45
        || audio.samples.iter().any(|sample| !sample.is_finite())
    {
        return Err("The English voice returned invalid or excessive audio.".into());
    }
    Ok(Secret(BASE64.encode(audio.to_wav())))
}

#[cfg(test)]
mod tests;
