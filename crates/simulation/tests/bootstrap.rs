use simulation::{artifacts, fixtures, scenario::Database, world::World};

async fn audience(store: &store::Store) -> String {
    store::sqlx::query_scalar("SELECT audience FROM signal_endpoint_authority WHERE id = 1")
        .fetch_one(&store.pool)
        .await
        .unwrap()
}

fn run(work: impl Future<Output = ()> + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(work)
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn generated_authority_is_reproducible_and_distinct_from_live_bootstrap() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = fixtures::daily();
        case.cells[0].seed.truncate(1);
        let first = World::open(case.clone(), &directory.path().join("first"))
            .await
            .unwrap();
        let second = World::open(case.clone(), &directory.path().join("second"))
            .await
            .unwrap();
        assert_eq!(
            first.state_hash().await.unwrap(),
            second.state_hash().await.unwrap()
        );
        assert_eq!(first.trace, second.trace);
        let original = audience(&first.nodes["a"].engine().store).await;
        assert_eq!(original, audience(&second.nodes["a"].engine().store).await);
        case.seed += 1;
        let different = World::open(case, &directory.path().join("different"))
            .await
            .unwrap();
        assert_ne!(
            original,
            audience(&different.nodes["a"].engine().store).await
        );
        let live = store::Store::open_memory().await.unwrap();
        let another_live = store::Store::open_memory().await.unwrap();
        assert_ne!(audience(&live).await, audience(&another_live).await);
        assert_ne!(original, audience(&live).await);
    });
}

#[test]
fn copied_database_keeps_its_authority_and_source_state() {
    run(async {
        let directory = tempfile::tempdir().unwrap();
        let mut case = fixtures::daily();
        case.cells[0].seed.truncate(1);
        let source = World::open(case.clone(), &directory.path().join("source"))
            .await
            .unwrap();
        let store = &source.nodes["a"].engine().store;
        let original = audience(store).await;
        let before = store.state_hash().await.unwrap();
        let path = directory.path().join("input.sqlite");
        store.snapshot_into(&path).await.unwrap();
        case.cells[0].database = Some(Database {
            file: "input.sqlite".into(),
            hash: artifacts::file_hash(&path).unwrap(),
        });
        case.cells[0].seed.clear();
        case.checks.clear();
        let mut session =
            artifacts::Session::open(case, &directory.path().join("copy"), directory.path())
                .await
                .unwrap();
        assert_eq!(
            original,
            audience(&session.world.nodes["a"].engine().store).await
        );
        while session.step().await.unwrap() {}
        assert_eq!(
            original,
            audience(&session.world.nodes["a"].engine().store).await
        );
        session.finish().await.unwrap();
        assert_eq!(before, store.state_hash().await.unwrap());
    });
}
