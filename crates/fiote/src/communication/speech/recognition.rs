use super::{local, moonshine};
use crate::{
    communication::models::{self, Artifact, Cache, Manifest},
    config::Secret,
};
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelId {
    #[default]
    Phonon,
    MoonshineTiny,
    MoonshineSmall,
}

impl ModelId {
    pub const ALL: [Self; 3] = [Self::Phonon, Self::MoonshineTiny, Self::MoonshineSmall];

    pub fn label(self) -> &'static str {
        match self {
            Self::Phonon => "Phonon-2",
            Self::MoonshineTiny => "Moonshine Tiny",
            Self::MoonshineSmall => "Moonshine Small",
        }
    }

    pub fn manifests(self) -> Vec<Manifest> {
        if self == Self::Phonon {
            return vec![local::MANIFEST];
        }
        let (id, revision, config_bytes, config_hash, weight_bytes, weight_hash) = match self {
            Self::MoonshineTiny => (
                "moonshine-streaming-tiny",
                "f8e9dfd8c562c257c151a907b7b7f2fe8ff8511a",
                1421,
                "166861753adaf928ae99b3709bfb1da47cb76319be58f37c6f9b7a539ce371f0",
                176_237_308,
                "0ab3443dabd95a4afb35f0a26155428c03b7c3c450fa6923ffdac4510ba3d040",
            ),
            Self::MoonshineSmall => (
                "moonshine-streaming-small",
                "2c036506f23a09c18df5a50057599ba6d9280999",
                1579,
                "9196efd60b559dc4dc4e9fab08a7b0fce3501d0373b995158c6c6d3ab9b58461",
                560_571_228,
                "003635df57b9a47771fd94a6290ab517d7487d0026c165ca9f2ac90411802646",
            ),
            Self::Phonon => unreachable!(),
        };
        let repository = match self {
            Self::MoonshineTiny => "moonshine-ai/moonshine-streaming-tiny",
            _ => "moonshine-ai/moonshine-streaming-small",
        };
        [
            Artifact {
                name: "model.safetensors",
                bytes: weight_bytes,
                sha256: weight_hash,
            },
            Artifact {
                name: "config.json",
                bytes: config_bytes,
                sha256: config_hash,
            },
            Artifact {
                name: "tokenizer.json",
                bytes: 1_676_069,
                sha256: "28bbc54a47dfa33673affa26010f23ac452685b18c76e81451d9b3ea25a54d54",
            },
        ]
        .into_iter()
        .map(|artifact| Manifest {
            id,
            repository,
            revision,
            license: "MIT",
            artifact,
        })
        .collect()
    }
}

#[async_trait::async_trait]
pub trait Model: Send + Sync {
    fn id(&self) -> ModelId;
    fn primary_cache(&self) -> Cache;
    fn present(&self) -> bool;
    fn loaded(&self) -> bool;
    fn unload(&self);
    async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String>;
    fn transcribe(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<String, String>;
}

#[async_trait::async_trait]
impl Model for local::Recognizer {
    fn id(&self) -> ModelId {
        ModelId::Phonon
    }
    fn primary_cache(&self) -> Cache {
        self.cache.clone()
    }
    fn present(&self) -> bool {
        self.cache.present()
    }
    fn loaded(&self) -> bool {
        self.loaded()
    }
    fn unload(&self) {
        self.unload();
    }
    async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        self.cache.acquire(source, cancelled, progress).await
    }
    fn transcribe(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<String, String> {
        self.transcribe(samples, cancelled)
            .map(|result| result.text)
    }
}

struct Moonshine {
    id: ModelId,
    caches: Vec<Cache>,
    engine: Mutex<Option<moonshine::Recognizer>>,
    loaded: AtomicBool,
}

#[async_trait::async_trait]
impl Model for Moonshine {
    fn id(&self) -> ModelId {
        self.id
    }
    fn primary_cache(&self) -> Cache {
        self.caches[0].clone()
    }
    fn present(&self) -> bool {
        self.caches.iter().all(Cache::present)
    }
    fn loaded(&self) -> bool {
        self.loaded.load(Ordering::Acquire)
    }
    fn unload(&self) {
        *self
            .engine
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = None;
        self.engine.clear_poison();
        self.loaded.store(false, Ordering::Release);
    }
    async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        if source.as_ref().is_some_and(|path| !path.is_dir()) {
            return Err("Import Moonshine from a directory containing model.safetensors, config.json and tokenizer.json.".into());
        }
        let mut completed = 0;
        for cache in &self.caches {
            let current = Arc::new(AtomicU64::new(0));
            let acquisition = cache.acquire(
                source
                    .as_ref()
                    .map(|path| path.join(cache.manifest.artifact.name)),
                cancelled.clone(),
                current.clone(),
            );
            tokio::pin!(acquisition);
            loop {
                tokio::select! {
                    result = &mut acquisition => { result?; break; }
                    _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                        progress.store(completed + current.load(Ordering::Acquire), Ordering::Release);
                    }
                }
            }
            completed += cache.manifest.artifact.bytes;
            progress.store(completed, Ordering::Release);
        }
        Ok(())
    }
    fn transcribe(&self, samples: &[f32], cancelled: &AtomicBool) -> Result<String, String> {
        let mut engine = self
            .engine
            .lock()
            .map_err(|_| "Reload the speech model after an inference failure.")?;
        if engine.is_none() {
            for cache in &self.caches {
                models::check_cancel(cancelled)?;
                cache.verified_file()?;
            }
            *engine = Some(moonshine::Recognizer::load(
                &self.primary_cache().directory(),
            )?);
            self.loaded.store(true, Ordering::Release);
        }
        let engine = engine.as_ref().ok_or("The speech model did not load.")?;
        let mut text = Vec::new();
        for samples in samples.chunks(320_000) {
            models::check_cancel(cancelled)?;
            if !contains_signal(samples) {
                continue;
            }
            text.push(engine.transcribe(samples, cancelled)?);
        }
        Ok(text.join(" "))
    }
}

pub struct Recognizer {
    root: PathBuf,
    model: RwLock<Arc<dyn Model>>,
    permit: Arc<tokio::sync::Semaphore>,
    pool: Arc<rayon::ThreadPool>,
}

impl Recognizer {
    pub fn selected(root: PathBuf, id: ModelId) -> Result<Self, String> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(
                std::thread::available_parallelism()
                    .map_or(1, usize::from)
                    .min(4),
            )
            .thread_name(|index| format!("fiote-recognition-{index}"))
            .build()
            .map_err(|error| error.to_string())?;
        Ok(Self {
            model: RwLock::new(Self::driver(&root, id)?),
            root,
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
            pool: Arc::new(pool),
        })
    }
    fn driver(root: &std::path::Path, id: ModelId) -> Result<Arc<dyn Model>, String> {
        if id == ModelId::Phonon {
            return Ok(Arc::new(local::Recognizer::new(root.to_path_buf())?));
        }
        Ok(Arc::new(Moonshine {
            id,
            caches: id
                .manifests()
                .into_iter()
                .map(|manifest| Cache::new(root.to_path_buf(), manifest))
                .collect::<Result<_, _>>()?,
            engine: Mutex::new(None),
            loaded: AtomicBool::new(false),
        }))
    }
    fn model(&self) -> Arc<dyn Model> {
        self.model
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
    pub fn selection(&self) -> ModelId {
        self.model().id()
    }
    pub fn primary_cache(&self) -> Cache {
        self.model().primary_cache()
    }
    pub fn present(&self) -> bool {
        self.model().present()
    }
    pub fn loaded(&self) -> bool {
        self.model().loaded()
    }
    pub fn busy(&self) -> bool {
        self.permit.available_permits() == 0
    }
    pub fn required_bytes(&self) -> u64 {
        self.selection()
            .manifests()
            .iter()
            .map(|manifest| manifest.artifact.bytes)
            .sum()
    }
    pub fn select(&self, id: ModelId) -> Result<(), String> {
        let _permit = self
            .permit
            .try_acquire()
            .map_err(|_| "Local speech is still finishing an operation.")?;
        if self.selection() != id {
            let driver = Self::driver(&self.root, id)?;
            *self
                .model
                .write()
                .map_err(|_| "Speech model selection is unavailable.")? = driver;
        }
        Ok(())
    }
    pub fn unload(&self) {
        self.model().unload();
    }
    pub async fn acquire(
        self: &Arc<Self>,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Local speech is still working.")?;
        let model = self.model();
        tokio::spawn(async move {
            let _permit = permit;
            model.acquire(source, cancelled, progress).await
        })
        .await
        .map_err(|error| error.to_string())?
    }
    pub fn submit(
        self: &Arc<Self>,
        audio: Secret,
        cancelled: Arc<AtomicBool>,
    ) -> Result<tokio::task::JoinHandle<Result<String, String>>, String> {
        let samples = super::decode_audio(&audio.0)?;
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Local speech is still finishing an operation.")?;
        let model = self.model();
        let pool = self.pool.clone();
        Ok(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            pool.install(|| Self::recognize(model.as_ref(), &samples, &cancelled))
        }))
    }
    fn recognize(
        model: &dyn Model,
        samples: &[f32],
        cancelled: &AtomicBool,
    ) -> Result<String, String> {
        models::check_cancel(cancelled)?;
        if samples.is_empty()
            || samples.len() > 1_920_000
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return Err(
                "Local speech requires finite mono 16 kHz samples, up to two minutes.".into(),
            );
        }
        if !contains_signal(samples) {
            return Err("No speech was recognized. Your draft is unchanged.".into());
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            model.transcribe(samples, cancelled)
        }));
        let text = match result {
            Ok(result) => result?,
            Err(_) => {
                model.unload();
                return Err(
                    "Local speech failed. Retry to reload the model; your draft is unchanged."
                        .into(),
                );
            }
        };
        models::check_cancel(cancelled)?;
        super::endpoint::transcript(&serde_json::json!({"text":text}))
    }
}

fn contains_signal(samples: &[f32]) -> bool {
    samples.iter().any(|sample| sample.abs() >= 0.001)
}

#[cfg(test)]
mod tests;
