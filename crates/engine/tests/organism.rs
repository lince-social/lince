use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::karma::{Cadence, Consequence};
use nucleus::{CauseKind, RecordKind};

mod support;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
    store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some(slug),
            kind: RecordKind::Plain,
            head: slug,
            body: "",
            quantity: store::exact::from_f64(quantity),
        },
    )
    .await
    .expect("record")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[tokio::test]
async fn same_pair_carries_many_link_kinds() {
    let e = engine().await;
    let small = plain(&e, "small-step", -1.0).await;
    let big = plain(&e, "big-step", -1.0).await;
    let before = store::concepts::create(&e.store.pool, "before", &[])
        .await
        .unwrap();
    let part_of = store::concepts::create(&e.store.pool, "part-of", &[])
        .await
        .unwrap();

    let first = store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &small,
            predicate_uid: &before,
            object_uid: Some(&big),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &small,
            predicate_uid: &part_of,
            object_uid: Some(&big),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: Some(store::exact::from_f64(0.2)),
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();

    let repeated = store::assertions::assert(
        &e.store.pool,
        store::assertions::NewAssertion {
            subject_uid: &small,
            predicate_uid: &before,
            object_uid: Some(&big),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(repeated, first);

    assert_eq!(
        store::assertions::edges_of_predicate(&e.store.pool, &before)
            .await
            .unwrap()
            .len(),
        1
    );
    let part = store::assertions::edges_of_predicate(&e.store.pool, &part_of)
        .await
        .unwrap();
    assert_eq!(part[0].quantity, Some(0.2));
}

#[tokio::test]
async fn concept_dag_widens_matching() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let fruit = store::concepts::create(&e.store.pool, "fruit", &[&food])
        .await
        .unwrap();
    let apple = store::concepts::create(&e.store.pool, "apple", &[&fruit])
        .await
        .unwrap();
    store::concepts::add_name(&e.store.pool, &apple, "pt-br", "Maçã")
        .await
        .unwrap();

    let family = store::concepts::descendants_including(&e.store.pool, &food)
        .await
        .unwrap();
    assert!(family.contains(&apple) && family.contains(&fruit));

    assert_eq!(
        store::concepts::resolve(&e.store.pool, "Maçã")
            .await
            .unwrap(),
        Some(apple)
    );
}

#[tokio::test]
async fn signals_sample_the_world_and_cascade() {
    let e = engine().await;
    let alert = plain(&e, "alerts.many-books", 0.0).await;
    store::misc::create_signal(
        &e.store.pool,
        store::misc::NewSignal {
            slug: "signals.books-count",
            head: "Tech books count",
            source_kind: "command",
            source: "echo 42",
            schedule: "60s",
        },
    )
    .await
    .unwrap();
    support::declare_rule(
        &e,
        "@alerts.many-books",
        Cadence::every_days(1),
        "2026-01-01T00:00:00Z",
        Some("signal(@signals.books-count) > 40"),
        Some("!=0"),
        Some("one"),
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
        }],
    )
    .await;

    let now = at("2026-07-05T10:00:00Z");
    let facts = e.sample_due_signals(now).await.unwrap();
    assert!(facts.iter().any(|f| f.cause.kind == CauseKind::Signal));
    assert_eq!(
        store::records::quantity(&e.store.pool, &alert)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(1.0)
    );

    let facts = e
        .sample_due_signals(at("2026-07-05T10:00:30Z"))
        .await
        .unwrap();
    assert!(facts.is_empty());
    let facts = e
        .sample_due_signals(at("2026-07-05T10:02:00Z"))
        .await
        .unwrap();
    assert!(
        facts.is_empty(),
        "same sampled value: no fact, no cascade, no noise"
    );
}

#[tokio::test]
async fn checkpoints_anchor_without_cascading() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;
    e.append_user(&apples, -3.0).await.unwrap();

    let checkpoints = e.checkpoint_all(Utc::now()).await.unwrap();
    assert_eq!(checkpoints.len(), 1);
    assert_eq!(checkpoints[0].delta, store::exact::zero());
    assert!(
        checkpoints[0]
            .payload
            .as_deref()
            .unwrap()
            .contains("\"level\":\"5\"")
    );
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(5.0),
        "checkpoint changes nothing"
    );

    let again = e.checkpoint_all(Utc::now()).await.unwrap();
    assert!(again.is_empty());
}
