use super::fixtures::{bucket, capture, engine, march, record, spending_month};
use nucleus::RecordKind;
use protein::{Aggregate, AggregateOp, GroupBy, Predicate, execute, execute_for};
use store::records::NewRecord;

#[tokio::test]
async fn totals_are_exact_and_come_back_as_text_not_floats() {
    let e = engine().await;
    spending_month(&e).await;

    let rows = execute(&e.store, &march(GroupBy::Total)).await.unwrap();
    let total = bucket(&rows, "(total)");
    assert_eq!(total["net"], "-1020.25");
    assert!(total["net"].is_string());
    assert_eq!(total["gains"], "10.50");
    assert_eq!(total["losses"], "-1030.75");
    assert_eq!(total["count"], 4);
    assert!(total["unit_uid"].is_null());
}

#[tokio::test]
async fn the_same_query_answers_a_question_that_has_nothing_to_do_with_a_balance() {
    let e = engine().await;
    let reason = store::concepts::create(&e.store.pool, "consumption", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "baking", &[&reason])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "spoilage", &[&reason])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "stock", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kg", &[])
        .await
        .unwrap();
    record(&e, "flour", "stock", Some("kg")).await;

    capture(&e, "flour", "-2.5", "baking", "2026-03-02T10:00:00Z").await;
    capture(&e, "flour", "-1.25", "baking", "2026-03-04T10:00:00Z").await;
    capture(&e, "flour", "-0.75", "spoilage", "2026-03-09T10:00:00Z").await;

    let mut q = march(GroupBy::Total);
    q.filter.push(Predicate::ClassifiedIn("consumption".into()));
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(bucket(&rows, "(total)")["net"], "-4.50");

    let baking = store::concepts::resolve(&e.store.pool, "baking")
        .await
        .unwrap()
        .unwrap();
    let rows = execute(&e.store, &march(GroupBy::Classification))
        .await
        .unwrap();
    assert_eq!(bucket(&rows, &baking)["net"], "-3.75");
}

#[tokio::test]
async fn records_of_different_units_are_separated_rather_than_added() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "stock", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "used", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "kg", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "litre", &[])
        .await
        .unwrap();
    record(&e, "flour", "stock", Some("kg")).await;
    record(&e, "milk", "stock", Some("litre")).await;

    capture(&e, "flour", "-2.5", "used", "2026-03-02T10:00:00Z").await;
    capture(&e, "milk", "-1.5", "used", "2026-03-03T10:00:00Z").await;

    let rows = execute(&e.store, &march(GroupBy::Total)).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["group"] == "(total)"));
    assert!(rows.iter().any(|r| r["net"] == "-2.5"));
    assert!(rows.iter().any(|r| r["net"] == "-1.5"));
    assert!(rows.iter().all(|r| !r["unit_uid"].is_null()));
}

#[tokio::test]
async fn adjacent_windows_tile_without_double_counting() {
    let e = engine().await;
    spending_month(&e).await;

    let count_between = |from: &'static str, to: &'static str| {
        let store = &e.store;
        async move {
            let mut q = march(GroupBy::Total);
            q.filter = vec![
                Predicate::AtSince(from.into()),
                Predicate::AtBefore(to.into()),
            ];
            execute(store, &q)
                .await
                .unwrap()
                .iter()
                .map(|r| r["count"].as_i64().unwrap())
                .sum::<i64>()
        }
    };

    let whole = count_between("2026-03-01T00:00:00Z", "2026-04-01T00:00:00Z").await;
    let first = count_between("2026-03-01T00:00:00Z", "2026-03-06T00:00:00Z").await;
    let second = count_between("2026-03-06T00:00:00Z", "2026-04-01T00:00:00Z").await;
    assert_eq!(whole, 4);
    assert_eq!(first + second, whole);
}

#[tokio::test]
async fn a_count_aggregate_does_not_invent_a_net() {
    let e = engine().await;
    spending_month(&e).await;

    let mut q = march(GroupBy::Classification);
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Count,
        by: GroupBy::Classification,
    });
    let rows = execute(&e.store, &q).await.unwrap();
    for row in &rows {
        assert!(row["count"].is_i64());
        assert!(row["net"].is_null());
        assert!(row["gains"].is_null());
    }
}

#[tokio::test]
async fn totals_never_leave_the_cell_for_a_remote_subject() {
    let e = engine().await;
    spending_month(&e).await;
    let person = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Remote reader",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();

    let rows = execute_for(&e.store, &march(GroupBy::Total), Some(&person.uid))
        .await
        .unwrap();
    assert!(rows.is_empty());
}
