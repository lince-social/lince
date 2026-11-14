use nucleus::RecordKind;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use store::{Store, assertions, records, sqlx};

static BATCHES: AtomicU64 = AtomicU64::new(0);

fn values(rows: &[records::RecordRow]) -> Vec<Value> {
    rows.iter()
        .map(|row| {
            json!({
                "uid": row.uid, "slug": row.slug, "kind": row.kind, "head": row.head,
                "body": row.body, "quantity": row.quantity.to_string(),
                "identity": row.identity_predicate_uid, "unit": row.unit_uid,
                "place": row.place_uid, "organ": row.organ_uid,
                "created": row.created_at, "updated": row.updated_at, "hlc": row.created_hlc,
            })
        })
        .collect()
}

async fn record(store: &Store, index: usize) -> String {
    records::create(
        &store.pool,
        records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: &format!("Related subject {index} · 日本語"),
            body: &"Exact retained description. ".repeat(index + 1),
            quantity: nucleus::DecimalValue::parse_inferred("2.50").unwrap(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn compare(store: &Store, predicate: &str, roots: &[String]) {
    let mut requested = roots.to_vec();
    requested.extend((0..1100).map(|index| format!("missing-{index}")));
    requested.extend(roots.iter().cloned());
    requested.push("' OR 1=1 -- 日本語".into());
    let requested: Vec<_> = requested.iter().map(String::as_str).collect();
    BATCHES.store(0, Ordering::Relaxed);
    let mut grouped = assertions::subjects_pointing_to_many(&store.pool, predicate, &requested)
        .await
        .unwrap();
    assert_eq!(BATCHES.load(Ordering::Relaxed), 1);
    for root in roots {
        let expected = assertions::subjects_pointing_to(&store.pool, predicate, root)
            .await
            .unwrap();
        assert_eq!(
            values(&grouped.remove(root).unwrap_or_default()),
            values(&expected)
        );
    }
    assert!(grouped.is_empty());
}

#[tokio::test]
async fn grouped_subjects_match_individual_reads_with_order_aliases_duplicates_and_changes() {
    assert!(nucleus::performance::install(|reading| {
        if let nucleus::performance::Reading::Count("record-related-subject-batches", count) =
            reading
        {
            BATCHES.fetch_add(count, Ordering::Relaxed);
        }
    }));
    let store = Store::open_memory().await.unwrap();
    let binding = store::vocabulary::feature_binding(&store.pool, "reference")
        .await
        .unwrap();
    let predicate = binding.write_concept_uid.clone();
    let alias = store::concepts::create(&store.pool, "another-reference", &[])
        .await
        .unwrap();
    let unrelated = store::concepts::create(&store.pool, "unrelated-subject", &[])
        .await
        .unwrap();
    let mut accepted = binding.clone();
    accepted.concept_uids.push(alias.clone());
    let mut transaction = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::bind_feature_on(&mut transaction, &accepted, binding.revision)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    let mut roots = Vec::new();
    let mut subjects = Vec::new();
    for index in 0..6 {
        roots.push(record(&store, index).await);
    }
    for index in 6..16 {
        subjects.push(record(&store, index).await);
    }
    let mut edges = Vec::new();
    for (index, subject) in subjects.iter().enumerate() {
        for (kind, root) in [
            (&predicate, index % roots.len()),
            (&alias, (index + 1) % roots.len()),
            (&alias, index % roots.len()),
            (&unrelated, (index + 2) % roots.len()),
        ] {
            let uid = assertions::assert(
                &store.pool,
                assertions::NewAssertion {
                    subject_uid: subject,
                    predicate_uid: kind,
                    object_uid: Some(&roots[root]),
                    role: assertions::AssertionRole::Ordinary,
                    quantity: None,
                    unit_uid: None,
                    asserted_by: None,
                },
            )
            .await
            .unwrap();
            sqlx::query("UPDATE record_assertion SET created_at=? WHERE uid=?")
                .bind(format!("2026-01-01T00:00:{:02}Z", 59 - edges.len()))
                .bind(&uid)
                .execute(&store.pool)
                .await
                .unwrap();
            edges.push(uid);
        }
    }
    for selector in [&predicate[..], "feature:reference", "missing-predicate"] {
        compare(&store, selector, &roots).await;
    }
    assertions::retract(&store.pool, &edges[1], None)
        .await
        .unwrap();
    records::mark_deleted(&store.pool, &subjects[3])
        .await
        .unwrap();
    compare(&store, "feature:reference", &roots).await;
    let current = store::vocabulary::feature_binding(&store.pool, "reference")
        .await
        .unwrap();
    let mut transaction = store::write_tx(&store.pool).await.unwrap();
    store::vocabulary::bind_feature_on(&mut transaction, &binding, current.revision)
        .await
        .unwrap();
    transaction.commit().await.unwrap();
    compare(&store, "feature:reference", &roots).await;
    BATCHES.store(0, Ordering::Relaxed);
    assert!(
        assertions::subjects_pointing_to_many(&store.pool, &predicate, &[])
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(BATCHES.load(Ordering::Relaxed), 0);
}
