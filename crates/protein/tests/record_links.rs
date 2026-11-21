use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use nucleus::{
    DecimalValue, RecordKind,
    performance::{Reading, install},
};
use serde_json::{Value, json};
use store::{
    Store,
    assertions::{AssertionRole, NewAssertion, assert, retract},
    concepts,
    exact::zero,
    records::{NewRecord, create},
};

static GRAPH_LOADS: AtomicU64 = AtomicU64::new(0);
static BODY_BYTES: AtomicU64 = AtomicU64::new(0);

fn observe(reading: Reading) {
    match reading {
        Reading::Count("protein-link-graph-loads", count) => {
            GRAPH_LOADS.fetch_add(count, Relaxed);
        }
        Reading::Count("record-read-body-bytes", count) => {
            BODY_BYTES.fetch_add(count, Relaxed);
        }
        _ => {}
    }
}

#[tokio::test]
async fn record_link_queries_share_and_refresh_the_graph() {
    assert!(install(observe));
    let store = Store::open_memory().await.expect("open test Store");
    let kind = concepts::create(&store.pool, "query-shared-link", &[])
        .await
        .unwrap();
    let other_kind = concepts::create(&store.pool, "query-other-link", &[])
        .await
        .unwrap();
    let body = "Private body excluded by requested fields. ".repeat(1600);
    let mut ids = Vec::new();
    for i in 0..12 {
        ids.push(
            create(
                &store.pool,
                NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: &format!("Graph Record {i}"),
                    body: &body,
                    quantity: zero(),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let quantity = DecimalValue::parse_inferred("2.5").expect("fixture quantity");
    let mut assertions = Vec::new();
    for (i, uid) in ids.iter().enumerate() {
        assertions.push(
            assert(
                &store.pool,
                NewAssertion {
                    subject_uid: uid,
                    predicate_uid: if i % 2 == 0 { &kind } else { &other_kind },
                    object_uid: Some(&ids[(i + 1) % ids.len()]),
                    role: AssertionRole::Ordinary,
                    quantity: Some(quantity),
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
    GRAPH_LOADS.store(0, Relaxed);
    BODY_BYTES.store(0, Relaxed);
    let rows = protein::execute(&store, &query).await.unwrap();
    assert_eq!(GRAPH_LOADS.load(Relaxed), 1);
    assert_eq!(BODY_BYTES.load(Relaxed), 0);
    assert_eq!(rows.len(), ids.len());
    for row in &rows {
        let mut single = query.clone();
        single.filter = vec![protein::Predicate::UidEq(
            row["uid"].as_str().unwrap().into(),
        )];
        assert_eq!(
            protein::execute(&store, &single).await.unwrap(),
            vec![row.clone()]
        );
        assert!(!row["links"].as_array().unwrap().is_empty());
        assert!(row.get("body").is_none());
    }
    query.include.links.as_mut().unwrap().kinds =
        vec![" query-shared-link ".into(), "query-shared-link".into()];
    let rows = protein::execute(&store, &query).await.unwrap();
    let values = links(rows);
    assert!(!values.is_empty());
    assert!(
        values
            .iter()
            .all(|row| row["kind_uid"] == kind && row["quantity"] == 2.5)
    );
    retract(&store.pool, &assertions[0], None)
        .await
        .expect("retract fixture link");
    let values = links(protein::execute(&store, &query).await.unwrap());
    assert!(values.iter().all(|row| row["uid"] != assertions[0]));
    query.include.links.as_mut().unwrap().kinds = vec!["query-missing-link".into()];
    assert!(links(protein::execute(&store, &query).await.unwrap()).is_empty());
    query.filter = vec![protein::Predicate::UidEq("missing-record".into())];
    GRAPH_LOADS.store(0, Relaxed);
    assert!(protein::execute(&store, &query).await.unwrap().is_empty());
    assert_eq!(GRAPH_LOADS.load(Relaxed), 0);
}

fn links(rows: Vec<Value>) -> Vec<Value> {
    rows.into_iter()
        .flat_map(|mut row| {
            let Value::Array(links) = row["links"].take() else {
                panic!("expected links for {}", row["uid"]);
            };
            links
        })
        .collect()
}
