//! Stage 8b foundation: the record-metadata Actions the table sand and record
//! editor need — `edit-record-text`, `set-slug`, `set-concept`, `set-unit`,
//! `set-extension`. Each must apply the store mutation AND drop an annotation
//! fact so live subscriptions refresh (blueprint VII.4).

use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: 0.0,
        },
    )
    .await
    .expect("record")
    .uid
}

#[tokio::test]
async fn edit_record_text_sets_fields_and_annotates() {
    let e = engine().await;
    let uid = plain(&e, "note").await;

    let out = e
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("New Title".into()),
                body: Some("Long body".into()),
            },
            None,
        )
        .await
        .expect("edit");

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "New Title");
    assert_eq!(row.body, "Long body");
    // exactly one annotation fact so subscriptions refresh, and it is zero-delta
    assert_eq!(out.facts.len(), 1);
    assert_eq!(out.facts[0].delta, 0.0);
    assert_eq!(out.facts[0].record_uid, uid);
}

#[tokio::test]
async fn edit_record_text_leaves_absent_field_untouched() {
    let e = engine().await;
    let uid = plain(&e, "note2").await;
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("H".into()),
            body: Some("B".into()),
        },
        None,
    )
    .await
    .unwrap();

    // body: None must not clobber the existing body
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("H2".into()),
            body: None,
        },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.head, "H2");
    assert_eq!(row.body, "B");
}

#[tokio::test]
async fn set_slug_renames_and_clears() {
    let e = engine().await;
    let uid = plain(&e, "old-slug").await;

    e.act(
        Action::SetSlug {
            target: uid.clone(),
            slug: Some("new-slug".into()),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        store::records::resolve(&e.store.pool, "new-slug")
            .await
            .unwrap()
            .map(|r| r.uid),
        Some(uid.clone())
    );

    // empty string clears the slug
    e.act(
        Action::SetSlug {
            target: uid.clone(),
            slug: Some(String::new()),
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .slug
            .is_none()
    );

    // invalid slug is rejected
    assert!(
        e.act(
            Action::SetSlug {
                target: uid.clone(),
                slug: Some("Not Valid".into())
            },
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn set_concept_and_unit_classify_and_clear() {
    let e = engine().await;
    let uid = plain(&e, "apples").await;
    store::concepts::create(&e.store.pool, "fruit", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kilogram", &[])
        .await
        .unwrap();

    e.act(
        Action::SetConcept {
            target: uid.clone(),
            concept: Some("fruit".into()),
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::SetUnit {
            target: uid.clone(),
            unit: Some("kilogram".into()),
        },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let fruit = store::concepts::resolve(&e.store.pool, "fruit")
        .await
        .unwrap();
    let kg = store::concepts::resolve(&e.store.pool, "kilogram")
        .await
        .unwrap();
    assert_eq!(row.concept_uid, fruit);
    assert_eq!(row.unit_uid, kg);

    // unknown concept name is an error
    assert!(
        e.act(
            Action::SetConcept {
                target: uid.clone(),
                concept: Some("nope".into())
            },
            None
        )
        .await
        .is_err()
    );

    // None clears
    e.act(
        Action::SetConcept {
            target: uid.clone(),
            concept: None,
        },
        None,
    )
    .await
    .unwrap();
    assert!(
        store::records::get(&e.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .concept_uid
            .is_none()
    );
}

#[tokio::test]
async fn compensate_reverses_a_quantity_fact() {
    let e = engine().await;
    let uid = plain(&e, "stock").await;

    // a quantity change to undo
    let out = e
        .act(
            Action::AddQuantity {
                target: uid.clone(),
                delta: 5.0,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        store::records::quantity(&e.store.pool, &uid).await.unwrap(),
        Some(5.0)
    );
    let fact_uid = out.facts[0].uid.clone();

    // undo it: an inverse (-5) compensation fact restores the level
    let comp = e
        .act(Action::Compensate { fact: fact_uid }, None)
        .await
        .unwrap();
    assert_eq!(comp.facts.len(), 1);
    assert_eq!(comp.facts[0].delta, -5.0);
    assert_eq!(
        store::records::quantity(&e.store.pool, &uid).await.unwrap(),
        Some(0.0)
    );
}

#[tokio::test]
async fn compensate_zero_delta_fact_is_a_noop() {
    let e = engine().await;
    let uid = plain(&e, "note").await;

    // a text edit produces a zero-delta annotation fact
    let out = e
        .act(
            Action::EditRecordText {
                target: uid.clone(),
                head: Some("H".into()),
                body: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.facts[0].delta, 0.0);

    // compensating it changes nothing (nothing to reverse) and errors on unknown
    let comp = e
        .act(
            Action::Compensate {
                fact: out.facts[0].uid.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert!(comp.facts.is_empty());
    assert!(
        e.act(
            Action::Compensate {
                fact: "f_missing".into()
            },
            None
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn set_extension_writes_readable_sidecar() {
    let e = engine().await;
    let uid = plain(&e, "widget").await;

    e.act(
        Action::SetExtension {
            target: uid.clone(),
            namespace: "board.card".into(),
            fds: serde_json::json!({ "x": 10, "y": 20 }),
        },
        None,
    )
    .await
    .unwrap();

    let got = store::records::get_extension(&e.store.pool, &uid, "board.card")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got, serde_json::json!({ "x": 10, "y": 20 }));
}

#[tokio::test]
async fn record_threads_and_messages_are_records_plus_links() {
    let e = engine().await;
    let subject = plain(&e, "abstract-idea").await;

    let thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion A".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let first = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "First message".into(),
                parent: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let _reply = e
        .act(
            Action::CreateMessage {
                thread: thread.clone(),
                body: "Reply message".into(),
                parent: Some(first.clone()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let other_thread = e
        .act(
            Action::CreateThread {
                target: subject.clone(),
                head: "Discussion B".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    assert!(
        e.act(
            Action::CreateMessage {
                thread: other_thread,
                body: "Cross-thread reply".into(),
                parent: Some(first.clone()),
            },
            None,
        )
        .await
        .is_err(),
        "replies must stay inside their thread"
    );

    assert_eq!(
        store::records::get(&e.store.pool, &thread)
            .await
            .unwrap()
            .unwrap()
            .kind,
        "thread"
    );
    assert_eq!(
        store::records::get(&e.store.pool, &first)
            .await
            .unwrap()
            .unwrap()
            .kind,
        "message"
    );

    assert!(
        store::concepts::resolve(&e.store.pool, "thread-of")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::concepts::resolve(&e.store.pool, "message-in")
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        store::concepts::resolve(&e.store.pool, "reply-to")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn link_actions_annotate_affected_records() {
    let e = engine().await;
    let a = plain(&e, "link-a").await;
    let b = plain(&e, "link-b").await;
    e.act(
        Action::CreateConcept {
            name: "contributes".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();

    let added = e
        .act(
            Action::AddLink {
                from: a.clone(),
                kind: "contributes".into(),
                to: b.clone(),
                quantity: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(added.facts.len(), 2);
    assert!(added.facts.iter().all(|fact| fact.delta == 0.0));
    assert!(added.facts.iter().any(|fact| fact.record_uid == a));
    assert!(added.facts.iter().any(|fact| fact.record_uid == b));

    let removed = e
        .act(
            Action::RemoveLink {
                from: a.clone(),
                kind: "contributes".into(),
                to: b.clone(),
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(removed.facts.len(), 2);
}

#[tokio::test]
async fn relink_order_rewrites_adjacent_order_links() {
    let e = engine().await;
    let a = plain(&e, "order-a").await;
    let b = plain(&e, "order-b").await;
    let c = plain(&e, "order-c").await;
    e.act(
        Action::CreateConcept {
            name: "order".into(),
            parents: vec![],
        },
        None,
    )
    .await
    .unwrap();
    e.act(
        Action::AddLink {
            from: a.clone(),
            kind: "order".into(),
            to: c.clone(),
            quantity: None,
        },
        None,
    )
    .await
    .unwrap();

    let out = e
        .act(
            Action::RelinkOrder {
                kind: "order".into(),
                ordered: vec![a.clone(), b.clone(), c.clone()],
                reverse: false,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(out.facts.len(), 3);

    let kind_uid = store::concepts::resolve(&e.store.pool, "order")
        .await
        .unwrap()
        .unwrap();
    let edges = store::links::edges_of_kind(&e.store.pool, &kind_uid)
        .await
        .unwrap();
    assert_eq!(edges.len(), 2);
    assert!(edges.iter().any(|edge| edge.from == a && edge.to == b));
    assert!(edges.iter().any(|edge| edge.from == b && edge.to == c));
}

#[tokio::test]
async fn delete_record_is_distinct_from_deactivate() {
    let e = engine().await;
    let uid = plain(&e, "doomed").await;
    e.act(Action::SetQuantity { target: uid.clone(), value: 3.0 }, None)
        .await
        .expect("give it a quantity");

    // Deactivate ONLY zeroes the quantity — the record stays readable.
    e.act(Action::Deactivate { target: uid.clone() }, None)
        .await
        .expect("deactivate");
    let row = store::records::get(&e.store.pool, &uid)
        .await
        .unwrap()
        .expect("deactivated record still exists");
    assert_eq!(row.quantity, 0.0);
    assert_eq!(row.slug.as_deref(), Some("doomed"));

    // HARD delete tombstones it: gone from get/resolve/list, slug freed,
    // Ledger facts untouched (the deletion annotation is the last one).
    let out = e
        .act(Action::DeleteRecord { target: "doomed".into() }, None)
        .await
        .expect("delete");
    assert_eq!(out.facts.len(), 1);
    assert_eq!(out.facts[0].delta, 0.0);
    assert!(
        store::records::get(&e.store.pool, &uid).await.unwrap().is_none(),
        "deleted record must not be readable"
    );
    assert!(
        store::records::resolve(&e.store.pool, "doomed").await.unwrap().is_none(),
        "deleted record must not resolve by slug"
    );
    assert!(
        !store::records::list_all(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .any(|r| r.uid == uid),
        "deleted record must not appear in the record base set"
    );
    let facts = store::facts::for_record(&e.store.pool, &uid, 50).await.unwrap();
    assert!(
        facts.len() >= 3,
        "creation-era + deactivate + deletion facts stay in the Ledger"
    );

    // The freed slug is reusable by a NEW record.
    let reused = e
        .act(
            Action::CreateRecord {
                slug: Some("doomed".into()),
                kind: nucleus::RecordKind::Plain,
                head: "reborn".into(),
                body: String::new(),
                quantity: 0.0,
            },
            None,
        )
        .await
        .expect("slug is free again");
    let new_uid = reused.created.expect("created uid");
    assert_ne!(new_uid, uid);
}
