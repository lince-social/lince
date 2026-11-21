// non-lean-yet
// Scope: mod context, mod dialogue, mod laya, mod nli.
// BeautifulRust (baseline lines):
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 187, 207.
// - [Clippy: clippy::clone_on_ref_ptr] Make shared ownership explicit at retained clones. Sites
//   185, 190, 205, 210, 253, 254, 281, 292, 331, 332, 335, 336.
// - [Clippy: clippy::exhaustive_enums] Review whether public API evolution requires non_exhaustive.
//   Sites 19, 78.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   189, 209.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 45, 137, 179, 199.
// - [rustc: missing_debug_implementations] type does not implement `std::fmt::Debug`; consider
//   adding `#[derive(Debug)]` or a manual implementation. Sites 140.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 2, 3,
//   4, 5, 19, 21, 22, 23, 27, 28, 29, 30, 34, 35, 36, 37, 38, 39, 40, 41, 45, 78, 79, 80, 81, 85,
//   86, 87, 88, 89, 93, 104, 105, 106, 107, 108, 109, 110, 111, 112, 116, 117, 118, 119, 121, 122,
//   123, 127, 128, 129, 130, 131, 137, 140, 146, 150, 159, 166, 169, 172, 175, 179, 199.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites acquire: String
//   (179), submit: String (199), validate: String (45).

mod context;
pub mod dialogue;
pub mod laya;
pub mod nli;
pub mod qwen;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64},
    },
};

pub use context::{Context, Dialogue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModelId {
    #[default]
    Laya,
    EnglishNli,
    Qwen,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub record: String,
    pub title: String,
    pub purpose: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub request_id: String,
    pub generation: u64,
    pub text: String,
    pub context: String,
    pub candidates: Vec<Candidate>,
    pub admission: bool,
    pub routing: bool,
}

impl Input {
    pub fn validate(&self) -> Result<(), String> {
        Context::decode(&self.context)?;
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.text.trim().is_empty()
            || self.text.len() > 65_536
            || self.context.len() > 4096
            || self.candidates.len() > 30
            || (!self.admission && !self.routing)
        {
            return Err("Decision input exceeds its supported bounds or has no question.".into());
        }
        let mut records = std::collections::BTreeSet::new();
        for candidate in &self.candidates {
            if !nucleus::valid_uid(&candidate.record, "r")
                || !records.insert(&candidate.record)
                || candidate.title.len() > 256
                || candidate.purpose.len() > 1600
            {
                return Err(
                    "Decision candidates require unique Fiote UIDs and bounded descriptions."
                        .into(),
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Admission {
    AssistantRequest,
    OrdinaryConversation,
    Unclear,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice<T> {
    pub value: Option<T>,
    pub probability: Option<f64>,
    pub margin: Option<f64>,
    pub reason: String,
}

impl<T> Choice<T> {
    pub fn abstain(reason: impl Into<String>) -> Self {
        Self {
            value: None,
            probability: None,
            margin: None,
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub request_id: String,
    pub generation: u64,
    pub model: String,
    pub revision: String,
    pub admission: Option<Choice<Admission>>,
    pub destination: Option<Choice<String>>,
    pub cold_load_ms: u128,
    pub inference_ms: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Information {
    pub model: String,
    pub revision: String,
    pub required_bytes: u64,
    #[serde(default)]
    pub routing_supported: bool,
    pub routing_validated: bool,
    pub admission_validated: bool,
}

#[async_trait::async_trait]
pub trait Model: Send + Sync {
    fn information(&self) -> Information;
    fn present(&self) -> bool;
    fn loaded(&self) -> bool;
    async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String>;
    fn decide(&self, input: &Input, cancelled: &AtomicBool) -> Result<Output, String>;
}

pub struct Engine {
    model: Arc<dyn Model>,
    permit: Arc<tokio::sync::Semaphore>,
}

impl Engine {
    pub fn local(root: PathBuf) -> Self {
        Self::selected(root, ModelId::default())
    }

    pub fn selected(root: PathBuf, model: ModelId) -> Self {
        let driver: Arc<dyn Model> = match model {
            ModelId::Laya => Arc::new(laya::Driver::new(root, laya::Thresholds::default())),
            ModelId::EnglishNli => Arc::new(nli::Driver::new(root)),
            ModelId::Qwen => Arc::new(qwen::Driver::new(root)),
        };
        Self::from_model(driver)
    }

    pub fn from_model(model: Arc<dyn Model>) -> Self {
        Self {
            model,
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub fn information(&self) -> Information {
        self.model.information()
    }
    pub fn present(&self) -> bool {
        self.model.present()
    }
    pub fn loaded(&self) -> bool {
        self.model.loaded()
    }
    pub fn busy(&self) -> bool {
        self.permit.available_permits() == 0
    }

    pub async fn acquire(
        &self,
        source: Option<PathBuf>,
        cancelled: Arc<AtomicBool>,
        progress: Arc<AtomicU64>,
    ) -> Result<(), String> {
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Decision model is still working.")?;
        let model = self.model.clone();
        tokio::spawn(async move {
            let _permit = permit;
            model.acquire(source, cancelled, progress).await
        })
        .await
        .map_err(|error| error.to_string())?
    }

    pub fn submit(
        &self,
        input: Input,
        cancelled: Arc<AtomicBool>,
    ) -> Result<tokio::task::JoinHandle<Result<Output, String>>, String> {
        input.validate()?;
        let permit = self
            .permit
            .clone()
            .try_acquire_owned()
            .map_err(|_| "Decision model is still finishing a previous operation.")?;
        let model = self.model.clone();
        Ok(tokio::task::spawn_blocking(move || {
            let _permit = permit;
            crate::communication::models::check_cancel(&cancelled)?;
            model.decide(&input, &cancelled)
        }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    struct BlockedModel {
        started: Arc<AtomicBool>,
        released: Arc<AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Model for BlockedModel {
        fn information(&self) -> Information {
            Information {
                model: "fixture".into(),
                revision: "test".into(),
                required_bytes: 0,
                routing_supported: false,
                routing_validated: false,
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
            cancelled: Arc<AtomicBool>,
            _: Arc<AtomicU64>,
        ) -> Result<(), String> {
            let started = self.started.clone();
            let released = self.released.clone();
            tokio::task::spawn_blocking(move || {
                started.store(true, Ordering::Release);
                while !released.load(Ordering::Acquire) {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                crate::communication::models::check_cancel(&cancelled)
            })
            .await
            .map_err(|error| error.to_string())?
        }
        fn decide(&self, _: &Input, cancelled: &AtomicBool) -> Result<Output, String> {
            self.started.store(true, Ordering::Release);
            while !self.released.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
            crate::communication::models::check_cancel(cancelled)?;
            Err("Fixture has no result".into())
        }
    }

    #[tokio::test]
    async fn cancelled_cpu_work_keeps_its_permit_until_the_model_really_returns() {
        let model = Arc::new(BlockedModel {
            started: Arc::new(AtomicBool::new(false)),
            released: Arc::new(AtomicBool::new(false)),
        });
        let engine = Engine::from_model(model.clone());
        let input = Input {
            request_id: "cancelled".into(),
            generation: 1,
            text: "Hello".into(),
            context: String::new(),
            candidates: Vec::new(),
            admission: true,
            routing: false,
        };
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = engine.submit(input.clone(), cancelled.clone()).unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !model.started.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        cancelled.store(true, Ordering::Release);
        task.abort();
        assert!(engine.busy());
        assert!(
            engine
                .submit(input, Arc::new(AtomicBool::new(false)))
                .is_err()
        );
        model.released.store(true, Ordering::Release);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while engine.busy() {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        assert!(task.await.unwrap().unwrap_err().contains("cancelled"));
    }

    #[tokio::test]
    async fn cancelled_import_keeps_its_permit_until_the_file_worker_returns() {
        struct Release(Arc<AtomicBool>);
        impl Drop for Release {
            fn drop(&mut self) {
                self.0.store(true, Ordering::Release);
            }
        }
        let model = Arc::new(BlockedModel {
            started: Arc::new(AtomicBool::new(false)),
            released: Arc::new(AtomicBool::new(false)),
        });
        let release = Release(model.released.clone());
        let engine = Arc::new(Engine::from_model(model.clone()));
        let cancelled = Arc::new(AtomicBool::new(false));
        let task = {
            let engine = engine.clone();
            let cancelled = cancelled.clone();
            tokio::spawn(async move {
                engine
                    .acquire(None, cancelled, Arc::new(AtomicU64::new(0)))
                    .await
            })
        };
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while !model.started.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        cancelled.store(true, Ordering::Release);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(engine.busy());
        assert!(
            engine
                .acquire(
                    None,
                    Arc::new(AtomicBool::new(false)),
                    Arc::new(AtomicU64::new(0))
                )
                .await
                .is_err()
        );
        drop(release);
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while engine.busy() {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        engine
            .acquire(
                None,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            )
            .await
            .unwrap();
    }
}
