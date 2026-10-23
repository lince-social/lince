use super::*;

struct Stub {
    calls: AtomicU64,
    release: AtomicBool,
    fail: bool,
}

#[async_trait::async_trait]
impl Model for Stub {
    fn id(&self) -> ModelId {
        ModelId::MoonshineSmall
    }
    fn primary_cache(&self) -> Cache {
        Cache::new(std::env::temp_dir(), self.id().manifests()[0]).unwrap()
    }
    fn present(&self) -> bool {
        true
    }
    fn loaded(&self) -> bool {
        false
    }
    fn unload(&self) {
        self.release.store(true, Ordering::Release);
    }
    async fn acquire(
        &self,
        _: Option<PathBuf>,
        _: Arc<AtomicBool>,
        _: Arc<AtomicU64>,
    ) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        while !self.release.load(Ordering::Acquire) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        Ok(())
    }
    fn transcribe(&self, _: &[f32], _: &AtomicBool) -> Result<String, String> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        while !self.release.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!self.fail, "model failed");
        Ok("hello".into())
    }
}

#[test]
fn registry_provides_pinned_models_and_complete_artifacts() {
    let root = tempfile::tempdir().unwrap();
    for id in ModelId::ALL {
        let recognizer = Recognizer::selected(root.path().into(), id).unwrap();
        assert_eq!(recognizer.selection(), id);
        assert!(!recognizer.present());
        assert!(!recognizer.loaded());
        assert!(!recognizer.busy());
        assert!(recognizer.required_bytes() > 100_000_000);
        assert_eq!(
            serde_json::from_str::<ModelId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
        for manifest in id.manifests() {
            assert_eq!(manifest.revision.len(), 40);
            assert_eq!(manifest.artifact.sha256.len(), 64);
            assert!(Cache::new(root.path().into(), manifest).is_ok());
        }
    }
}

#[test]
fn shared_guard_rejects_silence_invalid_and_cancelled_audio_before_model_inference() {
    let model = Stub {
        calls: AtomicU64::new(0),
        release: AtomicBool::new(true),
        fail: false,
    };
    let cancelled = AtomicBool::new(false);
    for samples in [
        &[][..],
        &[f32::NAN],
        &[f32::INFINITY],
        &[0.0; 1600],
        &[0.0001; 1600],
    ] {
        assert!(Recognizer::recognize(&model, samples, &cancelled).is_err());
    }
    assert_eq!(model.calls.load(Ordering::Acquire), 0);
    assert_eq!(
        Recognizer::recognize(&model, &[0.5; 1600], &cancelled).unwrap(),
        "hello"
    );
    cancelled.store(true, Ordering::Release);
    assert!(Recognizer::recognize(&model, &[0.5; 1600], &cancelled).is_err());
    assert_eq!(model.calls.load(Ordering::Acquire), 1);
    let failing = Stub {
        calls: AtomicU64::new(0),
        release: AtomicBool::new(true),
        fail: true,
    };
    assert!(
        Recognizer::recognize(&failing, &[0.5; 1600], &AtomicBool::new(false))
            .unwrap_err()
            .contains("Retry to reload")
    );
}

#[tokio::test]
async fn aborted_acquisition_retains_exclusive_permit_until_actual_work_finishes() {
    let root = tempfile::tempdir().unwrap();
    let recognizer = Arc::new(Recognizer::selected(root.path().into(), ModelId::Phonon).unwrap());
    let model = Arc::new(Stub {
        calls: AtomicU64::new(0),
        release: AtomicBool::new(false),
        fail: false,
    });
    *recognizer.model.write().unwrap() = model.clone();
    let caller = recognizer.clone();
    let task = tokio::spawn(async move {
        caller
            .acquire(
                None,
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicU64::new(0)),
            )
            .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while model.calls.load(Ordering::Acquire) == 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    task.abort();
    let _ = task.await;
    assert!(recognizer.busy());
    assert!(recognizer.select(ModelId::Phonon).is_err());
    model.release.store(true, Ordering::Release);
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while recognizer.busy() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    recognizer.select(ModelId::Phonon).unwrap();
}
