//! Stage 1/2 completion tests: focus queue (Window 1b), signal sampling,
//! checkpoints, and the Lingua concept DAG.

use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::{CauseKind, RecordKind};
use nucleus::karma::{Cadence, Consequence};

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

// The focus-queue acceptance test lives in `protein/tests` now — ordering by
// links is Protein's `topo` (blueprint Window 1b), not an engine query.

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

    // identity is the TRIPLE: both links between the same two records coexist
    store::links::add(&e.store.pool, &small, &before, &big, None)
        .await
        .unwrap();
    store::links::add(&e.store.pool, &small, &part_of, &big, Some(0.2))
        .await
        .unwrap();

    // but duplicating the exact same triple fails
    assert!(
        store::links::add(&e.store.pool, &small, &before, &big, None)
            .await
            .is_err()
    );

    // each kind is its own graph
    assert_eq!(
        store::links::edges_of_kind(&e.store.pool, &before)
            .await
            .unwrap()
            .len(),
        1
    );
    let part = store::links::edges_of_kind(&e.store.pool, &part_of)
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

    // `concept_in @food` matches @apple through the parent DAG
    let family = store::concepts::descendants_including(&e.store.pool, &food)
        .await
        .unwrap();
    assert!(family.contains(&apple) && family.contains(&fruit));

    // multilingual names resolve to the same concept
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
    // The sample lands as a Signal fact, and the rule reading it reacts on the
    // same pass. The rule's own change is an ordinary entry — what records
    // that a rule made it is the occurrence it spent, not a second cause kind.
    assert!(facts.iter().any(|f| f.cause.kind == CauseKind::Signal));
    assert_eq!(
        store::records::quantity(&e.store.pool, &alert)
            .await
            .unwrap().map(|q| q.to_f64()),
        Some(1.0)
    );

    // within the schedule window nothing re-samples; unchanged value makes no noise
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
    // The level is canonical decimal TEXT, not a JSON float: after compaction
    // this payload IS the record's level (blueprint E0.0).
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
            .unwrap().map(|q| q.to_f64()),
        Some(5.0),
        "checkpoint changes nothing"
    );

    // idempotent sweep: already-anchored records are skipped
    let again = e.checkpoint_all(Utc::now()).await.unwrap();
    assert!(again.is_empty());
}
