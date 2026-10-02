use engine::{
    Engine,
    roster::{CellEntry, ROOT_KEY_ID},
    trust::Signer,
};

pub async fn authorize(engine: &Engine) {
    let organ = store::organs::ensure_local(&engine.store.pool, "http://karma.test")
        .await
        .unwrap();
    let cell = store::cells::local(&engine.store.pool)
        .await
        .unwrap()
        .unwrap();
    let root = Signer::generate(&organ.uid, ROOT_KEY_ID);
    let key = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine.set_signer(key.clone()).await.unwrap();
    engine.publish_root_key(&root).await.unwrap();
    let mut roster = engine
        .publish_roster(
            &root,
            vec![CellEntry {
                cell_uid: cell.uid,
                node_id: "karma-test-node".into(),
                label: cell.label,
                operational_key: key.public_key_b64(),
                sealing_key: None,
                front_door: false,
                capabilities: engine::roster::full_capabilities(),
            }],
        )
        .await
        .unwrap();
    roster.roster.version += 1;
    roster.roster.not_after = "2100-01-01T00:00:00Z".into();
    roster.signature =
        root.sign_bytes(&engine::roster::roster_signing_payload(&roster.roster).unwrap());
    assert_eq!(
        engine.adopt_roster(&roster).await.unwrap(),
        engine::roster::RosterOutcome::Accepted
    );
}

pub async fn engine() -> Engine {
    let engine = Engine::open_memory().await.unwrap();
    authorize(&engine).await;
    engine
}
