use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use protein::{Predicate, Protein, execute, execute_for, execute_saved};
use serde_json::{from_value, json};

async fn record(engine: &Engine, head: &str, kind: RecordKind) -> String {
    engine
        .act(
            Action::CreateRecord {
                slug: None,
                kind,
                head: head.into(),
                body: String::new(),
                quantity: 1.0,
            },
            None,
        )
        .await
        .expect("create fixture Record")
        .created
        .expect("Record UID")
}

#[tokio::test]
async fn saved_all_groups_filter_flat_sources_and_preserve_visibility() {
    let engine = Engine::open_memory()
        .await
        .expect("conjunction fixture Engine");
    let mut records = Vec::with_capacity(2);
    for title in ["Apple", "Pear"] {
        records.push(record(&engine, title, RecordKind::Plain).await);
    }
    for (uid, amount) in records.iter().zip(["3", "4"]) {
        engine
            .act(
                Action::SetQuantityExact {
                    target: uid.clone(),
                    amount: amount.into(),
                },
                None,
            )
            .await
            .unwrap();
    }
    let [apple, _] = records.as_slice() else {
        panic!("expected two fixture Records")
    };
    let ast = json!({
        "source": "fact",
        "where": [{"all": [{"all": [{"record_eq": apple}]}]}],
        "aggregate": {"op": "sum", "by": "total"}
    });
    let query: Protein = from_value(ast.clone()).unwrap();
    let rows = execute(&engine.store, &query).await.unwrap();
    let direct = Protein {
        filter: vec![Predicate::RecordEq(apple.clone())],
        ..query.clone()
    };
    assert_eq!(rows, execute(&engine.store, &direct).await.unwrap());
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
        execute_saved(&engine.store, "fruit-facts", None)
            .await
            .unwrap()
    );

    let person = record(&engine, "Reader", RecordKind::Person).await;
    assert!(
        execute_for(&engine.store, &query, Some(&person))
            .await
            .unwrap()
            .is_empty()
    );

    let invalid = Protein {
        filter: vec![Predicate::Any(direct.filter)],
        ..query
    };
    assert!(execute(&engine.store, &invalid).await.is_err());
}
