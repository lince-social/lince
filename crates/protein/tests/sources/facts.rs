use super::fixtures::{engine, plain, protein};
use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::{Cause, NewFact};
use protein::{Aggregate, AggregateOp, GroupBy, Predicate, Source, execute};

async fn bump(e: &Engine, uid: &str, delta: f64, cause: Cause, now: DateTime<Utc>) {
    e.append(NewFact::quantity_f64(uid, delta, cause), now)
        .await
        .expect("append");
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .expect("fixture timestamp")
        .to_utc()
}

#[tokio::test]
async fn fact_source_filters_and_aggregates_the_ledger() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let apples = plain(&e, "apples.stock", 0.0).await;
    let hammer = plain(&e, "hammer", 0.0).await;
    store::assertions::set_identity(&e.store.pool, &apples, Some(&food), None)
        .await
        .unwrap();

    bump(
        &e,
        &apples,
        10.0,
        Cause::user_edit(),
        at("2026-07-01T08:00:00Z"),
    )
    .await;
    bump(
        &e,
        &apples,
        -3.0,
        Cause::settlement("t_X"),
        at("2026-07-02T08:00:00Z"),
    )
    .await;
    bump(
        &e,
        &hammer,
        1.0,
        Cause::user_edit(),
        at("2026-07-02T09:00:00Z"),
    )
    .await;

    let mut q = protein(Source::Fact);
    q.filter = vec![Predicate::RecordEq("apples.stock".into())];
    let rows = execute(&e.store, &q).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|r| r["record"] == apples.as_str()));

    for filter in [
        Predicate::AtSince("2026-07-02T00:00:00Z".into()),
        Predicate::ConceptIn("food".into()),
    ] {
        q.filter = vec![filter];
        assert_eq!(execute(&e.store, &q).await.unwrap().len(), 2);
    }

    q.filter.clear();
    q.aggregate = Some(Aggregate {
        op: AggregateOp::Sum,
        by: GroupBy::CauseKind,
    });
    let rows = execute(&e.store, &q).await.unwrap();
    let get = |group: &str| {
        rows.iter()
            .find(|r| r["group"] == group)
            .and_then(|r| r["net"].as_str())
    };
    assert_eq!(get("user_edit"), Some("11"));
    assert_eq!(get("settlement"), Some("-3"));

    q.aggregate.as_mut().unwrap().by = GroupBy::Day;
    let rows = execute(&e.store, &q).await.unwrap();
    assert!(
        rows.iter()
            .any(|r| r["group"] == "2026-07-01" && r["net"] == "10")
    );
    assert!(
        rows.iter()
            .any(|r| r["group"] == "2026-07-02" && r["net"] == "-2")
    );
}
