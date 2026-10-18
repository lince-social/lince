use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

struct PendingWork {
    finished: Arc<AtomicUsize>,
    wire_open_at_finish: Arc<AtomicUsize>,
    wire: WireSlot,
}

impl Drop for PendingWork {
    fn drop(&mut self) {
        self.finished.fetch_add(1, Ordering::SeqCst);
        if self.wire.try_read().is_ok_and(|slot| slot.is_some()) {
            self.wire_open_at_finish.fetch_add(1, Ordering::SeqCst);
        }
    }
}

async fn pending(work: PendingWork) -> tokio::task::JoinHandle<()> {
    let (started, ready) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let _work = work;
        started.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    ready.await.unwrap();
    handle
}

#[tokio::test]
async fn shutdown_awaits_owned_work_before_network_close_and_closes_cloned_database_access() {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("shutdown.db");
        let engine = Arc::new(
            engine::Engine::open(&format!("sqlite://{}?mode=rwc", path.display()))
                .await
                .unwrap(),
        );
        store::cells::set_config(
            &engine.store.pool,
            "lince.network",
            &serde_json::json!({"peer_port":0}),
        )
        .await
        .unwrap();
        store::cells::set_config(
            &engine.store.pool,
            "shutdown-check",
            &serde_json::json!({"saved":true}),
        )
        .await
        .unwrap();
        let wire = Arc::new(
            engine::wire::Wire::bind_with_discovery(
                engine.clone(),
                engine::wire::node_secret(&directory.path().join("node.key")).unwrap(),
                engine::wire::Reach::Local,
                None,
                false,
            )
            .await
            .unwrap(),
        );
        let runtime = CellRuntime {
            engine: engine.clone(),
            store: engine.store.clone(),
            lanes: Arc::new(LaneHub::new()),
            wire: Arc::new(tokio::sync::RwLock::new(Some(wire))),
            speech: None,
            commands: Default::default(),
            information: None,
            fiote: None,
        };
        let finished = Arc::new(AtomicUsize::new(0));
        let open = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            handles.push(
                pending(PendingWork {
                    finished: finished.clone(),
                    wire_open_at_finish: open.clone(),
                    wire: runtime.wire.clone(),
                })
                .await,
            );
        }
        let worker = handles.pop().unwrap();
        let supervisor = handles.pop().unwrap();
        let worker_abort = worker.abort_handle();
        let supervisor_abort = supervisor.abort_handle();
        Cell {
            runtime: runtime.clone(),
            supervisors: vec![supervisor],
            tasks: vec![worker],
        }
        .shutdown()
        .await;
        assert_eq!(finished.load(Ordering::SeqCst), 2);
        assert_eq!(
            open.load(Ordering::SeqCst),
            2,
            "Owned work must stop before network services close"
        );
        assert!(worker_abort.is_finished());
        assert!(supervisor_abort.is_finished());
        assert!(runtime.wire.read().await.is_none());
        assert!(runtime.store.pool.is_closed());
        assert!(matches!(
            store::sqlx::query("SELECT 1")
                .execute(&runtime.store.pool)
                .await,
            Err(store::sqlx::Error::PoolClosed)
        ));
        let reopened = store::Store::open(&format!("sqlite://{}?mode=rw", path.display()))
            .await
            .unwrap();
        assert_eq!(
            store::cells::config(&reopened.pool, "shutdown-check")
                .await
                .unwrap()
                .unwrap()["saved"],
            true
        );
        reopened.pool.close().await;
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn shutdown_closes_pending_transcription_and_prevents_cloned_speech_from_reopening() {
    use base64::Engine as _;
    use speech::{Request, Service};
    use tokio::io::AsyncReadExt;
    let root = tempfile::tempdir().unwrap();
    let engine = Arc::new(engine::Engine::open_memory().await.unwrap());
    let speech = Arc::new(speech::Host::open(root.path().join("speech.json")).unwrap());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let settings = speech::endpoint::Settings {
        url: format!("http://{}/transcribe", listener.local_addr().unwrap()),
        model: "owned-shutdown-test".into(),
        requires_key: false,
        allow_cloud: false,
    };
    speech
        .handle(Request::ConfigureEndpoint {
            settings: settings.clone(),
            credential: None,
            password: None,
        })
        .await
        .unwrap();
    speech
        .handle(Request::SelectMode {
            mode: speech::Mode::Endpoint,
        })
        .await
        .unwrap();
    let (entered, ready) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut bytes = [0; 4096];
        assert!(stream.read(&mut bytes).await.unwrap() > 0);
        entered.send(()).unwrap();
        while stream.read(&mut bytes).await.unwrap() != 0 {}
    });
    let job = nucleus::new_uid("speech");
    let audio = ::fiote::config::Secret(base64::engine::general_purpose::STANDARD.encode(
        include_bytes!("../../interface/tests/fixtures/fiote/audio.wav"),
    ));
    speech
        .handle(Request::StartEndpoint {
            job: job.clone(),
            settings: settings.clone(),
            audio: audio.clone(),
        })
        .await
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), ready)
        .await
        .unwrap()
        .unwrap();
    let runtime = CellRuntime {
        store: engine.store.clone(),
        engine,
        lanes: Arc::new(LaneHub::new()),
        wire: Default::default(),
        speech: Some(speech.clone()),
        commands: Default::default(),
        information: None,
        fiote: None,
    };
    tokio::time::timeout(
        std::time::Duration::from_secs(3),
        Cell {
            runtime: runtime.clone(),
            supervisors: Vec::new(),
            tasks: Vec::new(),
        }
        .shutdown(),
    )
    .await
    .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    assert!(runtime.store.pool.is_closed());
    assert!(speech.handle(Request::Poll { job }).await.is_err());
    assert!(!speech.handle(Request::Inspect).await.unwrap().ready);
    assert!(
        speech
            .handle(Request::StartEndpoint {
                job: nucleus::new_uid("speech"),
                settings,
                audio
            })
            .await
            .unwrap_err()
            .contains("shutting down")
    );
}
