//! Stage 3+ Protein features: aggregates, availability, the visibility gate,
//! saved Proteins, and the place `near` predicate.

use engine::actions::Action;
use engine::Engine;
use nucleus::RecordKind;
use protein::{
    Aggregate, AggregateOp, GroupBy, Include, Predicate, Protein, Source,
};

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

async fn make(e: &Engine, slug: &str, kind: RecordKind, q: f64) -> String {
    e.act(
        Action::CreateRecord {
            slug: Some(slug.into()),
            kind,
            head: slug.into(),
            body: String::new(),
            quantity: q,
        },
        None,
    )
    .await
    .unwrap()
    .created
    .unwrap()
}

fn base(source: Source, filter: Vec<Predicate>) -> Protein {
    Protein { source, filter, include: Include::default(), aggregate: None, order: vec![], limit: None }
}

#[tokio::test]
async fn aggregate_sums_by_group() {
    let e = engine().await;
    make(&e, "checking", RecordKind::Plain, 500.0).await;
    make(&e, "savings", RecordKind::Plain, 1500.0).await;
    make(&e, "rules.x", RecordKind::Rule, 1.0).await;

    let mut p = base(Source::Record, vec![]);
    p.aggregate = Some(Aggregate { op: AggregateOp::Sum, by: GroupBy::Kind });
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let plain = rows.iter().find(|r| r["group"] == "plain").unwrap();
    assert_eq!(plain["value"], 2000.0);
}

#[tokio::test]
async fn availability_reflects_active_outgoing_promises() {
    let e = engine().await;
    let apples = make(&e, "apples.stock", RecordKind::Plain, 10.0).await;
    // an active outgoing promise reserves 3
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
        e.act(Action::PromiseTransition { promise: promise.clone(), to }, None).await.unwrap();
    }

    let mut p = base(Source::Record, vec![Predicate::SlugEq("apples.stock".into())]);
    p.include.availability = true;
    let rows = protein::execute(&e.store, &p).await.unwrap();
    assert_eq!(rows[0]["quantity"], 10.0);
    assert_eq!(rows[0]["available"], 7.0, "10 - 3 reserved");
    assert_eq!(rows[0]["planned"], 7.0, "10 + (-3) agreed/active");
}

#[tokio::test]
async fn uid_eq_targets_one_record_directly() {
    let e = engine().await;
    let target = make(&e, "target.record", RecordKind::Plain, 3.0).await;
    make(&e, "other.record", RecordKind::Plain, 3.0).await;

    let p = base(Source::Record, vec![Predicate::UidEq(target)]);
    let rows = protein::execute(&e.store, &p).await.unwrap();

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "target.record");
}

#[tokio::test]
async fn visibility_gate_is_the_one_read_boundary() {
    let e = engine().await;
    let public_need = make(&e, "public.apples", RecordKind::Plain, -1.0).await;
    make(&e, "private.diary", RecordKind::Plain, -1.0).await;

    // grant only the public need to an outside organ
    e.act(
        Action::GrantVisibility {
            subject_kind: "organ".into(),
            subject: Some("organ.neighbors".into()),
            target: public_need.clone(),
        },
        None,
    )
    .await
    .unwrap();

    let p = base(Source::Record, vec![Predicate::QuantityLt(0.0)]);
    // local Cell sees both
    assert_eq!(protein::execute(&e.store, &p).await.unwrap().len(), 2);
    // the neighbor organ sees only what was granted
    let seen = protein::execute_for(&e.store, &p, Some("organ.neighbors")).await.unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0]["slug"], "public.apples");
    // a stranger sees nothing
    assert!(protein::execute_for(&e.store, &p, Some("organ.unknown")).await.unwrap().is_empty());
}

#[tokio::test]
async fn saved_protein_is_a_record() {
    let e = engine().await;
    make(&e, "a", RecordKind::Plain, -1.0).await;
    make(&e, "b", RecordKind::Plain, 5.0).await;

    let ast = serde_json::json!({
        "source": "record",
        "where": [ { "quantity_lt": 0.0 } ]
    });
    e.act(
        Action::SaveProtein { slug: "views.my-needs".into(), head: "My Needs".into(), ast },
        None,
    )
    .await
    .unwrap();

    let rows = protein::execute_saved(&e.store, "views.my-needs", None).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["slug"], "a");
}

#[tokio::test]
async fn near_predicate_uses_the_place_instinct() {
    let e = engine().await;
    let home = make(&e, "home", RecordKind::Plain, 1.0).await;
    let market = make(&e, "market.needs", RecordKind::Plain, -1.0).await;
    let far = make(&e, "far.needs", RecordKind::Plain, -1.0).await;

    // home at origin; market ~100m north; far ~10km north
    e.act(Action::SetPlace { target: home, lat: 0.0, lon: 0.0, address: None }, None).await.unwrap();
    e.act(Action::SetPlace { target: market, lat: 0.0009, lon: 0.0, address: None }, None)
        .await
        .unwrap();
    e.act(Action::SetPlace { target: far, lat: 0.09, lon: 0.0, address: None }, None).await.unwrap();

    let p = base(
        Source::Record,
        vec![
            Predicate::QuantityLt(0.0),
            Predicate::Near { of: "home".into(), meters: 2000.0 },
        ],
    );
    let rows = protein::execute(&e.store, &p).await.unwrap();
    let slugs: Vec<&str> = rows.iter().map(|r| r["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["market.needs"], "only the nearby need matches");
}
