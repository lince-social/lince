use nucleus::trust_analysis::{Identity, query::Query};
use store::{Store, trust_queries as queries};

async fn fixture() -> (Store, String, Query) {
    let store = Store::open_memory().await.unwrap();
    let organ = store::organs::ensure_local(&store.pool, "http://query.test")
        .await
        .unwrap();
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Person,
            head: "Reader",
            body: "",
            quantity: store::exact::integer(1),
        },
    )
    .await
    .unwrap()
    .uid;
    (
        store,
        person,
        Query::for_subject(Identity::Organ(organ.uid)),
    )
}

fn input(person: &str, query: &Query, id: &str) -> queries::Input {
    queries::Input {
        record: nucleus::new_uid("r"),
        person: person.into(),
        request_id: id.into(),
        name: "Food contributions".into(),
        query: query.clone(),
        authorization_intent: None,
    }
}

async fn save(store: &Store, input: queries::Input) -> queries::Commit {
    let (saved, tx) = queries::save(
        store::write_tx(&store.pool).await.unwrap(),
        input,
        nucleus::execution::now(),
        |_| Some("fixture-audit".into()),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    saved
}

async fn counts(store: &Store) -> (i64, i64, i64, i64) {
    store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM record),(SELECT COUNT(*) FROM fact),(SELECT COUNT(*) FROM trust_query_record),(SELECT COUNT(*) FROM trust_query_request)").fetch_one(&store.pool).await.unwrap()
}

#[tokio::test]
async fn reader_queries_are_private_zero_quantity_and_idempotent() {
    let (store, person, query) = fixture().await;
    let first = save(&store, input(&person, &query, "query:first")).await;
    assert!(first.created);
    assert!(first.saved.fact.delta.is_zero());
    assert_eq!(first.saved.fact.actor_uid.as_deref(), Some(person.as_str()));
    let policy = store::data_visibility::policy(
        &store.pool,
        &first.saved.record,
        nucleus::visibility::Data::Record,
    )
    .await
    .unwrap()
    .unwrap();
    let organ = store::organs::local(&store.pool).await.unwrap().unwrap();
    assert!(
        policy
            .policy
            .decide_principal(&organ.uid, Some(&person), Some(0))
            .allowed
    );
    assert!(
        !policy
            .policy
            .decide_principal(&organ.uid, None, Some(0))
            .allowed
    );
    assert!(
        !policy
            .policy
            .decide_principal(&organ.uid, Some(&nucleus::new_uid("r")), Some(0))
            .allowed
    );
    let before = counts(&store).await;
    let repeated = save(&store, input(&person, &query, "query:first")).await;
    assert!(!repeated.created);
    assert_eq!(first.saved.record, repeated.saved.record);
    assert_eq!(counts(&store).await, before);
    let duplicate = save(&store, input(&person, &query, "query:duplicate")).await;
    assert!(!duplicate.created);
    assert_eq!(duplicate.saved.record, first.saved.record);
    let after = counts(&store).await;
    assert_eq!((before.0, before.1, before.2), (after.0, after.1, after.2));
    assert_eq!(after.3, before.3 + 1);
    let loaded = queries::get_on(
        &mut *store.pool.acquire().await.unwrap(),
        &first.saved.record,
    )
    .await
    .unwrap();
    assert_eq!(loaded.query, query);
}

#[tokio::test]
async fn changed_requests_and_unauthorized_saves_roll_back_without_effects() {
    let (store, person, query) = fixture().await;
    save(&store, input(&person, &query, "query:first")).await;
    let before = counts(&store).await;
    let mut changed = input(&person, &query, "query:first");
    changed.query.weights.giver = nucleus::DecimalValue::parse_inferred("0.2").unwrap();
    assert!(
        queries::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed,
            nucleus::execution::now(),
            |_| Some("fixture-audit".into())
        )
        .await
        .is_err()
    );
    let mut changed = input(&person, &query, "query:first");
    changed.name = "Changed request".into();
    assert!(
        queries::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed,
            nucleus::execution::now(),
            |_| Some("fixture-audit".into())
        )
        .await
        .is_err()
    );
    let mut changed = input(&person, &query, "query:unsigned");
    changed.query.weights.giver = nucleus::DecimalValue::parse_inferred("0.3").unwrap();
    assert!(
        queries::save(
            store::write_tx(&store.pool).await.unwrap(),
            changed,
            nucleus::execution::now(),
            |_| None
        )
        .await
        .is_err()
    );
    assert_eq!(counts(&store).await, before);
}

#[tokio::test]
async fn original_reader_policies_cannot_be_replaced_through_generic_extensions_or_sql() {
    let (store, person, query) = fixture().await;
    let saved = save(&store, input(&person, &query, "query:first"))
        .await
        .saved;
    let mut changed = query.clone();
    changed.weights.giver = nucleus::DecimalValue::parse_inferred("99").unwrap();
    assert!(
        store::records::set_extension(
            &store.pool,
            &saved.record,
            nucleus::trust_analysis::query::NAMESPACE,
            &serde_json::to_value(changed).unwrap()
        )
        .await
        .is_err()
    );
    assert!(
        store::records::delete_extension(
            &store.pool,
            &saved.record,
            nucleus::trust_analysis::query::NAMESPACE
        )
        .await
        .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE trust_query_record SET policy_json='{}' WHERE record_uid=?")
            .bind(&saved.record)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert!(
        store::sqlx::query("UPDATE trust_query_request SET request_json='{}' WHERE record_uid=?")
            .bind(&saved.record)
            .execute(&store.pool)
            .await
            .is_err()
    );
    assert_eq!(
        queries::get_on(&mut *store.pool.acquire().await.unwrap(), &saved.record)
            .await
            .unwrap()
            .query,
        query
    );
}
