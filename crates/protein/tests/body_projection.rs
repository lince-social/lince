use nucleus::RecordKind;
use protein::{Aggregate, AggregateOp, GroupBy, Include, Order, Predicate, Protein, Source};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use store::records::{RecordBody, RecordRow, RecordSelection};

async fn record(store: &store::Store, slug: &str, head: &str, body: &str) -> RecordRow {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head,
            body,
            quantity: nucleus::DecimalValue::parse_inferred("9007199254740993.125").unwrap(),
        },
    )
    .await
    .unwrap()
}

fn metadata<Body>(row: &RecordRow<Body>) -> Value {
    json!({
        "uid": row.uid, "slug": row.slug, "kind": row.kind, "head": row.head,
        "quantity": row.quantity.to_string(), "identity": row.identity_predicate_uid,
        "unit": row.unit_uid, "place": row.place_uid, "organ": row.organ_uid,
        "created_at": row.created_at, "updated_at": row.updated_at, "created_hlc": row.created_hlc,
    })
}

fn query(uid: &str) -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(uid.into())],
        fields: Some(["uid", "head", "quantity"].map(str::to_owned).into()),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

#[tokio::test]
async fn partial_reads_distinguish_unloaded_empty_and_complete_bodies() {
    let store = store::Store::open_memory().await.unwrap();
    let first = record(
        &store,
        "first-projection",
        "Title é",
        "café 👩‍💻\nSecond line",
    )
    .await;
    let empty = record(&store, "empty-projection", "Empty description", "").await;
    let ids = [first.uid.as_str(), empty.uid.as_str()];
    for selection in [
        RecordSelection::All,
        RecordSelection::Uid(&first.uid),
        RecordSelection::Token("first-projection"),
        RecordSelection::Uids(&ids),
    ] {
        let full = store::records::read::<String>(&store.pool, selection, true)
            .await
            .unwrap();
        let partial = store::records::read::<Option<String>>(&store.pool, selection, false)
            .await
            .unwrap();
        let loaded = store::records::read::<Option<String>>(&store.pool, selection, true)
            .await
            .unwrap();
        assert_eq!(full.len(), partial.len());
        assert_eq!(full.len(), loaded.len());
        for ((full, partial), loaded) in full.iter().zip(&partial).zip(&loaded) {
            assert_eq!(metadata(full), metadata(partial));
            assert_eq!(metadata(full), metadata(loaded));
            assert!(partial.body.is_none());
            assert!(partial.body.required().is_err());
            assert_eq!(loaded.body.required().unwrap(), full.body);
            assert_eq!(
                store::records::get(&store.pool, &full.uid)
                    .await
                    .unwrap()
                    .unwrap()
                    .body,
                full.body
            );
        }
    }
    let loaded =
        store::records::read::<Option<String>>(&store.pool, RecordSelection::Uid(&empty.uid), true)
            .await
            .unwrap();
    assert_eq!(loaded[0].body.as_deref(), Some(""));
    assert!(
        store::records::read::<String>(&store.pool, RecordSelection::Uid(&empty.uid), false)
            .await
            .is_err()
    );
    assert!(
        store::records::read::<Option<String>>(&store.pool, RecordSelection::Uids(&[]), false)
            .await
            .unwrap()
            .is_empty()
    );
    let excessive = vec![first.uid.as_str(); 10001];
    assert!(
        store::records::read::<Option<String>>(
            &store.pool,
            RecordSelection::Uids(&excessive),
            false
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn projected_queries_match_full_results_for_filters_orders_includes_and_pagination() {
    let store = store::Store::open_memory().await.unwrap();
    let first = record(&store, "projection-a", "Beta title", "café 👩‍💻 z").await;
    let second = record(&store, "projection-b", "Alpha title", "a description").await;
    store::records::set_extension(
        &store.pool,
        &first.uid,
        "work",
        &json!({"estimate_min": 17}),
    )
    .await
    .unwrap();
    let base = Predicate::Any(vec![
        Predicate::UidEq(first.uid.clone()),
        Predicate::UidEq(second.uid.clone()),
    ]);
    for filter in [
        vec![base.clone()],
        vec![base.clone(), Predicate::TextContains("café".into())],
        vec![Predicate::All(vec![
            base.clone(),
            Predicate::Any(vec![
                Predicate::Not(Box::new(Predicate::TextContains("missing".into()))),
                Predicate::TextContains("café".into()),
            ]),
        ])],
    ] {
        for order in [
            Vec::new(),
            vec![Order::Asc("body".into())],
            vec![Order::Desc("description".into())],
            vec![Order::Asc("head".into())],
            vec![Order::Asc("unknown".into())],
        ] {
            let mut query = query(&first.uid);
            query.filter = filter.clone();
            query.order = order;
            query.include = Include {
                extension: Some(protein::ExtensionInclude {
                    namespace: "work".into(),
                }),
                relation_context: Some(protein::relation_context::Include::default()),
                numeric_fields: vec![protein::shader_inputs::Request {
                    record: first.uid.clone(),
                    field: nucleus::shader_effect::bindings::NumericField::Quantity,
                }],
                ..Default::default()
            };
            let mut full_query = query.clone();
            full_query.fields = None;
            let full = protein::execute(&store, &full_query).await.unwrap();
            for fields in [
                Vec::new(),
                ["uid", "head", "quantity"].map(str::to_owned).into(),
                ["body", "body", "head"].map(str::to_owned).into(),
                vec!["unknown".into()],
                ["head", "extension", "relation_context", "numeric_values"]
                    .map(str::to_owned)
                    .into(),
            ] {
                query.fields = Some(fields.clone());
                let mut expected = full.clone();
                for row in &mut expected {
                    row.as_object_mut().unwrap().retain(|field, _| {
                        field == "uid" || field == "kind" || fields.contains(field)
                    });
                }
                assert_eq!(protein::execute(&store, &query).await.unwrap(), expected);
                query.limit = Some(1);
                assert_eq!(
                    protein::execute(&store, &query).await.unwrap(),
                    expected.iter().take(1).cloned().collect::<Vec<_>>()
                );
                query.limit = None;
                if let Some(first) = expected.first() {
                    query.include.record_after = Some(first["uid"].as_str().unwrap().into());
                    assert_eq!(
                        protein::execute(&store, &query).await.unwrap(),
                        expected.iter().skip(1).cloned().collect::<Vec<_>>()
                    );
                    query.include.record_after = None;
                }
            }
            let complete = protein::select_records(&store, &query, None).await.unwrap();
            for row in complete {
                assert_eq!(
                    row.body,
                    if row.uid == first.uid {
                        first.body.as_str()
                    } else {
                        second.body.as_str()
                    }
                );
            }
        }
    }
    let mut aggregate = query(&first.uid);
    aggregate.filter = vec![base];
    aggregate.fields = None;
    aggregate.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Total,
    });
    let full = protein::execute(&store, &aggregate).await.unwrap();
    aggregate.fields = Some(Vec::new());
    assert_eq!(protein::execute(&store, &aggregate).await.unwrap(), full);
    let among = HashSet::from([first.uid.clone(), second.uid.clone()]);
    assert_eq!(
        protein::matching_among(&store, &aggregate, &among)
            .await
            .unwrap(),
        among
    );
    store::sqlx::query("UPDATE record SET deleted_at = '2026-11-05' WHERE uid = ?")
        .bind(&first.uid)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(
        protein::execute(&store, &query(&first.uid))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn delegated_body_policies_and_revocation_remain_fresh() {
    let store = store::Store::open_memory().await.unwrap();
    let target = record(
        &store,
        "delegated-projection",
        "Visible title",
        "permit café",
    )
    .await;
    let person = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Reader",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let role = store::auth::ensure_role(&store.pool, "projection-reader")
        .await
        .unwrap();
    store::auth::compare_and_set_role(&store.pool, &person, Some(role), 0)
        .await
        .unwrap();
    let permission = store::auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    store::auth::grant(&store.pool, role, permission)
        .await
        .unwrap();
    let policy = protein::authority::RolePolicy {
        read: Predicate::TextContains("permit".into()),
        grants: Vec::new(),
    };
    let stored =
        store::role_policies::set(&store.pool, role, &serde_json::to_value(policy).unwrap(), 0)
            .await
            .unwrap();
    let query = query(&target.uid);
    let rows = protein::execute_for(&store, &query, Some(&person))
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], target.uid);
    store::records::set_text(&store.pool, &target.uid, None, Some("no longer allowed"))
        .await
        .unwrap();
    assert!(
        protein::execute_for(&store, &query, Some(&person))
            .await
            .unwrap()
            .is_empty()
    );
    let policy = protein::authority::RolePolicy {
        read: Predicate::UidEq(target.uid.clone()),
        grants: Vec::new(),
    };
    store::role_policies::set(
        &store.pool,
        role,
        &serde_json::to_value(policy).unwrap(),
        stored.revision,
    )
    .await
    .unwrap();
    assert_eq!(
        protein::execute_for(&store, &query, Some(&person))
            .await
            .unwrap()
            .len(),
        1
    );
    store::auth::revoke(&store.pool, role, permission)
        .await
        .unwrap();
    assert!(
        protein::execute_for(&store, &query, Some(&person))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
#[ignore = "isolated decoded-body work accounting"]
async fn isolated_body_dependency_accounting() {
    static BYTES: AtomicU64 = AtomicU64::new(0);
    fn observe(reading: nucleus::performance::Reading) {
        if let nucleus::performance::Reading::Count("record-read-body-bytes", bytes) = reading {
            BYTES.fetch_add(bytes, Ordering::Relaxed);
        }
    }
    let store = store::Store::open_memory().await.unwrap();
    let target = record(
        &store,
        "counted-projection",
        "Title",
        &"café 👩‍💻\n".repeat(100),
    )
    .await;
    assert!(nucleus::performance::install(observe));
    let base = query(&target.uid);
    let mut cases = vec![(base.clone(), false)];
    let mut changed = base.clone();
    changed.fields = Some(Vec::new());
    cases.push((changed, false));
    let mut changed = base.clone();
    changed.fields = Some(vec!["body".into()]);
    cases.push((changed, true));
    let mut changed = base.clone();
    changed.fields = Some(vec!["unknown".into()]);
    cases.push((changed, true));
    let mut changed = base.clone();
    changed
        .filter
        .push(Predicate::All(vec![Predicate::Not(Box::new(
            Predicate::TextContains("absent".into()),
        ))]));
    cases.push((changed, true));
    for field in ["body", "description", "unknown"] {
        let mut changed = base.clone();
        changed.order = vec![Order::Asc(field.into())];
        cases.push((changed, true));
    }
    let mut changed = base.clone();
    changed.order = vec![Order::Asc("head".into())];
    cases.push((changed, false));
    let mut changed = base.clone();
    changed.fields = None;
    changed.order = vec![Order::Asc("body".into())];
    changed.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Total,
    });
    cases.push((changed, false));
    let mut changed = base.clone();
    changed.fields = Some(vec!["extension".into()]);
    changed.include.extension = Some(protein::ExtensionInclude {
        namespace: "work".into(),
    });
    cases.push((changed, false));
    for (index, (query, needs_body)) in cases.iter().enumerate() {
        BYTES.store(0, Ordering::Relaxed);
        protein::execute(&store, query).await.unwrap();
        let bytes = BYTES.load(Ordering::Relaxed);
        if *needs_body {
            assert!(bytes >= target.body.len() as u64, "case {index}: {bytes}");
        } else {
            assert_eq!(bytes, 0, "case {index}");
        }
    }
    BYTES.store(0, Ordering::Relaxed);
    let full = protein::matching_records(&store, &base, None)
        .await
        .unwrap();
    assert_eq!(full[0].body, target.body);
    assert_eq!(BYTES.load(Ordering::Relaxed), target.body.len() as u64);
}
