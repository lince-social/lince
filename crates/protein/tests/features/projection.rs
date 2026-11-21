use super::fixtures::{base, engine, make};
use engine::actions::Action;
use nucleus::RecordKind;
use protein::{Aggregate, AggregateOp, GroupBy, Order, Predicate, Source, execute, select_records};
use std::collections::HashSet;

#[tokio::test]
async fn aggregate_sums_by_group() {
    let e = engine().await;
    make(&e, "checking", RecordKind::Plain, 500.0).await;
    make(&e, "savings", RecordKind::Plain, 1500.0).await;
    make(&e, "rules.x", RecordKind::Rule, 1.0).await;

    let mut p = base(Source::Record, vec![]);
    p.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::Kind,
    });
    let rows = execute(&e.store, &p).await.unwrap();
    let plain = rows.iter().find(|r| r["group"] == "plain").unwrap();
    assert_eq!(plain["value"], "2000");
}

#[tokio::test]
async fn availability_reflects_active_outgoing_promises() {
    let e = engine().await;
    let apples = make(&e, "apples.stock", RecordKind::Plain, 10.0).await;
    make(&e, "someone", RecordKind::Person, 1.0).await;
    store::config::set_transfer_reservation_default(&e.store.pool, "active")
        .await
        .unwrap();
    let promise = e
        .act(
            Action::CreatePromise {
                record: apples.clone(),
                delta: -3.0,
                window_end: None,
                party: Some("someone".into()),
                open: false,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    for to in [nucleus::PromiseState::Agreed, nucleus::PromiseState::Active] {
        e.act(
            Action::PromiseTransition {
                promise: promise.clone(),
                to,
            },
            None,
        )
        .await
        .unwrap();
    }

    let mut p = base(
        Source::Record,
        vec![Predicate::SlugEq("apples.stock".into())],
    );
    p.include.availability = true;
    let rows = execute(&e.store, &p).await.unwrap();
    assert_eq!(rows[0]["quantity"], "10");
    assert_eq!(rows[0]["available"], 7.0, "10 - 3 reserved");
    assert_eq!(rows[0]["planned"], 7.0, "10 + (-3) agreed/active");
}

#[tokio::test]
async fn fields_narrow_a_row_to_what_was_asked_for() {
    let e = engine().await;
    make(&e, "narrowed", RecordKind::Plain, 3.0).await;

    let mut p = base(Source::Record, vec![Predicate::SlugEq("narrowed".into())]);
    p.fields = Some(vec!["head".into()]);
    let rows = execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    assert!(row.get("head").is_some(), "what was asked for is there");
    assert!(
        row.get("body").is_none() && row.get("quantity").is_none(),
        "and nothing else is: {row}"
    );
    assert!(row.get("uid").is_some(), "uid always survives");
    assert!(row.get("kind").is_some(), "and so does kind");
}

#[tokio::test]
async fn a_field_nobody_named_does_not_appear() {
    let e = engine().await;
    make(&e, "unnamed", RecordKind::Plain, 7.0).await;

    let mut p = base(Source::Record, vec![Predicate::SlugEq("unnamed".into())]);
    p.fields = Some(vec!["head".into(), "a_column_invented_later".into()]);
    let rows = execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    assert!(row.get("head").is_some());
    assert!(
        row.get("organ").is_none() && row.get("created_at").is_none(),
        "columns the selector never named stay home: {row}"
    );
}

#[tokio::test]
async fn a_withheld_column_is_absent_rather_than_blank() {
    let e = engine().await;
    make(&e, "blank-body", RecordKind::Plain, 1.0).await;

    let mut p = base(Source::Record, vec![Predicate::SlugEq("blank-body".into())]);
    let rows = execute(&e.store, &p).await.unwrap();
    let unnarrowed = rows.first().expect("one row");
    assert_eq!(
        unnarrowed.get("body").and_then(serde_json::Value::as_str),
        Some(""),
        "unnarrowed, an empty body is present and empty"
    );

    p.fields = Some(vec!["head".into()]);
    let rows = execute(&e.store, &p).await.unwrap();
    let narrowed = rows.first().expect("one row");
    assert!(
        narrowed.get("body").is_none(),
        "narrowed, it is ABSENT — not null, which a renderer would draw as empty: {narrowed}"
    );
}

#[tokio::test]
async fn no_selector_returns_the_whole_row() {
    let e = engine().await;
    make(&e, "whole", RecordKind::Plain, 1.0).await;

    let p = base(Source::Record, vec![Predicate::SlugEq("whole".into())]);
    let rows = execute(&e.store, &p).await.unwrap();
    let row = rows.first().expect("one row");

    for column in ["uid", "head", "body", "quantity", "organ", "created_at"] {
        assert!(row.get(column).is_some(), "{column} must still be there");
    }
}

#[tokio::test]
async fn record_pages_respect_order_visibility_and_expired_boundaries() {
    let e = engine().await;
    let first = make(&e, "paging-first", RecordKind::Plain, 3.0).await;
    let middle = make(&e, "paging-middle", RecordKind::Plain, 2.0).await;
    let last = make(&e, "paging-last", RecordKind::Plain, 1.0).await;
    let mut query = base(
        Source::Record,
        vec![Predicate::TextContains("paging-".into())],
    );
    query.order = vec![Order::Desc("quantity".into())];
    query.limit = Some(1);
    assert_eq!(execute(&e.store, &query).await.unwrap()[0]["uid"], first);
    query.include.record_after = Some(first.clone());
    assert_eq!(execute(&e.store, &query).await.unwrap()[0]["uid"], middle);
    let visible = HashSet::from([middle.clone(), last]);
    assert!(
        select_records(&e.store, &query, Some(&visible))
            .await
            .is_err()
    );
    e.act(Action::DeleteRecord { target: first }, None)
        .await
        .unwrap();
    assert!(
        execute(&e.store, &query)
            .await
            .unwrap_err()
            .to_string()
            .contains("Refresh")
    );
    query.include.record_after = None;
    assert_eq!(execute(&e.store, &query).await.unwrap()[0]["uid"], middle);
}
