use super::fixtures::{bucket, capture, engine, march, record, spending_month};
use engine::actions::Action;
use protein::{GroupBy, Predicate, execute};

#[tokio::test]
async fn grouping_by_classification_says_what_the_changes_were_for() {
    let e = engine().await;
    spending_month(&e).await;
    let food = store::concepts::resolve(&e.store.pool, "food")
        .await
        .unwrap()
        .unwrap();

    let rows = execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    let food_row = bucket(&rows, &food);
    assert_eq!(food_row["net"], "-20.25");
    assert_eq!(food_row["gains"], "10.50");
    assert_eq!(food_row["count"], 3);
}

#[tokio::test]
async fn the_record_axis_and_the_change_axis_are_different_questions() {
    let e = engine().await;
    spending_month(&e).await;
    let balance = store::concepts::resolve(&e.store.pool, "balance")
        .await
        .unwrap()
        .unwrap();

    let rows = execute(&e.store, &march(GroupBy::Concept)).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(bucket(&rows, &balance)["count"], 4);

    let rows = execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
}

#[tokio::test]
async fn classified_in_walks_down_the_lingua_dag() {
    let e = engine().await;
    spending_month(&e).await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("cost".into()));
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-1020.25");

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("food".into()));
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-20.25");
}

#[tokio::test]
async fn concept_in_reads_what_a_record_counts_as_not_only_what_it_is() {
    let e = engine().await;
    spending_month(&e).await;
    store::concepts::create(&e.store.pool, "savings-pot", &[])
        .await
        .unwrap();
    record(&e, "rainy-day", "savings-pot", None).await;
    e.act(
        Action::AssertRecord {
            subject: "rainy-day".into(),
            predicate: "balance".into(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
    capture(&e, "rainy-day", "100", "rent", "2026-03-10T10:00:00Z").await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ConceptIn("balance".into()));
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["count"], 5);
}

#[tokio::test]
async fn unclassified_changes_get_a_named_bucket_and_are_never_dropped() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "balance", &[])
        .await
        .unwrap();
    record(&e, "checking", "balance", None).await;
    e.act(
        Action::CaptureEntry {
            target: "checking".into(),
            amount: "-7.25".into(),
            concept: None,
            note: None,
            at: Some("2026-03-04T10:00:00Z".into()),
            request_id: None,
        },
        None,
    )
    .await
    .unwrap();

    let rows = execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    let unclassified = bucket(&rows, "(unclassified)");
    assert_eq!(unclassified["net"], "-7.25");
    assert_eq!(unclassified["count"], 1);
}

#[tokio::test]
async fn re_tagging_a_change_moves_the_total_without_touching_the_fact() {
    let e = engine().await;
    spending_month(&e).await;
    let food = store::concepts::resolve(&e.store.pool, "food")
        .await
        .unwrap()
        .unwrap();
    let rent = store::concepts::resolve(&e.store.pool, "rent")
        .await
        .unwrap()
        .unwrap();

    let checking = store::records::resolve(&e.store.pool, "checking")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let facts = store::facts::for_record(&e.store.pool, &checking, 100)
        .await
        .unwrap();
    let mistagged = facts
        .iter()
        .find(|f| f.delta.to_string() == "-1000")
        .expect("the rent change");
    let before = mistagged.hash.clone();

    e.act(
        Action::ClassifyFact {
            fact: mistagged.uid.clone(),
            concept: Some("food".into()),
            note: Some("was not rent".into()),
        },
        None,
    )
    .await
    .unwrap();

    let after = store::facts::get(&e.store.pool, &mistagged.uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.hash, before);
    assert_eq!(after.delta.to_string(), "-1000");

    let rows = execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(bucket(&rows, &food)["net"], "-1020.25");
    assert!(rows.iter().all(|r| r["group"] != rent.as_str()));

    let history = store::ledger::classification_history(&e.store.pool, &mistagged.uid)
        .await
        .unwrap();
    assert_eq!(history.len(), 2);
}
