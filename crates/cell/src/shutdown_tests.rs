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
