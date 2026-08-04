//! Op log foundation (Ontology §11): every local write on a syncable table
//! becomes a field-level op — (tbl, uid, field, kind, value, hlc, actor) —
//! stamped by the Cell's HLC, with the local organ as actor. A Cell without a
//! local organ has no sync identity and logs nothing.

use engine::Engine;
use engine::actions::Action;
use engine::trust::Signer;
use nucleus::RecordKind;
use store::records::NewRecord;
use store::sync_ops;

async fn cell() -> (Engine, String) {
    let e = Engine::open_memory().await.expect("engine opens");
    let organ = store::organs::ensure_local(&e.store.pool, "http://cell.test")
        .await
        .expect("local organ")
        .uid;
    e.set_signer(Signer::generate(&organ, "k1"))
        .await
        .expect("signer");
    (e, organ)
}

async fn plain(e: &Engine, slug: &str) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .map(|r| r.uid)
    .expect("record")
}

async fn ops_for(e: &Engine, tbl: &str, uid: &str, field: &str) -> Vec<sync_ops::OpRow> {
    sync_ops::for_field(&e.store.pool, tbl, uid, field)
        .await
        .expect("ops")
}

#[tokio::test]
async fn record_create_and_edit_log_field_ops() {
    let (e, organ) = cell().await;
    let uid = plain(&e, "apples").await;

    // Creation logged the initial fields.
    for field in ["kind", "head", "body", "slug", "organ_uid"] {
        let ops = ops_for(&e, "record", &uid, field).await;
        assert_eq!(ops.len(), 1, "one create op for {field}");
        assert_eq!(ops[0].kind, "set");
        assert_eq!(ops[0].actor_organ, organ);
    }

    // A text edit logs ONE cumulative crdt op (the record-doc owns text);
    // head/body set ops stay create-era only.
    e.act(
        Action::EditRecordText {
            target: uid.clone(),
            head: Some("Apples".into()),
            body: Some("crisp".into()),
        },
        None,
    )
    .await
    .expect("edit");
    assert_eq!(ops_for(&e, "record", &uid, "head").await.len(), 1);
    let doc_ops = ops_for(&e, "record", &uid, "").await;
    assert_eq!(doc_ops.len(), 1);
    assert_eq!(doc_ops[0].kind, "crdt");
    assert!(
        doc_ops[0].hlc > ops_for(&e, "record", &uid, "head").await[0].hlc,
        "HLC is monotonic"
    );

    // Delete is a tombstone op, never a missing row in the log.
    e.act(
        Action::DeleteRecord {
            target: uid.clone(),
        },
        None,
    )
    .await
    .expect("delete");
    let tombstones: Vec<_> = ops_for(&e, "record", &uid, "")
        .await
        .into_iter()
        .filter(|op| op.kind == "tombstone")
        .collect();
    assert_eq!(tombstones.len(), 1);
}

#[tokio::test]
async fn extension_writes_diff_per_key() {
    let (e, _) = cell().await;
    let uid = plain(&e, "task").await;
    let ns = "work.tracking";

    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 3, "owner": "ana" }),
    )
    .await
    .expect("first set");
    // Change one key, drop one, keep nothing else equal.
    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 5 }),
    )
    .await
    .expect("second set");

    let estimate = ops_for(&e, "record_extension", &uid, "work.tracking.estimate").await;
    assert_eq!(estimate.len(), 2, "changed key logs a new set op");
    assert_eq!(estimate[1].value.as_deref(), Some("5"));

    let owner = ops_for(&e, "record_extension", &uid, "work.tracking.owner").await;
    assert_eq!(owner.len(), 2);
    assert_eq!(owner[1].kind, "tombstone", "removed key logs a tombstone");

    // An unchanged write logs nothing new.
    store::records::set_extension(
        &e.store.pool,
        &uid,
        ns,
        &serde_json::json!({ "estimate": 5 }),
    )
    .await
    .expect("noop set");
    assert_eq!(
        ops_for(&e, "record_extension", &uid, "work.tracking.estimate")
            .await
            .len(),
        2
    );
}

#[tokio::test]
async fn assertions_and_facts_join_the_log() {
    let (e, _) = cell().await;
    let subject = plain(&e, "note").await;
    let concept = store::concepts::create(&e.store.pool, "todo", &[])
        .await
        .expect("concept");

    // Concept creation is a plain per-field set op.
    let concept_ops = ops_for(&e, "concept", &concept, "canonical_name").await;
    assert_eq!(concept_ops.len(), 1);
    assert_eq!(concept_ops[0].value.as_deref(), Some("\"todo\""));

    // Assert / retract are set / tombstone per assertion uid.
    let assertion = store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &subject,
            predicate_uid: &concept,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .expect("assert");
    let ops = ops_for(&e, "record_assertion", &assertion, "").await;
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0].kind, "set");
    let value: serde_json::Value = serde_json::from_str(ops[0].value.as_deref().unwrap()).unwrap();
    assert_eq!(value["subject_uid"], subject.as_str());
    assert_eq!(value["predicate_uid"], concept.as_str());

    store::assertions::retract(&e.store.pool, &assertion, None)
        .await
        .expect("retract");
    let ops = ops_for(&e, "record_assertion", &assertion, "").await;
    assert_eq!(ops.len(), 2);
    assert_eq!(ops[1].kind, "tombstone");

    // A quantity write is a fact — it joins the log as kind `fact`.
    e.append_user(&subject, 2.0).await.expect("bump");
    let facts = store::facts::for_record(&e.store.pool, &subject, 100)
        .await
        .expect("facts");
    let bump_fact = facts
        .iter()
        .find(|f| !f.delta.is_zero())
        .expect("bump fact");
    let fact_ops = ops_for(&e, "fact", &bump_fact.uid, "").await;
    assert_eq!(fact_ops.len(), 1);
    assert_eq!(fact_ops[0].kind, "fact");
    assert!(fact_ops[0].value.is_none());
}

#[tokio::test]
async fn no_local_organ_means_no_ops() {
    let e = Engine::open_memory().await.expect("engine");
    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "loose",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    assert!(ops_for(&e, "record", &uid, "head").await.is_empty());
}

#[tokio::test]
async fn op_identity_is_actor_plus_hlc() {
    let (e, organ) = cell().await;
    let hlc = nucleus::hlc::next();
    let first = sync_ops::append(
        &e.store.pool,
        "record",
        "r-x",
        "head",
        sync_ops::OpKind::Set,
        Some("\"a\""),
        hlc,
        &organ,
        None,
        None,
    )
    .await
    .expect("append");
    assert!(first.is_some());
    // Same (actor, hlc) again: idempotent no-op, not an error.
    let dup = sync_ops::append(
        &e.store.pool,
        "record",
        "r-x",
        "head",
        sync_ops::OpKind::Set,
        Some("\"b\""),
        hlc,
        &organ,
        None,
        None,
    )
    .await
    .expect("dup append");
    assert!(dup.is_none());
}
