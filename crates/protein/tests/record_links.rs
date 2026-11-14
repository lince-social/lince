use engine::Engine;
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};

static GRAPH_LOADS: AtomicU64 = AtomicU64::new(0);
static BODY_BYTES: AtomicU64 = AtomicU64::new(0);

fn observe(reading: nucleus::performance::Reading) {
    match reading {
        nucleus::performance::Reading::Count("protein-link-graph-loads", count) => {
            GRAPH_LOADS.fetch_add(count, Ordering::Relaxed);
        }
        nucleus::performance::Reading::Count("record-read-body-bytes", count) => {
            BODY_BYTES.fetch_add(count, Ordering::Relaxed);
        }
        _ => {}
    }
}

#[tokio::test]
async fn many_rows_share_one_fresh_graph_and_keep_filtered_links_and_record_projections_exact() {
    assert!(nucleus::performance::install(observe));
    let engine = Engine::open_memory().await.unwrap();
    let store = &engine.store;
    let kind = store::concepts::create(&store.pool, "query-shared-link", &[])
        .await
        .unwrap();
    let other_kind = store::concepts::create(&store.pool, "query-other-link", &[])
        .await
        .unwrap();
    let mut ids = Vec::new();
    for i in 0..12 {
        ids.push(
            store::records::create(
                &store.pool,
                store::records::NewRecord {
                    slug: None,
                    kind: nucleus::RecordKind::Plain,
                    head: &format!("Graph Record {i}"),
                    body: &"Private body excluded by requested fields. ".repeat(1600),
                    quantity: store::exact::zero(),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let mut assertions = Vec::new();
    for i in 0..12 {
        assertions.push(
            store::assertions::assert(
                &store.pool,
                store::assertions::NewAssertion {
                    subject_uid: &ids[i],
                    predicate_uid: if i % 2 == 0 { &kind } else { &other_kind },
                    object_uid: Some(&ids[(i + 1) % ids.len()]),
                    role: store::assertions::AssertionRole::Ordinary,
                    quantity: Some(nucleus::DecimalValue::parse_inferred("2.5").unwrap()),
                    unit_uid: None,
                    asserted_by: None,
                },
            )
            .await
            .unwrap(),
        );
    }
    let mut query: protein::Protein = serde_json::from_value(json!({
        "source": "record", "where": [{"any": ids.iter().map(|id| json!({"uid_eq": id})).collect::<Vec<_>>() }],
        "fields": ["uid", "head", "links"],
        "include": {"links": {"kinds": ["*"], "direction": "both", "depth": 4}}
    })).unwrap();
    GRAPH_LOADS.store(0, Ordering::Relaxed);
    BODY_BYTES.store(0, Ordering::Relaxed);
    let rows = protein::execute(store, &query).await.unwrap();
    assert_eq!(GRAPH_LOADS.load(Ordering::Relaxed), 1);
    assert_eq!(BODY_BYTES.load(Ordering::Relaxed), 0);
    assert_eq!(rows.len(), ids.len());
    for row in &rows {
        let mut single = query.clone();
        single.filter = vec![protein::Predicate::UidEq(
            row["uid"].as_str().unwrap().into(),
        )];
        assert_eq!(
            protein::execute(store, &single).await.unwrap(),
            vec![row.clone()]
        );
        assert!(!row["links"].as_array().unwrap().is_empty());
        assert!(row.get("body").is_none());
    }
    query.include.links.as_mut().unwrap().kinds =
        vec![" query-shared-link ".into(), "query-shared-link".into()];
    let rows = protein::execute(store, &query).await.unwrap();
    let links = |rows: Vec<Value>| {
        rows.into_iter()
            .flat_map(|mut row| row["links"].take().as_array().unwrap().clone())
            .collect::<Vec<_>>()
    };
    let values = links(rows);
    assert!(!values.is_empty());
    assert!(
        values
            .iter()
            .all(|row| row["kind_uid"] == kind && row["quantity"] == 2.5)
    );
    store::assertions::retract(&store.pool, &assertions[0], None)
        .await
        .unwrap();
    let values = links(protein::execute(store, &query).await.unwrap());
    assert!(values.iter().all(|row| row["uid"] != assertions[0]));
    query.include.links.as_mut().unwrap().kinds = vec!["query-missing-link".into()];
    assert!(links(protein::execute(store, &query).await.unwrap()).is_empty());
    query.filter = vec![protein::Predicate::UidEq("missing-record".into())];
    GRAPH_LOADS.store(0, Ordering::Relaxed);
    assert!(protein::execute(store, &query).await.unwrap().is_empty());
    assert_eq!(GRAPH_LOADS.load(Ordering::Relaxed), 0);
}
