//! Stage 8b foundation: the record-metadata Actions the table sand and record
//! editor need — `edit-record-text`, `set-slug`, `set-concept`, `set-unit`,
//! `set-extension`. Each must apply the store mutation AND drop an annotation
//! fact so live subscriptions refresh (blueprint VII.4).

use engine::actions::Action;
use engine::Engine;
use nucleus::RecordKind;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord { slug: Some(slug), kind: RecordKind::Plain, head: slug, body: "", quantity: 0.0 },
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

    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
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
        Action::EditRecordText { target: uid.clone(), head: Some("H".into()), body: Some("B".into()) },
        None,
    )
    .await
    .unwrap();

    // body: None must not clobber the existing body
    e.act(
        Action::EditRecordText { target: uid.clone(), head: Some("H2".into()), body: None },
        None,
    )
    .await
    .unwrap();

    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
    assert_eq!(row.head, "H2");
    assert_eq!(row.body, "B");
}

#[tokio::test]
async fn set_slug_renames_and_clears() {
    let e = engine().await;
    let uid = plain(&e, "old-slug").await;

    e.act(Action::SetSlug { target: uid.clone(), slug: Some("new-slug".into()) }, None)
        .await
        .unwrap();
    assert_eq!(
        store::records::resolve(&e.store.pool, "new-slug").await.unwrap().map(|r| r.uid),
        Some(uid.clone())
    );

    // empty string clears the slug
    e.act(Action::SetSlug { target: uid.clone(), slug: Some(String::new()) }, None)
        .await
        .unwrap();
    assert!(store::records::get(&e.store.pool, &uid).await.unwrap().unwrap().slug.is_none());

    // invalid slug is rejected
    assert!(e
        .act(Action::SetSlug { target: uid.clone(), slug: Some("Not Valid".into()) }, None)
        .await
        .is_err());
}

#[tokio::test]
async fn set_concept_and_unit_classify_and_clear() {
    let e = engine().await;
    let uid = plain(&e, "apples").await;
    store::concepts::create(&e.store.pool, "fruit", &[]).await.unwrap();
    store::concepts::create(&e.store.pool, "kilogram", &[]).await.unwrap();

    e.act(Action::SetConcept { target: uid.clone(), concept: Some("fruit".into()) }, None)
        .await
        .unwrap();
    e.act(Action::SetUnit { target: uid.clone(), unit: Some("kilogram".into()) }, None)
        .await
        .unwrap();

    let row = store::records::get(&e.store.pool, &uid).await.unwrap().unwrap();
    let fruit = store::concepts::resolve(&e.store.pool, "fruit").await.unwrap();
    let kg = store::concepts::resolve(&e.store.pool, "kilogram").await.unwrap();
    assert_eq!(row.concept_uid, fruit);
    assert_eq!(row.unit_uid, kg);

    // unknown concept name is an error
    assert!(e
        .act(Action::SetConcept { target: uid.clone(), concept: Some("nope".into()) }, None)
        .await
        .is_err());

    // None clears
    e.act(Action::SetConcept { target: uid.clone(), concept: None }, None).await.unwrap();
    assert!(store::records::get(&e.store.pool, &uid).await.unwrap().unwrap().concept_uid.is_none());
}

#[tokio::test]
async fn compensate_reverses_a_quantity_fact() {
    let e = engine().await;
    let uid = plain(&e, "stock").await;

    // a quantity change to undo
    let out = e.act(Action::AddQuantity { target: uid.clone(), delta: 5.0 }, None).await.unwrap();
    assert_eq!(store::records::quantity(&e.store.pool, &uid).await.unwrap(), Some(5.0));
    let fact_uid = out.facts[0].uid.clone();

    // undo it: an inverse (-5) compensation fact restores the level
    let comp = e.act(Action::Compensate { fact: fact_uid }, None).await.unwrap();
    assert_eq!(comp.facts.len(), 1);
    assert_eq!(comp.facts[0].delta, -5.0);
    assert_eq!(store::records::quantity(&e.store.pool, &uid).await.unwrap(), Some(0.0));
}

#[tokio::test]
async fn compensate_zero_delta_fact_is_a_noop() {
    let e = engine().await;
    let uid = plain(&e, "note").await;

    // a text edit produces a zero-delta annotation fact
    let out = e
        .act(Action::EditRecordText { target: uid.clone(), head: Some("H".into()), body: None }, None)
        .await
        .unwrap();
    assert_eq!(out.facts[0].delta, 0.0);

    // compensating it changes nothing (nothing to reverse) and errors on unknown
    let comp = e.act(Action::Compensate { fact: out.facts[0].uid.clone() }, None).await.unwrap();
    assert!(comp.facts.is_empty());
    assert!(e.act(Action::Compensate { fact: "f_missing".into() }, None).await.is_err());
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
