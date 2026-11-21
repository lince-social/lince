// non-lean-yet
// Scope: fn authorize, fn engine.
// BeautifulRust (baseline lines):
// - [Clippy probe: clippy::missing_assert_message] assert without any message. Sites 45.
// - [rustc probe: unreachable_pub] unreachable `pub` item. Sites 7, 54.

use engine::{
    Engine,
    roster::{CellEntry, ROOT_KEY_ID},
    trust::Signer,
};

pub async fn authorize(engine: &Engine) {
    let organ = store::organs::ensure_local(&engine.store.pool, "http://karma.test")
        .await
        .expect("the Karma test requires a local Organ");
    let cell = store::cells::local(&engine.store.pool)
        .await
        .expect("read the local Cell for the Karma test")
        .expect("the Karma test requires a local Cell");
    let root = Signer::generate(&organ.uid, ROOT_KEY_ID);
    let key = Signer::generate(&organ.uid, &engine::roster::cell_key_id(&cell.uid));
    engine
        .set_signer(key.clone())
        .await
        .expect("install the Karma test signer");
    engine
        .publish_root_key(&root)
        .await
        .expect("publish the Karma test root key");
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
        .expect("publish the fixture roster with full capabilities");
    roster.roster.version += 1;
    roster.roster.not_after = "2100-01-01T00:00:00Z".into();
    let payload = engine::roster::roster_signing_payload(&roster.roster)
        .expect("serialize the extended Karma test roster for signing");
    roster.signature = root.sign_bytes(&payload);
    assert_eq!(
        engine
            .adopt_roster(&roster)
            .await
            .expect("adopt the Karma test roster"),
        engine::roster::RosterOutcome::Accepted
    );
}

pub async fn engine() -> Engine {
    let engine = Engine::open_memory()
        .await
        .expect("open the in-memory Karma test Engine");
    authorize(&engine).await;
    engine
}
