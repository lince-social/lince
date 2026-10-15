use std::collections::HashSet;

use protein::{Predicate, Protein, Source};

#[tokio::test]
async fn exact_identity_queries_preserve_filter_and_visibility_semantics() {
    let store = store::Store::open_memory().await.unwrap();
    let record = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: Some("apples"),
            kind: nucleus::RecordKind::Plain,
            head: "Apples",
            body: "Fruit",
            quantity: nucleus::DecimalValue::parse_inferred("9007199254740993").unwrap(),
        },
    )
    .await
    .unwrap();
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
    let rows = protein::matching_records(&store, &by_uid, None)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].quantity, record.quantity);
    assert!(
        protein::matching_records(&store, &by_uid, Some(&HashSet::new()))
            .await
            .unwrap()
            .is_empty()
    );
    let rows = protein::matching_records(&store, &query(Predicate::SlugEq("apples".into())), None)
        .await
        .unwrap();
    assert_eq!(rows[0].uid, record.uid);
    assert!(
        protein::matching_records(&store, &query(Predicate::SlugEq(record.uid.clone())), None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        protein::matching_records(&store, &query(Predicate::UidEq("apples".into())), None)
            .await
            .unwrap()
            .is_empty()
    );
}
