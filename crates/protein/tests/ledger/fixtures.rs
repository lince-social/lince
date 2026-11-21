use engine::{Engine, actions::Action};
use nucleus::RecordKind;
use protein::{Aggregate, AggregateOp, GroupBy, Include, Predicate, Protein, Source};
use serde_json::Value;
use store::records::NewRecord;

pub(super) async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://protein-ledger.test")
        .await
        .expect("local Organ");
    engine
}

pub(super) async fn record(e: &Engine, slug: &str, concept: &str, unit: Option<&str>) -> String {
    let uid = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .expect("record")
    .uid;
    let concept_uid = store::concepts::resolve(&e.store.pool, concept)
        .await
        .expect("resolve fixture concept")
        .expect("fixture concept exists");
    store::assertions::set_identity(&e.store.pool, &uid, Some(&concept_uid), None)
        .await
        .expect("set fixture identity");
    if let Some(unit) = unit {
        let unit_uid = store::concepts::resolve(&e.store.pool, unit)
            .await
            .expect("resolve fixture unit")
            .expect("fixture unit exists");
        store::records::set_unit(&e.store.pool, &uid, Some(&unit_uid))
            .await
            .expect("set fixture unit");
    }
    uid
}

pub(super) async fn capture(e: &Engine, target: &str, amount: &str, concept: &str, at: &str) {
    e.act(
        Action::CaptureEntry {
            target: target.to_string(),
            amount: amount.to_string(),
            concept: Some(concept.to_string()),
            note: None,
            at: Some(at.to_string()),
            request_id: None,
        },
        None,
    )
    .await
    .expect("capture");
}

pub(super) fn march(by: GroupBy) -> Protein {
    Protein {
        source: Source::Fact,
        filter: vec![
            Predicate::AtSince("2026-03-01T00:00:00Z".into()),
            Predicate::AtBefore("2026-04-01T00:00:00Z".into()),
        ],
        fields: None,
        include: Include::default(),
        aggregate: Some(Aggregate {
            op: AggregateOp::Sum,
            by,
        }),
        order: vec![],
        limit: None,
    }
}

pub(super) fn bucket<'a>(rows: &'a [Value], group: &str) -> &'a Value {
    rows.iter()
        .find(|r| r["group"] == group)
        .unwrap_or_else(|| panic!("no `{group}` bucket in {rows:#?}"))
}

pub(super) async fn spending_month(e: &Engine) {
    let cost = store::concepts::create(&e.store.pool, "cost", &[])
        .await
        .expect("create fixture concept");
    for name in ["food", "rent"] {
        store::concepts::create(&e.store.pool, name, &[&cost])
            .await
            .expect("create child concept");
    }
    store::concepts::create(&e.store.pool, "balance", &[])
        .await
        .expect("create fixture concept");
    record(e, "checking", "balance", None).await;

    capture(e, "checking", "-1000", "rent", "2026-03-01T09:00:00Z").await;
    capture(e, "checking", "-10.50", "food", "2026-03-05T12:00:00Z").await;
    capture(e, "checking", "-20.25", "food", "2026-03-06T12:00:00Z").await;
    capture(e, "checking", "10.50", "food", "2026-03-07T12:00:00Z").await;
    capture(e, "checking", "-999", "food", "2026-04-02T12:00:00Z").await;
}
