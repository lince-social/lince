use std::collections::BTreeSet;
use std::sync::Arc;

use nucleus::{DecimalValue, RecordKind};
use serde_json::json;
use store::Store;
use store::access_snapshot::{
    AccessSnapshotLimits, AssertionRole, LinguaVisibility, VisibilityGrant, VisibilitySubject,
    VisibilityTarget,
};

async fn record(
    store: &Store,
    kind: RecordKind,
    head: &str,
    body: &str,
    quantity: DecimalValue,
) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind,
            head,
            body,
            quantity,
        },
    )
    .await
    .unwrap()
    .uid
}

async fn metadata(store: &Store) -> store::access_snapshot::AccessMetadataSnapshot {
    let mut tx = store.pool.begin().await.unwrap();
    let snapshot = store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    snapshot
}

async fn local_organ(store: &Store) -> String {
    store::organs::local(&store.pool)
        .await
        .unwrap()
        .unwrap()
        .uid
}

#[tokio::test]
async fn metadata_is_complete_without_record_bodies() {
    let store = Store::open_memory().await.unwrap();
    let organ = local_organ(&store).await;
    let root = record(
        &store,
        RecordKind::Plain,
        "Private root",
        "root body",
        store::exact::zero(),
    )
    .await;
    let deleted = record(
        &store,
        RecordKind::Plain,
        "Deleted",
        "deleted body",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query("UPDATE record SET replica_root = uid WHERE uid = ?")
        .bind(&root)
        .execute(&store.pool)
        .await
        .unwrap();
    let foreign = store::records::create_in_root(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Foreign-root member",
            body: "private member body",
            quantity: store::exact::zero(),
        },
        Some(&root),
    )
    .await
    .unwrap()
    .uid;
    store::records::mark_deleted(&store.pool, &deleted)
        .await
        .unwrap();

    let parent = store::concepts::create(&store.pool, "parent", &[])
        .await
        .unwrap();
    let child = store::concepts::create(&store.pool, "child", &[&parent])
        .await
        .unwrap();
    store::sqlx::query("UPDATE concept SET origin_organ = ? WHERE uid = ?")
        .bind(&organ)
        .bind(&child)
        .execute(&store.pool)
        .await
        .unwrap();
    let assertion_uid = store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &root,
            predicate_uid: &child,
            object_uid: Some(&deleted),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: Some(DecimalValue::parse_canonical(3, "12.300").unwrap()),
            unit_uid: Some(&parent),
            asserted_by: Some(&organ),
        },
    )
    .await
    .unwrap();
    let retracted = store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &root,
            predicate_uid: &parent,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: Some(&organ),
        },
    )
    .await
    .unwrap();
    store::assertions::retract(&store.pool, &retracted, Some(&organ))
        .await
        .unwrap();
    let place = store::places::create(&store.pool, 1.0, 2.0, None)
        .await
        .unwrap();
    let lingua = store::linguas::create(&store.pool, "Shared", Some(&organ), "shared")
        .await
        .unwrap();
    store::linguas::adopt(&store.pool, &lingua, &child)
        .await
        .unwrap();

    let snapshot = metadata(&store).await;
    let deleted_row = snapshot
        .records
        .iter()
        .find(|row| row.uid == deleted)
        .unwrap();
    assert!(deleted_row.deleted);
    let root_row = snapshot.records.iter().find(|row| row.uid == root).unwrap();
    assert_eq!(root_row.replica_root.as_deref(), Some(root.as_str()));
    assert!(
        snapshot.records.iter().any(|row| {
            row.uid == foreign && row.replica_root.as_deref() == Some(root.as_str())
        })
    );
    assert!(snapshot.concepts.iter().any(|concept| {
        concept.uid == child && concept.origin_organ.as_deref() == Some(organ.as_str())
    }));
    assert!(
        snapshot
            .concept_parents
            .iter()
            .any(|edge| edge.concept_uid == child && edge.parent_uid == parent)
    );
    let assertion = snapshot
        .assertions
        .iter()
        .find(|assertion| assertion.uid == assertion_uid)
        .unwrap();
    assert_eq!(assertion.role, AssertionRole::Ordinary);
    assert_eq!(
        assertion.quantity,
        Some(DecimalValue::parse_canonical(3, "12.300").unwrap())
    );
    assert!(
        !snapshot
            .assertions
            .iter()
            .any(|assertion| assertion.uid == retracted)
    );
    assert!(snapshot.places.contains(&place));
    assert!(snapshot.linguas.iter().any(|row| {
        row.uid == lingua
            && row.owner_organ.as_deref() == Some(organ.as_str())
            && row.visibility == LinguaVisibility::Shared
    }));
    assert!(
        snapshot
            .lingua_concepts
            .iter()
            .any(|row| row.lingua_uid == lingua && row.concept_uid == child)
    );
}

#[tokio::test]
async fn metadata_does_not_materialize_record_bodies() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(
        &store,
        RecordKind::Plain,
        "Small metadata",
        &"x".repeat(4096),
        store::exact::zero(),
    )
    .await;
    let mut limits = AccessSnapshotLimits::default();
    limits.row_bytes = 256;
    let mut tx = store.pool.begin().await.unwrap();
    let snapshot = store::access_snapshot::metadata_on(&mut tx, &limits)
        .await
        .unwrap();
    assert!(snapshot.records.iter().any(|row| row.uid == uid));
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&uid), &limits)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn targeted_content_includes_deleted_records_exact_values_and_every_extension() {
    let store = Store::open_memory().await.unwrap();
    let quantity = DecimalValue::parse_canonical(4, "-10.2500").unwrap();
    let uid = record(&store, RecordKind::Plain, "Complete", "full body", quantity).await;
    let unit = store::concepts::create(&store.pool, "kilogram", &[])
        .await
        .unwrap();
    let place = store::places::create(&store.pool, 3.0, 4.0, None)
        .await
        .unwrap();
    store::records::set_unit(&store.pool, &uid, Some(&unit))
        .await
        .unwrap();
    store::places::set_record_place(&store.pool, &uid, &place)
        .await
        .unwrap();
    store::records::set_extension(&store.pool, &uid, "alpha", &json!({"one": 1}))
        .await
        .unwrap();
    store::records::set_extension(&store.pool, &uid, "beta", &json!({"two": [2, 3]}))
        .await
        .unwrap();
    store::sqlx::query(
        "UPDATE record_extension SET version = 2 WHERE record_uid = ? AND namespace = 'beta'",
    )
    .bind(&uid)
    .execute(&store.pool)
    .await
    .unwrap();
    store::records::mark_deleted(&store.pool, &uid)
        .await
        .unwrap();

    let mut tx = store.pool.begin().await.unwrap();
    let content = store::access_snapshot::content_on(
        &mut tx,
        std::slice::from_ref(&uid),
        &AccessSnapshotLimits::default(),
    )
    .await
    .unwrap();
    tx.rollback().await.unwrap();
    let row = content.get(&uid).unwrap();
    assert!(row.deleted);
    assert_eq!(row.kind, RecordKind::Plain);
    assert_eq!(row.head, "Complete");
    assert_eq!(row.body, "full body");
    assert_eq!(row.quantity, quantity);
    assert_eq!(row.unit_uid.as_deref(), Some(unit.as_str()));
    assert_eq!(row.place_uid.as_deref(), Some(place.as_str()));
    assert_eq!(row.extensions.len(), 2);
    assert_eq!(
        row.extensions["alpha"].fields,
        json!({"one": 1}).as_object().unwrap().clone()
    );
    assert_eq!(row.extensions["beta"].version, 2);
    assert_eq!(
        row.extensions["beta"].fields,
        json!({"two": [2, 3]}).as_object().unwrap().clone()
    );
}

#[tokio::test]
async fn visibility_rules_are_validated_and_unsupported_restrictions_are_retained() {
    let store = Store::open_memory().await.unwrap();
    let person = record(
        &store,
        RecordKind::Person,
        "Person",
        "",
        store::exact::zero(),
    )
    .await;
    let organ = local_organ(&store).await;
    let target = record(
        &store,
        RecordKind::Plain,
        "Target",
        "",
        store::exact::zero(),
    )
    .await;
    let concept = store::concepts::create(&store.pool, "Visible concept", &[])
        .await
        .unwrap();
    let place = store::places::create(&store.pool, 1.0, 1.0, None)
        .await
        .unwrap();
    let role = store::auth::ensure_role(&store.pool, "viewer")
        .await
        .unwrap();
    let rules = [
        (
            nucleus::new_uid("v"),
            "public",
            None,
            target.clone(),
            None,
            "visible",
        ),
        (
            nucleus::new_uid("v"),
            "actor",
            Some(person.clone()),
            concept.clone(),
            Some("head"),
            "hidden",
        ),
        (
            nucleus::new_uid("v"),
            "organ",
            Some(organ.clone()),
            place.clone(),
            None,
            "visible",
        ),
        (
            nucleus::new_uid("v"),
            "role",
            Some(role.to_string()),
            target.clone(),
            None,
            "visible",
        ),
        (
            nucleus::new_uid("v"),
            "fiote",
            Some("future-subject".into()),
            target.clone(),
            Some("body"),
            "hidden",
        ),
    ];
    for (uid, subject_kind, subject_uid, target_uid, field, grant) in &rules {
        store::sqlx::query(
            "INSERT INTO visibility_rule
                (uid, subject_kind, subject_uid, target_uid, field, grant_level)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(uid)
        .bind(subject_kind)
        .bind(subject_uid)
        .bind(target_uid)
        .bind(field)
        .bind(grant)
        .execute(&store.pool)
        .await
        .unwrap();
    }

    let snapshot = metadata(&store).await;
    assert!(snapshot.role_ids.contains(&role));
    assert!(snapshot.visibility_rules.iter().any(|rule| {
        rule.subject == VisibilitySubject::Public
            && rule.target == VisibilityTarget::Record(target.clone())
            && rule.grant == VisibilityGrant::Visible
    }));
    assert!(snapshot.visibility_rules.iter().any(|rule| {
        rule.subject == VisibilitySubject::Actor(person.clone())
            && rule.target == VisibilityTarget::Concept(concept.clone())
            && rule.field.as_deref() == Some("head")
            && rule.grant == VisibilityGrant::Hidden
    }));
    assert!(snapshot.visibility_rules.iter().any(|rule| {
        rule.subject == VisibilitySubject::Organ(organ.clone())
            && rule.target == VisibilityTarget::Place(place.clone())
    }));
    assert!(
        snapshot
            .visibility_rules
            .iter()
            .any(|rule| rule.subject == VisibilitySubject::Role(role))
    );
    assert!(snapshot.visibility_rules.iter().any(|rule| {
        rule.subject
            == VisibilitySubject::Unsupported {
                kind: "fiote".into(),
                uid: Some("future-subject".into()),
            }
            && rule.field.as_deref() == Some("body")
            && rule.grant == VisibilityGrant::Hidden
    }));
}

#[tokio::test]
async fn message_draft_ownership_accepts_person_and_organ_without_a_conversation() {
    let store = Store::open_memory().await.unwrap();
    let author = record(
        &store,
        RecordKind::Person,
        "Author",
        "",
        store::exact::zero(),
    )
    .await;
    let operator = local_organ(&store).await;
    let thread = record(
        &store,
        RecordKind::Thread,
        "Task discussion",
        "",
        store::exact::zero(),
    )
    .await;
    let draft = record(
        &store,
        RecordKind::MessageDraft,
        "Draft",
        "private",
        store::exact::zero(),
    )
    .await;
    let stored = json!({
        "author": author,
        "operator": operator,
        "thread": thread,
        "pinned": false,
        "timing": "now"
    });
    store::records::set_extension(&store.pool, &draft, "lince.message-draft", &stored)
        .await
        .unwrap();

    let snapshot = metadata(&store).await;
    let ownership = snapshot
        .message_drafts
        .iter()
        .find(|row| row.record_uid == draft)
        .unwrap();
    assert_eq!(ownership.author_uid, stored["author"].as_str().unwrap());
    assert_eq!(ownership.operator_uid, stored["operator"].as_str().unwrap());
    assert_eq!(ownership.thread_uid.as_deref(), stored["thread"].as_str());
    assert_eq!(ownership.metadata, stored.as_object().unwrap().clone());
}

#[tokio::test]
async fn missing_targets_duplicate_targets_and_invalid_stored_shapes_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        RecordKind::Plain,
        "Target",
        "",
        store::exact::zero(),
    )
    .await;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            &[target.clone(), target.clone()],
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            &[nucleus::new_uid("r")],
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();

    store::sqlx::query("UPDATE record SET kind = 'unknown-kind' WHERE uid = ?")
        .bind(&target)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn corrupt_extensions_draft_identities_and_visibility_references_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let author = record(
        &store,
        RecordKind::Person,
        "Author",
        "",
        store::exact::zero(),
    )
    .await;
    let draft = record(
        &store,
        RecordKind::MessageDraft,
        "Draft",
        "",
        store::exact::zero(),
    )
    .await;
    store::records::set_extension(
        &store.pool,
        &draft,
        "lince.message-draft",
        &json!({"author": author, "operator": nucleus::new_uid("r")}),
    )
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    let operator = local_organ(&store).await;
    store::records::set_extension(
        &store.pool,
        &draft,
        "lince.message-draft",
        &json!({"author": author, "operator": operator}),
    )
    .await
    .unwrap();

    let ordinary = record(
        &store,
        RecordKind::Plain,
        "Extension",
        "",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, version, fds)
         VALUES (?, 'corrupt', 1, '[')",
    )
    .bind(&ordinary)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            std::slice::from_ref(&ordinary),
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();

    store::sqlx::query(
        "INSERT INTO visibility_rule
            (uid, subject_kind, subject_uid, target_uid, grant_level)
         VALUES (?, 'role', '01', ?, 'visible')",
    )
    .bind(nucleus::new_uid("v"))
    .bind(&ordinary)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn every_row_count_and_byte_limit_refuses_without_partial_output() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        RecordKind::Plain,
        "Bounded",
        "payload",
        store::exact::zero(),
    )
    .await;
    store::records::set_extension(&store.pool, &target, "one", &json!({"value": 1}))
        .await
        .unwrap();
    let parent = store::concepts::create(&store.pool, "Limit parent", &[])
        .await
        .unwrap();
    let child = store::concepts::create(&store.pool, "Limit child", &[&parent])
        .await
        .unwrap();
    let assertion = store::assertions::assert(
        &store.pool,
        store::assertions::NewAssertion {
            subject_uid: &target,
            predicate_uid: &child,
            object_uid: None,
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    assert!(!assertion.is_empty());
    let place = store::places::create(&store.pool, 0.0, 0.0, None)
        .await
        .unwrap();
    assert!(!place.is_empty());
    let role = store::auth::ensure_role(&store.pool, "limit role")
        .await
        .unwrap();
    store::sqlx::query(
        "INSERT INTO visibility_rule
            (uid, subject_kind, subject_uid, target_uid, grant_level)
         VALUES (?, 'role', ?, ?, 'visible')",
    )
    .bind(nucleus::new_uid("v"))
    .bind(role.to_string())
    .bind(&target)
    .execute(&store.pool)
    .await
    .unwrap();
    let lingua = store::linguas::create(&store.pool, "Limit Lingua", None, "private")
        .await
        .unwrap();
    store::linguas::adopt(&store.pool, &lingua, &child)
        .await
        .unwrap();
    let author = record(
        &store,
        RecordKind::Person,
        "Limit author",
        "",
        store::exact::zero(),
    )
    .await;
    let draft = record(
        &store,
        RecordKind::MessageDraft,
        "Limit draft",
        "",
        store::exact::zero(),
    )
    .await;
    store::records::set_extension(
        &store.pool,
        &draft,
        "lince.message-draft",
        &json!({"author": author, "operator": author}),
    )
    .await
    .unwrap();

    let mut cases = Vec::new();
    let mut limits = AccessSnapshotLimits::default();
    limits.records = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.concepts = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.concept_parents = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.assertions = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.places = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.roles = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.visibility_rules = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.linguas = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.lingua_concepts = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.message_drafts = 0;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.row_bytes = 1;
    cases.push(limits);
    let mut limits = AccessSnapshotLimits::default();
    limits.metadata_bytes = 1;
    cases.push(limits);
    for limits in cases {
        let mut tx = store.pool.begin().await.unwrap();
        assert!(
            store::access_snapshot::metadata_on(&mut tx, &limits)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }

    let mut limits = AccessSnapshotLimits::default();
    limits.targets = 0;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&target), &limits)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut limits = AccessSnapshotLimits::default();
    limits.extensions = 0;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&target), &limits)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut limits = AccessSnapshotLimits::default();
    limits.row_bytes = 1;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&target), &limits)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let mut limits = AccessSnapshotLimits::default();
    limits.content_bytes = 1;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&target), &limits)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn caller_transaction_keeps_one_snapshot_and_rollback_never_leaks_rows() {
    let dir = std::env::temp_dir().join(nucleus::new_uid("access-snapshot"));
    std::fs::create_dir_all(&dir).unwrap();
    let url = format!("sqlite://{}", dir.join("lince.db").display());
    let store = Arc::new(Store::open(&url).await.unwrap());
    let initial = record(
        &store,
        RecordKind::Plain,
        "Initial",
        "",
        store::exact::zero(),
    )
    .await;

    let mut reader = store.pool.begin().await.unwrap();
    let before = store::access_snapshot::metadata_on(&mut reader, &AccessSnapshotLimits::default())
        .await
        .unwrap();
    let second_store = Arc::clone(&store);
    let inserted = tokio::spawn(async move {
        record(
            &second_store,
            RecordKind::Plain,
            "Concurrent",
            "",
            store::exact::zero(),
        )
        .await
    })
    .await
    .unwrap();
    let still_before =
        store::access_snapshot::metadata_on(&mut reader, &AccessSnapshotLimits::default())
            .await
            .unwrap();
    assert_eq!(before, still_before);
    assert!(before.records.iter().any(|row| row.uid == initial));
    assert!(!before.records.iter().any(|row| row.uid == inserted));
    reader.rollback().await.unwrap();

    let after = metadata(&store).await;
    assert!(after.records.iter().any(|row| row.uid == inserted));

    let rolled_back = nucleus::new_uid("r");
    let organ = local_organ(&store).await;
    let now = chrono::Utc::now().to_rfc3339();
    let mut writer = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query(
        "INSERT INTO record
            (uid, kind, head, body, quantity_mantissa, quantity_scale,
             organ_uid, created_at, updated_at)
         VALUES (?, 'plain', 'Rolled back', '', '0', 0, ?, ?, ?)",
    )
    .bind(&rolled_back)
    .bind(&organ)
    .bind(&now)
    .bind(&now)
    .execute(&mut *writer)
    .await
    .unwrap();
    let inside = store::access_snapshot::metadata_on(&mut writer, &AccessSnapshotLimits::default())
        .await
        .unwrap();
    assert!(inside.records.iter().any(|row| row.uid == rolled_back));
    writer.rollback().await.unwrap();
    let after_rollback = metadata(&store).await;
    assert!(
        !after_rollback
            .records
            .iter()
            .any(|row| row.uid == rolled_back)
    );

    store.pool.close().await;
    std::fs::remove_dir_all(dir).unwrap();
}

#[tokio::test]
async fn missing_roots_wrong_identity_kinds_and_non_object_extensions_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        RecordKind::Plain,
        "Target",
        "",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query("UPDATE record SET replica_root = ? WHERE uid = ?")
        .bind(nucleus::new_uid("r"))
        .bind(&target)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    store::sqlx::query("UPDATE record SET replica_root = NULL WHERE uid = ?")
        .bind(&target)
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&store.pool)
        .await
        .unwrap();
    store::sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, version, fds)
         VALUES (?, 'array', 1, '[]')",
    )
    .bind(&target)
    .execute(&store.pool)
    .await
    .unwrap();
    store::sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            std::slice::from_ref(&target),
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();

    let invalid = nucleus::new_uid("r");
    let organ = local_organ(&store).await;
    let now = chrono::Utc::now().to_rfc3339();
    store::sqlx::query(
        "INSERT INTO record
            (uid, kind, head, body, quantity_mantissa, quantity_scale,
             organ_uid, created_at, updated_at)
         VALUES (?, 'plain', '', '', '0', 0, ?, ?, ?)",
    )
    .bind(&invalid)
    .bind(&target)
    .bind(&now)
    .bind(&now)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert_ne!(organ, target);
}

#[tokio::test]
async fn invalid_exact_decimals_and_invalid_record_identities_are_refused() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        RecordKind::Plain,
        "Target",
        "",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query("UPDATE record SET quantity_mantissa = '01' WHERE uid = ?")
        .bind(&target)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            std::slice::from_ref(&target),
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();

    let organ = local_organ(&store).await;
    let now = chrono::Utc::now().to_rfc3339();
    store::sqlx::query(
        "INSERT INTO record
            (uid, kind, head, body, quantity_mantissa, quantity_scale,
             organ_uid, created_at, updated_at)
         VALUES ('bad-record', 'plain', '', '', '0', 0, ?, ?, ?)",
    )
    .bind(&organ)
    .bind(&now)
    .bind(&now)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn non_strict_blob_record_values_return_errors_instead_of_panicking() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(
        &store,
        RecordKind::Plain,
        "Typed",
        "body",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query("UPDATE record SET kind = X'80' WHERE uid = ?")
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    store::sqlx::query("UPDATE record SET kind = 'plain', head = X'80' WHERE uid = ?")
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            std::slice::from_ref(&uid),
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn non_strict_real_integer_fields_return_errors_instead_of_panicking() {
    let store = Store::open_memory().await.unwrap();
    let subject = record(
        &store,
        RecordKind::Plain,
        "Subject",
        "",
        store::exact::zero(),
    )
    .await;
    let predicate = store::concepts::create(&store.pool, "typed assertion", &[])
        .await
        .unwrap();
    store::sqlx::query(
        "INSERT INTO record_assertion
            (uid, subject_uid, predicate_uid, role, quantity_mantissa,
             quantity_scale, created_at)
         VALUES (?, ?, ?, 'ordinary', '1', 1.5, ?)",
    )
    .bind(nucleus::new_uid("a"))
    .bind(&subject)
    .bind(&predicate)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();

    store::sqlx::query("DELETE FROM record_assertion WHERE subject_uid = ?")
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();
    store::records::set_extension(&store.pool, &subject, "typed", &json!({"ok": true}))
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_extension SET version = 1.5 WHERE record_uid = ?")
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(
            &mut tx,
            std::slice::from_ref(&subject),
            &AccessSnapshotLimits::default(),
        )
        .await
        .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn large_blob_integer_columns_are_refused_before_payload_reads() {
    let store = Store::open_memory().await.unwrap();
    let subject = record(
        &store,
        RecordKind::Plain,
        "Blob types",
        "",
        store::exact::zero(),
    )
    .await;
    let predicate = store::concepts::create(&store.pool, "blob assertion", &[])
        .await
        .unwrap();
    let oversized = 2 * 1024 * 1024_i64;
    let limits = AccessSnapshotLimits {
        row_bytes: 512,
        ..AccessSnapshotLimits::default()
    };

    store::sqlx::query("UPDATE record SET quantity_scale = zeroblob(?) WHERE uid = ?")
        .bind(oversized)
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&subject), &limits)
            .await
            .unwrap_err()
            .to_string()
            .contains("storage type")
    );
    tx.rollback().await.unwrap();
    store::sqlx::query("UPDATE record SET quantity_scale = 0 WHERE uid = ?")
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();

    store::sqlx::query(
        "INSERT INTO record_assertion
            (uid, subject_uid, predicate_uid, role, quantity_mantissa,
             quantity_scale, created_at)
         VALUES (?, ?, ?, 'ordinary', '1', zeroblob(?), ?)",
    )
    .bind(nucleus::new_uid("a"))
    .bind(&subject)
    .bind(&predicate)
    .bind(oversized)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &limits)
            .await
            .unwrap_err()
            .to_string()
            .contains("storage type")
    );
    tx.rollback().await.unwrap();
    store::sqlx::query("DELETE FROM record_assertion WHERE subject_uid = ?")
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();

    store::records::set_extension(&store.pool, &subject, "blob", &json!({"ok": true}))
        .await
        .unwrap();
    store::sqlx::query("UPDATE record_extension SET version = zeroblob(?) WHERE record_uid = ?")
        .bind(oversized)
        .bind(&subject)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, std::slice::from_ref(&subject), &limits)
            .await
            .unwrap_err()
            .to_string()
            .contains("storage type")
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn null_known_visibility_identities_return_errors_instead_of_panicking() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        RecordKind::Plain,
        "Target",
        "",
        store::exact::zero(),
    )
    .await;
    store::sqlx::query(
        "INSERT INTO visibility_rule
            (uid, subject_kind, subject_uid, target_uid, grant_level)
         VALUES (?, 'actor', NULL, ?, 'visible')",
    )
    .bind(nucleus::new_uid("v"))
    .bind(&target)
    .execute(&store.pool)
    .await
    .unwrap();
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::metadata_on(&mut tx, &AccessSnapshotLimits::default())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn exact_content_byte_boundaries_are_inclusive_and_cumulative() {
    let store = Store::open_memory().await.unwrap();
    let first = record(
        &store,
        RecordKind::Plain,
        "First",
        &"a".repeat(128),
        store::exact::zero(),
    )
    .await;
    let second = record(
        &store,
        RecordKind::Plain,
        "Second",
        &"b".repeat(96),
        store::exact::zero(),
    )
    .await;
    store::records::set_extension(&store.pool, &first, "edge", &json!({"value": "x"}))
        .await
        .unwrap();
    let record_bytes = store::sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
              + COALESCE(length(CAST(kind AS BLOB)), 0)
              + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
              + COALESCE(length(CAST(replica_root AS BLOB)), 0)
              + COALESCE(length(CAST(slug AS BLOB)), 0)
              + COALESCE(length(CAST(head AS BLOB)), 0)
              + COALESCE(length(CAST(body AS BLOB)), 0)
              + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
              + COALESCE(length(CAST(unit_uid AS BLOB)), 0)
              + COALESCE(length(CAST(place_uid AS BLOB)), 0) + 16
           FROM record WHERE uid = ?",
    )
    .bind(&first)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let second_bytes = store::sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(length(CAST(uid AS BLOB)), 0)
              + COALESCE(length(CAST(kind AS BLOB)), 0)
              + COALESCE(length(CAST(organ_uid AS BLOB)), 0)
              + COALESCE(length(CAST(replica_root AS BLOB)), 0)
              + COALESCE(length(CAST(slug AS BLOB)), 0)
              + COALESCE(length(CAST(head AS BLOB)), 0)
              + COALESCE(length(CAST(body AS BLOB)), 0)
              + COALESCE(length(CAST(quantity_mantissa AS BLOB)), 0)
              + COALESCE(length(CAST(unit_uid AS BLOB)), 0)
              + COALESCE(length(CAST(place_uid AS BLOB)), 0) + 16
           FROM record WHERE uid = ?",
    )
    .bind(&second)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let extension_bytes = store::sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(length(CAST(namespace AS BLOB)), 0)
              + COALESCE(length(CAST(fds AS BLOB)), 0) + 8
           FROM record_extension WHERE record_uid = ?",
    )
    .bind(&first)
    .fetch_one(&store.pool)
    .await
    .unwrap();
    let total = usize::try_from(record_bytes + second_bytes + extension_bytes).unwrap();
    let largest = usize::try_from(record_bytes.max(second_bytes).max(extension_bytes)).unwrap();
    let targets = vec![first.clone(), second.clone()];
    let limits = AccessSnapshotLimits {
        row_bytes: largest,
        content_bytes: total,
        ..AccessSnapshotLimits::default()
    };
    let mut tx = store.pool.begin().await.unwrap();
    let content = store::access_snapshot::content_on(&mut tx, &targets, &limits)
        .await
        .unwrap();
    assert_eq!(content.len(), 2);
    tx.rollback().await.unwrap();

    let mut below = limits.clone();
    below.content_bytes = total - 1;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, &targets, &below)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    below = limits;
    below.row_bytes = largest - 1;
    let mut tx = store.pool.begin().await.unwrap();
    assert!(
        store::access_snapshot::content_on(&mut tx, &targets, &below)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}

#[test]
fn default_limits_fit_the_authority_evaluator() {
    let limits = AccessSnapshotLimits::default();
    assert_eq!(limits.records, 4096);
    assert_eq!(limits.concepts, 4096);
    assert_eq!(limits.places, 4096);
    assert_eq!(limits.assertions, 32768);
    assert_eq!(limits.metadata_bytes, 16 * 1024 * 1024);
    assert_eq!(limits.content_bytes, 16 * 1024 * 1024);
    assert_eq!(BTreeSet::from([limits.targets]).len(), 1);
}
