mod support;

use engine::{Engine, actions::Action, sync::OpBatch};
use serde_json::json;

async fn pair() -> (Engine, Engine, String, String) {
    let a = Engine::open_memory().await.unwrap();
    let b = Engine::open_memory().await.unwrap();
    support::karma::authorize(&a).await;
    support::karma::authorize(&b).await;
    let ao = store::organs::local(&a.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bo = store::organs::local(&b.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    a.adopt_introduction(&b.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    b.adopt_introduction(&a.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    (a, b, ao, bo)
}

#[tokio::test]
async fn saved_delivery_modes_are_validated_and_visible_through_the_contact_query() {
    let (a, _b, _ao, bo) = pair().await;
    for mode in ["direct", "mailbox", "auto"] {
        a.act(
            Action::SetContactDelivery {
                target: bo.clone(),
                mode: mode.into(),
            },
            None,
        )
        .await
        .unwrap();
        let query=serde_json::from_value(json!({"source":"record","where":[{"uid_eq":bo}],"fields":["uid","contact"],"include":{"contact":true}})).unwrap();
        let rows = protein::execute(&a.store, &query).await.unwrap();
        assert_eq!(rows[0]["contact"]["delivery"], mode);
    }
    assert!(
        a.act(
            Action::SetContactDelivery {
                target: bo.clone(),
                mode: "anything".into()
            },
            None
        )
        .await
        .is_err()
    );
    assert_eq!(
        store::organs::contact(&a.store.pool, &bo)
            .await
            .unwrap()
            .unwrap()
            .delivery(),
        store::organs::Delivery::Auto
    );
}

#[tokio::test]
async fn saved_mail_records_the_sharing_authority_used_to_prepare_it() {
    let a = Engine::open_memory().await.unwrap();
    let b = Engine::open_memory().await.unwrap();
    let ao = store::organs::local(&a.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let bo = store::organs::local(&b.store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let cell = store::cells::local(&b.store.pool).await.unwrap().unwrap();
    a.set_organ_signer(engine::trust::Signer::generate(&ao, "mail"))
        .await
        .unwrap();
    let root = engine::trust::Signer::generate(&bo, engine::roster::ROOT_KEY_ID);
    b.publish_root_key(&root).await.unwrap();
    let (_, key) = engine::seal::generate(&cell.uid, 1, "2099-01-01T00:00:00Z");
    b.publish_roster(
        &root,
        vec![engine::roster::CellEntry {
            cell_uid: cell.uid,
            node_id: "mail-device".into(),
            label: "Device".into(),
            operational_key: root.public_key_b64(),
            sealing_key: Some(key),
            front_door: false,
            capabilities: engine::roster::full_capabilities(),
        }],
    )
    .await
    .unwrap();
    a.adopt_introduction(&b.introduction().await.unwrap(), 1)
        .await
        .unwrap();
    a.adopt_roster(&b.roster_of(&bo).await.unwrap().unwrap())
        .await
        .unwrap();
    let batch = OpBatch {
        from_organ: ao,
        ops: Vec::new(),
    };
    let queued = a.prepare_outgoing_mail(&bo, None, &batch).await.unwrap();
    let stored: String =
        store::sqlx::query_scalar("SELECT policy_hash FROM mailbox_outbox_authority WHERE uid=?")
            .bind(&queued.uid)
            .fetch_one(&a.store.pool)
            .await
            .unwrap();
    store::organs::set_contact_scope(&a.store.pool, &bo, Some(&["head".into()]))
        .await
        .unwrap();
    let changed = a.prepare_outgoing_mail(&bo, None, &batch).await.unwrap();
    assert_ne!(changed.uid, queued.uid);
    let kept: String =
        store::sqlx::query_scalar("SELECT policy_hash FROM mailbox_outbox_authority WHERE uid=?")
            .bind(&queued.uid)
            .fetch_one(&a.store.pool)
            .await
            .unwrap();
    assert_eq!(stored, kept);
    let sender = std::sync::Arc::new(a);
    let wire = engine::wire::Wire::bind(
        sender.clone(),
        iroh::SecretKey::from_bytes(&[121; 32]),
        engine::wire::Reach::Local,
    )
    .await
    .unwrap();
    wire.retry_saved_mail_once().await.unwrap();
    let held_error: Option<String> =
        store::sqlx::query_scalar("SELECT error FROM mailbox_outbox WHERE uid=?")
            .bind(&queued.uid)
            .fetch_one(&sender.store.pool)
            .await
            .unwrap();
    assert!(held_error.unwrap().contains("permissions"));
    wire.shutdown().await;
    assert!(
        store::mailbox::outbox::receipts(&sender.store.pool, &queued.uid)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn delivery_choice_survives_a_durable_database_restart() {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", dir.path().join("delivery.db").display());
    let a = Engine::open(&url).await.unwrap();
    let b = Engine::open_memory().await.unwrap();
    let intro = b.introduction().await.unwrap();
    a.adopt_introduction(&intro, 1).await.unwrap();
    a.act(
        Action::SetContactDelivery {
            target: intro.organ_uid.clone(),
            mode: "direct".into(),
        },
        None,
    )
    .await
    .unwrap();
    a.store.pool.close().await;
    let reopened = Engine::open(&url).await.unwrap();
    assert_eq!(
        store::organs::contact(&reopened.store.pool, &intro.organ_uid)
            .await
            .unwrap()
            .unwrap()
            .delivery(),
        store::organs::Delivery::Direct
    );
}
