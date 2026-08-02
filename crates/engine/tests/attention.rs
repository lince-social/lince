//! Parts XII–XIII completion: the projected-crossing sweep, decision expiry,
//! the notify budget/digest, and branching (compare two futures).

use chrono::{DateTime, Utc};
use engine::Engine;
use nucleus::karma::{Cadence, Consequence};
use nucleus::{PromiseState, RecordKind};

mod support;
use store::misc::NewPromise;
use store::records::NewRecord;

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine opens")
}

/// A Record seeded through the Ledger, not around it.
///
/// Writing a starting level straight into the cache used to be harmless
/// because rules read the cache. They read the Fact chain now — the Ledger is
/// the truth and the cache is derived from it — so a fixture that skipped the
/// chain would set up a world the rule cannot see.
async fn plain(e: &Engine, slug: &str, quantity: f64) -> String {
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
    if quantity != 0.0 {
        e.append_user(&uid, quantity)
            .await
            .expect("a starting level");
    }
    uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

#[tokio::test]
async fn projected_crossings_enqueue_decisions_once() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 2.0).await;
    plain(&e, "hammer", 5.0).await; // no commitments: never crosses

    // an active commitment takes 5 apples in three days -> projected -3
    store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: Some(apples.clone()),
            delta: -5.0,
            window_end: Some("2026-07-14T00:00:00Z".into()),
            state: Some(PromiseState::Active),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let now = at("2026-07-11T00:00:00Z");
    let created = e.crossings_pass(now).await.unwrap();
    assert_eq!(created.len(), 1, "only the record that will cross");

    let decisions = store::misc::open_decisions(&e.store.pool).await.unwrap();
    let crossing: Vec<_> = decisions
        .iter()
        .filter(|(_, kind, _)| kind == "crossing")
        .collect();
    assert_eq!(crossing.len(), 1);
    assert!(crossing[0].2.contains("apples.stock"), "{}", crossing[0].2);

    // heartbeat is idempotent about it
    e.heartbeat(now).await.unwrap();
    assert_eq!(
        store::misc::open_decisions(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .filter(|(_, kind, _)| kind == "crossing")
            .count(),
        1
    );
}

#[tokio::test]
async fn expired_decisions_close_through_the_ledger() {
    let e = engine().await;
    plain(&e, "subject", 0.0).await;
    let subject = store::records::resolve(&e.store.pool, "subject")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let decision = store::misc::create_decision_expiring(
        &e.store.pool,
        &subject,
        "ask",
        "still relevant?",
        &serde_json::json!([{ "label": "yes" }]),
        "2026-07-10T00:00:00Z",
    )
    .await
    .unwrap();

    e.heartbeat(at("2026-07-11T00:00:00Z")).await.unwrap();

    assert_eq!(
        store::records::quantity(&e.store.pool, &decision)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "expired decision closed"
    );
    assert!(
        store::misc::open_decisions(&e.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn notify_budget_parks_overflow_in_the_digest() {
    let e = engine().await;
    let x = plain(&e, "x", 0.0).await;
    store::config::set_attention_budget(&e.store.pool, 1)
        .await
        .unwrap();
    support::declare_rule(
        &e,
        &x,
        Cadence::every_days(1),
        "2026-01-01T00:00:00Z",
        Some("@x"),
        Some("!=0"),
        Some("one"),
        vec![Consequence::Notify {
            message: Some("ping".into()),
        }],
    )
    .await;

    // A second rule watching the same Record. Two rules rather than one rule
    // fired twice, because a rule may now act only once per period — which is
    // the debounce, and is not what this test is about.
    support::declare_rule(
        &e,
        &x,
        Cadence::every_days(1),
        "2026-01-01T00:00:00Z",
        Some("@x"),
        Some("!=0"),
        Some("one"),
        vec![Consequence::Notify {
            message: Some("pong".into()),
        }],
    )
    .await;

    // One change, two rules, two notifications — and a budget of one.
    e.append_user(&x, 1.0).await.unwrap();
    let delivered = e.run_due_effects().await.unwrap();
    assert_eq!(delivered.len(), 2, "both rules queued a notification");
    assert!(
        !delivered[0].result.starts_with("parked"),
        "the first is within budget"
    );
    assert!(
        delivered[1].result.starts_with("parked:digest"),
        "the second is over budget: parked, not delivered ({})",
        delivered[1].result
    );
}

#[tokio::test]
async fn deciding_executes_the_chosen_options_action() {
    let e = engine().await;
    plain(&e, "subject", 0.0).await;
    plain(&e, "counter", 0.0).await;
    let subject = store::records::resolve(&e.store.pool, "subject")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let decision = store::misc::create_decision(
        &e.store.pool,
        &subject,
        "ask",
        "bump the counter?",
        &serde_json::json!([
            {
                "label": "yes",
                "action": { "action": "set-quantity", "target": "counter", "value": 7.0 },
            },
            { "label": "no" },
        ]),
    )
    .await
    .unwrap();

    e.act(
        engine::actions::Action::Decide {
            decision: decision.clone(),
            answer: "yes".into(),
        },
        None,
    )
    .await
    .unwrap();

    let counter = store::records::resolve(&e.store.pool, "counter")
        .await
        .unwrap()
        .unwrap()
        .uid;
    assert_eq!(
        store::records::quantity(&e.store.pool, &counter)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(7.0),
        "the chosen option's Action ran"
    );
    assert_eq!(
        store::records::quantity(&e.store.pool, &decision)
            .await
            .unwrap()
            .map(|q| q.to_f64()),
        Some(0.0),
        "and the decision closed"
    );
}

#[tokio::test]
async fn branching_compares_two_futures() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 2.0).await;
    store::misc::insert_promise(
        &e.store.pool,
        NewPromise {
            record_uid: Some(apples.clone()),
            delta: -5.0,
            window_end: Some("2026-07-14T00:00:00Z".into()),
            state: Some(PromiseState::Active),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let now = at("2026-07-11T00:00:00Z");
    let until = at("2026-07-20T00:00:00Z");

    // branch A: the world as it is
    let base = e.snapshot(now).await.unwrap();
    let a = nucleus::imagination::project(&base, until);

    // branch B: drag the promise away (withdrawn) and re-fold
    let mut branched = base.clone();
    branched.promises.clear();
    let b = nucleus::imagination::project(&branched, until);

    assert_eq!(a.projected(&apples), Some(-3.0));
    assert_eq!(b.projected(&apples), Some(2.0));
    assert!(a.crossing_below(&apples, 0.0).is_some());
    assert!(b.crossing_below(&apples, 0.0).is_none());
}
