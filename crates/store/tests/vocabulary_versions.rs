use nucleus::RecordKind;
use store::{
    Store,
    assertions::{AssertionRole, NewAssertion},
    records::NewRecord,
};

async fn record(store: &Store, kind: RecordKind, head: &str) -> String {
    store::records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn revise(store: &Store, concept: &str, revision: i64, definition: &str) {
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::revise_definition_on(&mut tx, concept, revision, definition)
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn assertions_pin_definitions_and_units_through_edits_and_quantity_changes() {
    let store = Store::open_memory().await.unwrap();
    let concept = store::concepts::create(&store.pool, "food", &[])
        .await
        .unwrap();
    let unit = store::concepts::create(&store.pool, "portion", &[])
        .await
        .unwrap();
    revise(&store, &concept, 1, "Food suitable for eating").await;
    let subject = record(&store, RecordKind::Plain, "Lunch").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let assertion = store::assertions::insert_tx(
        &mut tx,
        &nucleus::new_uid("a"),
        NewAssertion {
            subject_uid: &subject,
            predicate_uid: &concept,
            object_uid: None,
            role: AssertionRole::Ordinary,
            quantity: Some(store::exact::zero()),
            unit_uid: Some(&unit),
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (assertion.predicate_revision, assertion.unit_revision),
        (2, Some(1))
    );
    revise(&store, &concept, 2, "Food and beverages").await;
    revise(&store, &unit, 1, "One measured serving").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let updated = store::assertions::set_quantity_tx(
        &mut tx,
        &assertion.uid,
        store::assertions::AssertionQuantity {
            quantity: Some(store::exact::from_f64(3.0)),
            unit_uid: Some(&unit),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(
        (updated.predicate_revision, updated.unit_revision),
        (2, Some(1))
    );
    assert!(
        store::sqlx::query("UPDATE record_assertion SET predicate_revision=3 WHERE uid=?")
            .bind(&assertion.uid)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE record_assertion SET unit_revision=2 WHERE uid=?")
            .bind(&assertion.uid)
            .execute(&store.pool)
            .await
            .is_err()
    );
    let another = store::concepts::create(&store.pool, "plate", &[])
        .await
        .unwrap();
    revise(&store, &another, 1, "One plate").await;
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    let changed = store::assertions::set_quantity_tx(
        &mut tx,
        &assertion.uid,
        store::assertions::AssertionQuantity {
            quantity: updated.quantity,
            unit_uid: Some(&another),
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(changed.unit_revision, Some(2));
    let raw: String=store::sqlx::query_scalar("SELECT value FROM sync_op WHERE tbl='record_assertion' AND uid=? AND kind='set' ORDER BY seq DESC LIMIT 1")
        .bind(&assertion.uid).fetch_one(&store.pool).await.unwrap();
    let wire: serde_json::Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(wire["predicate_revision"], 2);
    assert_eq!(wire["unit_revision"], 2);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    assert!(
        store::vocabulary::revise_definition_on(&mut tx, &concept, 2, "Stale edit")
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(store::sqlx::query("UPDATE concept_definition_version SET definition='changed' WHERE concept_uid=? AND revision=2").bind(&concept).execute(&store.pool).await.is_err());
}

#[tokio::test]
async fn record_history_preserves_content_and_paginates_without_recording_noops() {
    let store = Store::open_memory().await.unwrap();
    let uid = record(&store, RecordKind::Plain, "Original").await;
    for index in 0..35 {
        store::sqlx::query("UPDATE record SET head=?,body=? WHERE uid=?")
            .bind(format!("Title {index}"))
            .bind(format!("Body {index}"))
            .bind(&uid)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    store::sqlx::query("UPDATE record SET updated_at='later',head=head WHERE uid=?")
        .bind(&uid)
        .execute(&store.pool)
        .await
        .unwrap();
    let mut connection = store.pool.acquire().await.unwrap();
    let first = store::vocabulary::record_history_on(&mut connection, &uid, 0)
        .await
        .unwrap();
    let last = store::vocabulary::record_history_on(&mut connection, &uid, 30)
        .await
        .unwrap();
    assert_eq!(first["count"], 36);
    assert_eq!(first["versions"].as_array().unwrap().len(), 30);
    assert_eq!(last["versions"].as_array().unwrap().len(), 6);
    assert_eq!(first["versions"][0]["content"]["body"], "Body 34");
    assert_eq!(last["versions"][5]["content"]["head"], "Original");
    drop(connection);
    assert!(
        store::sqlx::query("DELETE FROM record_content_version WHERE record_uid=?")
            .bind(&uid)
            .execute(&store.pool)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn publications_are_portable_private_terms_stay_private_and_withdrawal_keeps_adoptions() {
    let source = Store::open_memory().await.unwrap();
    let source_organ = store::organs::local(&source.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut tx = store::write_tx(&source.pool).await.unwrap();
    let vocabulary = store::vocabulary::create_vocabulary_on(&mut tx, "Pantry", &source_organ)
        .await
        .unwrap();
    let second = store::vocabulary::create_vocabulary_on(&mut tx, "Garden", &source_organ)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_ne!(vocabulary, second);
    let concept = store::concepts::create_in(&source.pool, &vocabulary, "rice", &[])
        .await
        .unwrap();
    revise(&source, &concept, 1, "Edible rice grains").await;
    let person = record(&source, RecordKind::Person, "Owner").await;
    let mut tx = store::write_tx(&source.pool).await.unwrap();
    store::vocabulary::add_term_on(
        &mut tx,
        &concept,
        "private-rice-nickname",
        "",
        Some(&person),
        true,
    )
    .await
    .unwrap();
    let revision: i64 = store::sqlx::query_scalar("SELECT revision FROM lingua WHERE uid=?")
        .bind(&vocabulary)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let package = store::vocabulary::publish_on(&mut tx, &vocabulary, revision)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let raw = serde_json::to_string(&package).unwrap();
    assert!(!raw.contains("private-rice-nickname"));
    let destination = Store::open_memory().await.unwrap();
    let local = store::organs::local(&destination.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let team = record(&destination, RecordKind::Organ, "Team").await;
    for organ in [&local, &team] {
        let mut tx = store::write_tx(&destination.pool).await.unwrap();
        store::vocabulary::adopt_on(&mut tx, &package, organ)
            .await
            .unwrap();
        store::vocabulary::adopt_on(&mut tx, &package, organ)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let mut tx = store::write_tx(&source.pool).await.unwrap();
    let current_revision: i64 =
        store::sqlx::query_scalar("SELECT revision FROM lingua WHERE uid=?")
            .bind(&vocabulary)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    let newer = store::vocabulary::publish_on(&mut tx, &vocabulary, current_revision)
        .await
        .unwrap();
    store::vocabulary::withdraw_on(&mut tx, &package.uid)
        .await
        .unwrap();
    let visibility: String = store::sqlx::query_scalar("SELECT visibility FROM lingua WHERE uid=?")
        .bind(&vocabulary)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(visibility, "public");
    store::vocabulary::withdraw_on(&mut tx, &newer.uid)
        .await
        .unwrap();
    assert!(
        store::vocabulary::publication_on(&mut tx, &package.uid, true)
            .await
            .is_err()
    );
    assert_eq!(
        store::vocabulary::publication_on(&mut tx, &package.uid, false)
            .await
            .unwrap(),
        package
    );
    tx.commit().await.unwrap();
    let definitions = store::vocabulary::definition_versions(&destination.pool)
        .await
        .unwrap();
    assert_eq!(definitions[&concept][1].definition, "Edible rice grains");
    assert!(
        store::concepts::delete(&source.pool, &concept)
            .await
            .is_err()
    );
    assert!(
        store::concepts::delete(&destination.pool, &concept)
            .await
            .is_err()
    );
    store::sync_apply::delete_concept(
        &destination.pool,
        &concept,
        store::sync_apply::Stamp {
            tbl: "concept",
            uid: &concept,
            field: "",
            hlc: nucleus::hlc::next(),
        },
    )
    .await
    .unwrap();
    assert!(
        store::vocabulary::sources(&destination.pool)
            .await
            .unwrap()
            .contains_key(&concept)
    );
    let count: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM organ_vocabulary WHERE vocabulary_uid=?")
            .bind(&vocabulary)
            .fetch_one(&destination.pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
    let mut bad = package.clone();
    bad.uid = nucleus::new_uid("vp");
    bad.dictionaries[0].definitions[1].definition = "Reinterpreted rice".into();
    let mut tx = store::write_tx(&destination.pool).await.unwrap();
    assert!(
        store::vocabulary::adopt_on(&mut tx, &bad, &local)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    let count: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM vocabulary_publication WHERE uid=?")
            .bind(&bad.uid)
            .fetch_one(&destination.pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    let mut tx = store::write_tx(&destination.pool).await.unwrap();
    assert!(
        store::vocabulary::require_local_concept_on(&mut tx, &concept)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn packages_cannot_claim_local_ownership_or_exceed_the_input_budget() {
    let source = Store::open_memory().await.unwrap();
    let organ = store::organs::local(&source.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    let mut tx = store::write_tx(&source.pool).await.unwrap();
    let vocabulary = store::vocabulary::create_vocabulary_on(&mut tx, "A collection", &organ)
        .await
        .unwrap();
    let package = store::vocabulary::publish_on(&mut tx, &vocabulary, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut forged = package.clone();
    forged.uid = nucleus::new_uid("vp");
    let mut tx = store::write_tx(&source.pool).await.unwrap();
    assert!(
        store::vocabulary::adopt_on(&mut tx, &forged, &organ)
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    assert!(
        store::vocabulary::read_package(&" ".repeat(store::vocabulary::PACKAGE_LIMIT + 1)).is_err()
    );
    forged.format = 99;
    assert!(store::vocabulary::read_package(&serde_json::to_string(&forged).unwrap()).is_err());
}

#[tokio::test]
async fn replication_preserves_selected_versions_when_the_receiver_has_newer_definitions() {
    let source = Store::open_memory().await.unwrap();
    let receiver = Store::open_memory().await.unwrap();
    let predicate = store::concepts::create(&source.pool, "served", &[])
        .await
        .unwrap();
    let unit = store::concepts::create(&source.pool, "serving", &[])
        .await
        .unwrap();
    revise(&source, &predicate, 1, "An initial meaning").await;
    revise(&source, &unit, 1, "An initial unit").await;
    let origin = store::organs::local(&source.pool)
        .await
        .unwrap()
        .unwrap()
        .uid;
    for concept in [&predicate, &unit] {
        let mut connection = source.pool.acquire().await.unwrap();
        let dictionary = store::vocabulary::dictionary_on(&mut connection, concept)
            .await
            .unwrap();
        store::vocabulary::apply_dictionary(&receiver.pool, &dictionary, &origin, false)
            .await
            .unwrap();
        revise(&receiver, concept, 2, "A newer local reading").await;
    }
    let subject = record(&receiver, RecordKind::Plain, "Received assertion").await;
    let uid = nucleus::new_uid("a");
    let stamp = store::sync_apply::Stamp {
        tbl: "record_assertion",
        uid: &uid,
        field: "",
        hlc: nucleus::hlc::next(),
    };
    let mut value = serde_json::json!({
        "subject_uid":subject,"predicate_uid":predicate,"predicate_revision":2,
        "unit_uid":unit,"unit_revision":1,"quantity_mantissa":"1","quantity_scale":0,
    });
    store::sync_apply::upsert_assertion(&receiver.pool, &uid, &value, stamp)
        .await
        .unwrap();
    let saved = store::assertions::get(&receiver.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (saved.predicate_revision, saved.unit_revision),
        (2, Some(1))
    );
    value["unit_revision"] = serde_json::json!(3);
    assert!(
        store::sync_apply::upsert_assertion(&receiver.pool, &uid, &value, stamp)
            .await
            .is_err()
    );
    value["unit_revision"] = serde_json::Value::Null;
    value["unit_uid"] = serde_json::Value::Null;
    store::sync_apply::upsert_assertion(&receiver.pool, &uid, &value, stamp)
        .await
        .unwrap();
    value["unit_uid"] = serde_json::json!(unit);
    value["unit_revision"] = serde_json::json!(1);
    store::sync_apply::upsert_assertion(&receiver.pool, &uid, &value, stamp)
        .await
        .unwrap();
    assert_eq!(
        store::assertions::get(&receiver.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .unit_revision,
        Some(1)
    );
}

#[tokio::test]
async fn joining_an_organ_rebinds_default_vocabularies_and_refuses_to_discard_owned_meanings() {
    let store = Store::open_memory().await.unwrap();
    let joined = nucleus::new_uid("r");
    store::organs::adopt_identity(&store.pool, &joined, "http://joined.test")
        .await
        .unwrap();
    let count: i64 =
        store::sqlx::query_scalar("SELECT COUNT(*) FROM organ_vocabulary WHERE organ_uid=?")
            .bind(&joined)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(count, 2);
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::create_vocabulary_on(&mut tx, "Personal meanings", &joined)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        store::organs::adopt_identity(&store.pool, &nucleus::new_uid("r"), "http://other.test")
            .await
            .is_err()
    );
    assert_eq!(
        store::organs::local(&store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid,
        joined
    );
}
