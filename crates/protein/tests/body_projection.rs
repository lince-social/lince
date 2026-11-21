use nucleus::{
    DecimalValue, RecordKind,
    performance::{Reading, install},
    shader_effect::bindings::NumericField,
};
use protein::{
    Aggregate, AggregateOp, ExtensionInclude, GroupBy, Include, Order, Predicate, Protein, Source,
    authority::RolePolicy, execute, execute_for, matching_among, matching_records,
    relation_context, select_records, shader_inputs::Request,
};
use serde_json::{Value, json, to_value};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicU64, Ordering},
};
use store::{
    Store, auth,
    records::{
        NewRecord, RecordBody, RecordRow, RecordSelection, create, get, read, set_extension,
        set_text,
    },
    role_policies,
    sqlx::query as sql,
};

async fn record(store: &Store, slug: &str, head: &str, body: &str) -> RecordRow {
    create(
        &store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head,
            body,
            quantity: DecimalValue::parse_inferred("9007199254740993.125")
                .expect("fixture quantity"),
        },
    )
    .await
    .expect("create Record fixture")
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
    let store = Store::open_memory().await.unwrap();
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
        let full = read::<String>(&store.pool, selection, true).await.unwrap();
        let partial = read::<Option<String>>(&store.pool, selection, false)
            .await
            .unwrap();
        let loaded = read::<Option<String>>(&store.pool, selection, true)
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
                get(&store.pool, &full.uid).await.unwrap().unwrap().body,
                full.body
            );
        }
    }
    let loaded = read::<Option<String>>(&store.pool, RecordSelection::Uid(&empty.uid), true)
        .await
        .unwrap();
    let [loaded] = loaded.as_slice() else {
        panic!("expected one Record with a loaded empty body")
    };
    assert_eq!(loaded.body.as_deref(), Some(""));
    assert!(
        read::<String>(&store.pool, RecordSelection::Uid(&empty.uid), false)
            .await
            .is_err()
    );
    assert!(
        read::<Option<String>>(&store.pool, RecordSelection::Uids(&[]), false)
            .await
            .unwrap()
            .is_empty()
    );
    let excessive = vec![first.uid.as_str(); 10001];
    assert!(
        read::<Option<String>>(&store.pool, RecordSelection::Uids(&excessive), false)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn projected_queries_match_full_results_for_filters_orders_includes_and_pagination() {
    let store = Store::open_memory().await.unwrap();
    let first = record(&store, "projection-a", "Beta title", "café 👩‍💻 z").await;
    let second = record(&store, "projection-b", "Alpha title", "a description").await;
    set_extension(
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
    let include = Include {
        extension: Some(ExtensionInclude {
            namespace: "work".into(),
        }),
        relation_context: Some(relation_context::Include::default()),
        numeric_fields: vec![Request {
            record: first.uid.clone(),
            field: NumericField::Quantity,
        }],
        ..Default::default()
    };
    let fields: [&[&str]; 5] = [
        &[],
        &["uid", "head", "quantity"],
        &["body", "body", "head"],
        &["unknown"],
        &["head", "extension", "relation_context", "numeric_values"],
    ];
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
            let mut query = Protein {
                source: Source::Record,
                filter: filter.clone(),
                order,
                include: include.clone(),
                fields: None,
                aggregate: None,
                limit: None,
            };
            let full = execute(&store, &query).await.unwrap();
            for fields in fields {
                query.fields = Some(fields.iter().map(|field| (*field).into()).collect());
                let mut expected = full.clone();
                for row in &mut expected {
                    row.as_object_mut().unwrap().retain(|field, _| {
                        field == "uid" || field == "kind" || fields.contains(&field.as_str())
                    });
                }
                assert_eq!(execute(&store, &query).await.unwrap(), expected);
                query.limit = Some(1);
                assert_eq!(
                    execute(&store, &query).await.unwrap(),
                    expected.get(..1).unwrap_or(&expected)
                );
                query.limit = None;
                if let Some(first) = expected.first() {
                    query.include.record_after = Some(first["uid"].as_str().unwrap().into());
                    assert_eq!(execute(&store, &query).await.unwrap(), expected[1..]);
                    query.include.record_after = None;
                }
            }
            let complete = select_records(&store, &query, None).await.unwrap();
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
    let full = execute(&store, &aggregate).await.unwrap();
    aggregate.fields = Some(Vec::new());
    assert_eq!(execute(&store, &aggregate).await.unwrap(), full);
    let among = HashSet::from([first.uid.clone(), second.uid.clone()]);
    assert_eq!(
        matching_among(&store, &aggregate, &among).await.unwrap(),
        among
    );
    sql("UPDATE record SET deleted_at = '2026-11-05' WHERE uid = ?")
        .bind(&first.uid)
        .execute(&store.pool)
        .await
        .unwrap();
    assert!(
        execute(&store, &query(&first.uid))
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn delegated_body_policies_and_revocation_remain_fresh() {
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        "delegated-projection",
        "Visible title",
        "permit café",
    )
    .await;
    let person = create(
        &store.pool,
        NewRecord {
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
    let role = auth::ensure_role(&store.pool, "projection-reader")
        .await
        .unwrap();
    auth::compare_and_set_role(&store.pool, &person, Some(role), 0)
        .await
        .unwrap();
    let permission = auth::ensure_permission(&store.pool, "record", "read")
        .await
        .unwrap();
    auth::grant(&store.pool, role, permission).await.unwrap();
    let policy = RolePolicy {
        read: Predicate::TextContains("permit".into()),
        grants: Vec::new(),
    };
    let stored = role_policies::set(&store.pool, role, &to_value(policy).unwrap(), 0)
        .await
        .unwrap();
    let query = query(&target.uid);
    let scoped = || execute_for(&store, &query, Some(&person));
    let rows = scoped().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["uid"], target.uid);
    set_text(&store.pool, &target.uid, None, Some("no longer allowed"))
        .await
        .unwrap();
    assert!(scoped().await.unwrap().is_empty());
    let policy = RolePolicy {
        read: Predicate::UidEq(target.uid),
        grants: Vec::new(),
    };
    role_policies::set(
        &store.pool,
        role,
        &to_value(policy).unwrap(),
        stored.revision,
    )
    .await
    .unwrap();
    assert_eq!(scoped().await.unwrap().len(), 1);
    auth::revoke(&store.pool, role, permission).await.unwrap();
    assert!(scoped().await.unwrap().is_empty());
}

#[tokio::test]
#[ignore = "isolated decoded-body work accounting"]
async fn isolated_body_dependency_accounting() {
    static BYTES: AtomicU64 = AtomicU64::new(0);
    fn observe(reading: Reading) {
        if let Reading::Count("record-read-body-bytes", bytes) = reading {
            BYTES.fetch_add(bytes, Ordering::Relaxed);
        }
    }
    let store = Store::open_memory().await.unwrap();
    let target = record(
        &store,
        "counted-projection",
        "Title",
        &"café 👩‍💻\n".repeat(100),
    )
    .await;
    assert!(install(observe));
    let base = query(&target.uid);
    let with_fields = |fields: &[&str]| Protein {
        fields: Some(fields.iter().map(|field| (*field).into()).collect()),
        ..base.clone()
    };
    let order_by = |field: &str| Protein {
        order: vec![Order::Asc(field.into())],
        ..base.clone()
    };
    let mut text = base.clone();
    text.filter
        .push(Predicate::All(vec![Predicate::Not(Box::new(
            Predicate::TextContains("absent".into()),
        ))]));
    let mut aggregate = order_by("body");
    aggregate.fields = None;
    aggregate.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Total,
    });
    let mut extra = with_fields(&["extension"]);
    extra.include.extension = Some(ExtensionInclude {
        namespace: "work".into(),
    });
    let cases = [
        (base.clone(), false),
        (with_fields(&[]), false),
        (with_fields(&["body"]), true),
        (with_fields(&["unknown"]), true),
        (text, true),
        (order_by("body"), true),
        (order_by("description"), true),
        (order_by("unknown"), true),
        (order_by("head"), false),
        (aggregate, false),
        (extra, false),
    ];
    let body_bytes = u64::try_from(target.body.len()).expect("fixture size fits the counter");
    for (index, (query, needs_body)) in cases.iter().enumerate() {
        BYTES.store(0, Ordering::Relaxed);
        execute(&store, query).await.unwrap();
        let bytes = BYTES.load(Ordering::Relaxed);
        if *needs_body {
            assert!(bytes >= body_bytes, "case {index}: {bytes}");
        } else {
            assert_eq!(bytes, 0, "case {index}");
        }
    }
    BYTES.store(0, Ordering::Relaxed);
    let full = matching_records(&store, &base, None).await.unwrap();
    assert_eq!(full[0].body, target.body);
    assert_eq!(BYTES.load(Ordering::Relaxed), body_bytes);
}
