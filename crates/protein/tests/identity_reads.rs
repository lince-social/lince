use nucleus::{DecimalValue, RecordKind};
use protein::{Predicate, Protein, Source, matching_records};
use std::collections::HashSet;
use store::{
    Store,
    records::{NewRecord, create},
};

#[tokio::test]
async fn exact_identity_queries_preserve_filter_and_visibility_semantics() {
    const SLUG: &str = "apples";
    let store = Store::open_memory().await.expect("Record fixture store");
    let record = create(
        &store.pool,
        NewRecord {
            slug: Some(SLUG),
            kind: RecordKind::Plain,
            head: "Apples",
            body: "Fruit",
            quantity: DecimalValue::parse_inferred("9007199254740993")
                .expect("exact fixture quantity"),
        },
    )
    .await
    .expect("create Record");
    let query = |predicate| Protein {
        source: Source::Record,
        filter: vec![predicate],
        fields: None,
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: Some(10),
    };
    let by_uid = query(Predicate::UidEq(record.uid.clone()));
    let rows = matching_records(&store, &by_uid, None)
        .await
        .expect("read by UID");
    let [row] = rows.as_slice() else {
        panic!("expected one Record for {}, got {}", record.uid, rows.len());
    };
    assert_eq!(row.quantity, record.quantity);
    assert!(
        matching_records(&store, &by_uid, Some(&HashSet::new()))
            .await
            .expect("read by UID within empty scope")
            .is_empty()
    );
    let rows = matching_records(&store, &query(Predicate::SlugEq(SLUG.into())), None)
        .await
        .expect("read by slug");
    assert_eq!(rows.first().expect("slug matched").uid, record.uid);
    for filter in [Predicate::SlugEq(record.uid), Predicate::UidEq(SLUG.into())] {
        assert!(
            matching_records(&store, &query(filter), None)
                .await
                .expect("read with mismatched identity type")
                .is_empty()
        );
    }
}
