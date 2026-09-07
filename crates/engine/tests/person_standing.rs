use engine::Engine;
use engine::sync::Delivery;
use engine::trust::Signer;

async fn cell() -> (Engine, String) {
    let engine = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&engine.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    engine
        .set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (engine, organ)
}

async fn become_sibling_of(engine: &Engine, organ_uid: &str) {
    let existing = store::organs::local(&engine.store.pool)
        .await
        .unwrap()
        .expect("local organ")
        .uid;
    let mut tx = store::write_tx(&engine.store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET slug = NULL WHERE uid = ?")
        .bind(&existing)
        .execute(&mut *tx)
        .await
        .unwrap();
    let assigned = store::sqlx::query(
        "UPDATE record SET slug = ? WHERE uid = ? AND kind = 'organ' AND deleted_at IS NULL",
    )
    .bind(store::organs::LOCAL_ORGAN_SLUG)
    .bind(organ_uid)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(assigned.rows_affected(), 1);
    for statement in [
        "UPDATE record SET organ_uid = ? WHERE organ_uid = ?",
        "UPDATE sync_op SET organ_uid = ? WHERE organ_uid = ?",
    ] {
        store::sqlx::query(statement)
            .bind(organ_uid)
            .bind(&existing)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    assert_eq!(
        store::organs::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        organ_uid
    );
    assert_eq!(
        store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .organ_uid,
        organ_uid
    );
    assert!(
        store::records::get(&engine.store.pool, &existing)
            .await
            .unwrap()
            .is_some()
    );
}

async fn pair(from: &Engine, from_organ: &str, to: &Engine, to_organ: &str) {
    let from_intro = from.introduction().await.unwrap();
    let to_intro = to.introduction().await.unwrap();
    to.adopt_introduction(&from_intro, 1).await.unwrap();
    from.adopt_introduction(&to_intro, 1).await.unwrap();
    store::organs::set_sync_policy(&from.store.pool, to_organ, true, false)
        .await
        .unwrap();
    store::organs::set_sync_policy(&to.store.pool, from_organ, true, false)
        .await
        .unwrap();
}

async fn deliver_to_sibling(from: &Engine, to: &Engine, organ: &str) {
    let (ops, _head) = from.ops_after(0, 1_000).await.unwrap();
    to.import_op_batch(&engine::sync::OpBatch {
        from_organ: organ.to_string(),
        ops,
    })
    .await
    .expect("import");
}

async fn deliver(from: &Engine, to: &Engine) {
    let target = to;
    from.drain_outbox(|_contact, root, batch| async move {
        match root {
            Some(root) => target.import_grant_batch(&root, &batch).await,
            None => target.import_op_batch(&batch).await,
        }
        .map_or_else(|e| Delivery::Failed(e.to_string()), |_| Delivery::Sent)
    })
    .await
    .expect("drain");
}

async fn person(engine: &Engine, slug: &str) -> String {
    store::records::create(
        &engine.store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: nucleus::RecordKind::Person,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("person record")
    .uid
}

#[tokio::test]
async fn deactivating_someone_reaches_this_organs_other_cell() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let uid = person(&a, "maria").await;
    deliver_to_sibling(&a, &b, &organ).await;
    assert!(
        store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "precondition: she can act on both Cells to begin with"
    );

    store::people::deactivate(
        &a.store.pool,
        &uid,
        "2026-08-15T12:00:00Z",
        Some("moved out"),
    )
    .await
    .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        !store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "the other Cell must stop accepting her too"
    );
    assert_eq!(
        store::people::standing(&b.store.pool, &uid)
            .await
            .unwrap()
            .and_then(|standing| standing.at),
        Some("2026-08-15T12:00:00Z".to_string()),
    );
}

#[tokio::test]
async fn reactivating_reaches_the_other_cell_too() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;
    become_sibling_of(&b, &organ).await;

    let uid = person(&a, "joao").await;
    store::people::deactivate(&a.store.pool, &uid, "2026-08-15T12:00:00Z", None)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;
    assert!(!store::people::is_active(&b.store.pool, &uid).await.unwrap());

    store::people::reactivate(&a.store.pool, &uid)
        .await
        .unwrap();
    deliver_to_sibling(&a, &b, &organ).await;

    assert!(
        store::people::is_active(&b.store.pool, &uid).await.unwrap(),
        "the other Cell must let him back in"
    );
}

#[tokio::test]
async fn a_contacts_feed_never_carries_a_persons_standing() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;

    let uid = person(&a, "maria").await;
    store::people::deactivate(
        &a.store.pool,
        &uid,
        "2026-08-15T12:00:00Z",
        Some("moved out"),
    )
    .await
    .unwrap();

    let sent = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let collector = std::sync::Arc::clone(&sent);
    a.drain_outbox(move |_contact, _root, batch| {
        let collector = std::sync::Arc::clone(&collector);
        async move {
            collector.lock().unwrap().extend(batch.ops.iter().cloned());
            Delivery::Sent
        }
    })
    .await
    .expect("drain");

    let sent = sent.lock().unwrap();
    assert!(
        sent.iter().any(|op| op.uid == uid),
        "precondition: the Person herself does travel, so the absence below is the filter"
    );
    assert!(
        !sent
            .iter()
            .any(|op| store::people::is_standing_field(&op.field)),
        "our membership admin is nobody else's business: {:?}",
        sent.iter().map(|op| &op.field).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn a_contact_cannot_deactivate_one_of_our_people() {
    let (a, organ) = cell().await;
    let (b, b_organ) = cell().await;
    pair(&a, &organ, &b, &b_organ).await;

    let uid = person(&a, "maria").await;
    deliver(&a, &b).await;

    let hostile = engine::sync::OpBatch {
        from_organ: b_organ.clone(),
        ops: vec![engine::sync::WireOp {
            tbl: "record_extension".into(),
            uid: uid.clone(),
            field: format!(
                "{}.{}",
                store::people::NAMESPACE,
                store::people::STANDING_KEY
            ),
            kind: "set".into(),
            value: Some(
                serde_json::json!({ "active": false, "at": "2026-08-15T12:00:00Z" }).to_string(),
            ),
            hlc: nucleus::hlc::next(),
            actor_cell: format!("{b_organ}-cell"),
            organ_uid: b_organ.clone(),
            fact: None,
        }],
    };
    a.import_op_batch(&hostile).await.expect("import returns");

    assert!(
        store::people::is_active(&a.store.pool, &uid).await.unwrap(),
        "a contact must not be able to close an account in our Organ"
    );
    assert_eq!(
        store::records::get_extension(&a.store.pool, &uid, store::people::NAMESPACE)
            .await
            .unwrap(),
        None,
        "and it must not even land — refused, not stored-and-ignored"
    );
}
