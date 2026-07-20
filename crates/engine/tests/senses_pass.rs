//! Parts IX–X completion: the demand token, and the senses heartbeat arm —
//! match rules as records, drafts landing in the Decision Queue.

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::{Cause, ConsequenceKind, NewFact, PromiseState, RecordKind};
use store::records::NewRecord;
use store::senses::RemoteOpenRow;

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
            quantity,
        },
    )
    .await
    .expect("record")
    .uid
}

fn at(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn remote(promise_uid: &str, concept: Option<&str>, delta: f64, proximity: u32) -> RemoteOpenRow {
    RemoteOpenRow {
        promise_uid: promise_uid.into(),
        organ: "organ.bakery".into(),
        proximity,
        concept: concept.map(String::from),
        unit: None,
        delta,
        window_start: None,
        window_end: None,
        confidence: 0.8,
    }
}

#[tokio::test]
async fn demand_token_samples_the_hourly_histogram() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let apples = plain(&e, "apples.stock", 0.0).await;
    store::records::set_concept(&e.store.pool, &apples, Some(&food))
        .await
        .unwrap();
    plain(&e, "mirror", 0.0).await;

    // three facts at 08:xx, one at 20:xx -> demand at an 08:xx now is 0.75
    for (i, hour) in [(1, 8), (2, 8), (3, 8), (4, 20)] {
        e.append(
            NewFact::quantity(apples.clone(), 1.0, Cause::user_edit()),
            at(&format!("2026-07-0{i}T{hour:02}:15:00Z")),
        )
        .await
        .unwrap();
    }

    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.demand-mirror",
            head: "Demand mirror",
            condition: "demand(@food) * (@apples.stock >= 0)",
            gate: "always",
            carry: "value",
            consequences: vec![(ConsequenceKind::SetQuantity, Some("@mirror".into()), None)],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    // trigger an evaluation at 08:30 — demand(@food) = 3/5 of facts so far...
    // careful: this append itself lands at 08:30 and counts (4 of 6 at 08).
    e.append(
        NewFact::quantity(apples.clone(), 1.0, Cause::user_edit()),
        at("2026-07-05T08:30:00Z"),
    )
    .await
    .unwrap();
    let mirror = store::records::resolve(&e.store.pool, "mirror")
        .await
        .unwrap()
        .unwrap()
        .uid;
    let value = store::records::quantity(&e.store.pool, &mirror)
        .await
        .unwrap()
        .unwrap();
    assert!(
        (value - 4.0 / 5.0).abs() < 1e-9,
        "4 of 5 facts in the 08 hour, got {value}"
    );
}

#[tokio::test]
async fn senses_heartbeat_arm_drafts_decisions_once() {
    let e = engine().await;
    let food = store::concepts::create(&e.store.pool, "food", &[])
        .await
        .unwrap();
    let apples = plain(&e, "apples.stock", -3.0).await;
    store::records::set_concept(&e.store.pool, &apples, Some(&food))
        .await
        .unwrap();

    // my published Need: an OPEN promise wanting -3 filled
    let local = store::misc::insert_promise(
        &e.store.pool,
        store::misc::NewPromise {
            record_uid: Some(apples.clone()),
            delta: -3.0,
            state: Some(PromiseState::Open),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // the discovery cache knows a complementary offer nearby and one too far
    store::senses::upsert_remote_open(&e.store.pool, &remote("p_R1", Some(&food), 5.0, 1))
        .await
        .unwrap();
    store::senses::upsert_remote_open(&e.store.pool, &remote("p_FAR", Some(&food), 5.0, 9))
        .await
        .unwrap();

    // a match rule as a record, created through the Action catalog
    let rule = e
        .act(
            Action::CreateMatchRule {
                slug: "senses.food-nearby".into(),
                head: "Food nearby".into(),
                watch_concept: Some("food".into()),
                max_proximity: 1,
                min_confidence: 0.5,
                auto: "draft_only".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    let created = e.senses_pass().await.unwrap();
    assert_eq!(
        created.len(),
        1,
        "one draft: the far organ is ceilinged out"
    );

    let decisions = store::misc::open_decisions(&e.store.pool).await.unwrap();
    let drafts: Vec<_> = decisions
        .iter()
        .filter(|(_, kind, _)| kind == "draft")
        .collect();
    assert_eq!(drafts.len(), 1);
    assert!(drafts[0].2.contains("organ.bakery"));

    // the pass is idempotent: same situation never asks twice
    let again = e.senses_pass().await.unwrap();
    assert!(again.is_empty());

    // heartbeat runs the arm too (fresh cache row, same dedup)
    let more = e.heartbeat(at("2026-07-11T12:00:00Z")).await.unwrap();
    let _ = more;
    assert_eq!(
        store::misc::open_decisions(&e.store.pool)
            .await
            .unwrap()
            .iter()
            .filter(|(_, kind, _)| kind == "draft")
            .count(),
        1
    );

    // deactivating the match rule (a record like any other) stops the matcher
    e.act(Action::Deactivate { target: rule }, None)
        .await
        .unwrap();
    store::senses::upsert_remote_open(&e.store.pool, &remote("p_R2", Some(&food), 3.0, 1))
        .await
        .unwrap();
    assert!(e.senses_pass().await.unwrap().is_empty());
    let _ = local;
}
