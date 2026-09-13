use engine::{Engine, actions::Action};
use serde_json::json;

#[tokio::test]
async fn saved_all_groups_filter_flat_sources_and_preserve_visibility() {
    let engine = Engine::open_memory().await.unwrap();
    let mut records = Vec::new();
    for title in ["Apple", "Pear"] {
        records.push(
            engine
                .act(
                    Action::CreateRecord {
                        slug: None,
                        kind: nucleus::RecordKind::Plain,
                        head: title.into(),
                        body: String::new(),
                        quantity: 1.0,
                    },
                    None,
                )
                .await
                .unwrap()
                .created
                .unwrap(),
        );
    }
    for (index, record) in records.iter().enumerate() {
        engine
            .act(
                Action::SetQuantityExact {
                    target: record.clone(),
                    amount: (index + 3).to_string(),
                },
                None,
            )
            .await
            .unwrap();
    }
    let ast = json!({"source":"fact", "where":[{"all":[{"all":[{"record_eq":records[0]}]}]}], "aggregate":{"op":"sum", "by":"total"}});
    let query: protein::Protein = serde_json::from_value(ast.clone()).unwrap();
    let rows = protein::execute(&engine.store, &query).await.unwrap();
    let mut direct = query.clone();
    direct.filter = vec![protein::Predicate::RecordEq(records[0].clone())];
    assert_eq!(
        rows,
        protein::execute(&engine.store, &direct).await.unwrap()
    );
    assert!(!rows.is_empty());
    engine
        .act(
            Action::SaveProtein {
                slug: "fruit-facts".into(),
                head: "Fruit facts".into(),
                ast,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        rows,
        protein::execute_saved(&engine.store, "fruit-facts", None)
            .await
            .unwrap()
    );
    let person = engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind: nucleus::RecordKind::Person,
                head: "Reader".into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let hidden = protein::execute_for(&engine.store, &query, Some(&person))
        .await
        .unwrap();
    assert!(hidden.is_empty());
    let mut invalid = query;
    invalid.filter = vec![protein::Predicate::Any(vec![protein::Predicate::RecordEq(
        records[0].clone(),
    )])];
    assert!(protein::execute(&engine.store, &invalid).await.is_err());
}
