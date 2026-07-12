//! Integration tests for the engine (blueprint Stages 1–2 acceptance).

use chrono::{DateTime, TimeDelta, Utc};
use engine::Engine;
use nucleus::{Cause, CauseKind, ConsequenceKind, NewFact, RecordKind};
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

#[tokio::test]
async fn append_updates_cache_and_is_idempotent() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;

    let facts = e.append_user(&apples, -1.0).await.unwrap();
    assert_eq!(facts.len(), 1);
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples)
            .await
            .unwrap(),
        Some(7.0)
    );

    // replaying the same fact uid is a no-op success (sync replay safety)
    let replay = NewFact {
        uid: Some(facts[0].uid.clone()),
        ..facts[0].clone().into_new()
    };
    let again = e.append(replay, Utc::now()).await.unwrap();
    assert!(again.is_empty());
    assert_eq!(
        store::records::quantity(&e.store.pool, &apples)
            .await
            .unwrap(),
        Some(7.0)
    );

    // hash chain holds
    let log = store::facts::for_record(&e.store.pool, &apples, 10)
        .await
        .unwrap();
    assert!(log.iter().all(nucleus::fact::verify_chain_step));
}

// helper: NewFact from an existing Fact (test-only replay shape)
trait IntoNew {
    fn into_new(self) -> NewFact;
}
impl IntoNew for nucleus::Fact {
    fn into_new(self) -> NewFact {
        NewFact {
            uid: Some(self.uid),
            record_uid: self.record_uid,
            delta: self.delta,
            at: Some(self.at),
            actor_uid: self.actor_uid,
            cause: self.cause,
            payload: self.payload,
        }
    }
}

#[tokio::test]
async fn rule_fires_on_change_with_provenance() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;
    let alert = plain(&e, "alerts.low-apples", 0.0).await;

    // when apples drop below 3, set the alert record to 1 (carry=one)
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.low-apples",
            head: "Low apples",
            condition: "@apples.stock",
            gate: "<3",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::SetQuantity,
                Some("@alerts.low-apples".into()),
                None,
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    // 8 -> 5: gate blocks
    e.append_user(&apples, -3.0).await.unwrap();
    assert_eq!(
        store::records::quantity(&e.store.pool, &alert)
            .await
            .unwrap(),
        Some(0.0)
    );

    // 5 -> 2: fires, alert = 1, cause = rule:<uid>
    let facts = e.append_user(&apples, -3.0).await.unwrap();
    assert_eq!(
        store::records::quantity(&e.store.pool, &alert)
            .await
            .unwrap(),
        Some(1.0)
    );
    let rule_fact = facts
        .iter()
        .find(|f| f.record_uid == alert)
        .expect("cascade fact");
    assert_eq!(rule_fact.cause.kind, CauseKind::Rule);
    assert!(
        rule_fact.cause.uid.is_some(),
        "automation always answers why"
    );
}

#[tokio::test]
async fn derived_value_rules_are_spreadsheet_cells() {
    let e = engine().await;
    plain(&e, "x", 4.0).await;
    let y = plain(&e, "y", 0.0).await;

    // zero consequences = named derived value (blueprint VI.3)
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.double-x",
            head: "x * 2",
            condition: "@x * 2",
            gate: "always",
            carry: "value",
            consequences: vec![],
        },
    )
    .await
    .unwrap();
    // consumer references it via value()
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.apply",
            head: "y = double-x + 1",
            condition: "value(@rules.double-x) + 1",
            gate: "always",
            carry: "value",
            consequences: vec![(ConsequenceKind::SetQuantity, Some("@y".into()), None)],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    let x_uid = store::records::resolve(&e.store.pool, "x")
        .await
        .unwrap()
        .unwrap()
        .uid;
    e.append_user(&x_uid, 1.0).await.unwrap(); // x: 4 -> 5
    assert_eq!(
        store::records::quantity(&e.store.pool, &y).await.unwrap(),
        Some(11.0),
        "y = (5 * 2) + 1"
    );
}

#[tokio::test]
async fn frequency_tick_drives_the_daily_habit() {
    let e = engine().await;
    let exercise = plain(&e, "exercise", 0.0).await;
    store::freqs::create(
        &e.store.pool,
        store::freqs::NewFrequency {
            slug: "freq.daily-7am",
            head: "Daily 7am",
            seconds: 0,
            days: 1,
            months: 0,
            day_of_week: None,
            next_at: at("2026-07-05T07:00:00Z"),
            catch_up: false,
        },
    )
    .await
    .unwrap();
    // the LINCE.md classic: -1 * freq, '=' (!=0), consequence sets the Need
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.daily-exercise",
            head: "Daily exercise",
            condition: "-1 * freq(@freq.daily-7am)",
            gate: "!=0",
            carry: "value",
            consequences: vec![(ConsequenceKind::SetQuantity, Some("@exercise".into()), None)],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    let now = at("2026-07-05T10:00:00Z");
    let facts = e.tick(now).await.unwrap();
    assert!(!facts.is_empty());
    assert_eq!(
        store::records::quantity(&e.store.pool, &exercise)
            .await
            .unwrap(),
        Some(-1.0),
        "exercise became a Need"
    );

    // next tick same day: frequency advanced past now, nothing fires
    let facts = e.tick(now + TimeDelta::minutes(5)).await.unwrap();
    assert!(facts.is_empty());
}

#[tokio::test]
async fn rules_emit_promises_previewable_automation() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;

    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.reorder",
            head: "Reorder apples",
            condition: "@apples.stock",
            gate: "<3",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::EmitPromise,
                Some("@apples.stock".into()),
                Some(serde_json::json!({ "delta": 5.0 })),
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&apples, -6.0).await.unwrap(); // 8 -> 2, fires
    let promises = store::misc::promises_for_record(&e.store.pool, &apples)
        .await
        .unwrap();
    assert_eq!(promises.len(), 1);
    assert_eq!(promises[0].delta, 5.0);
    assert_eq!(promises[0].state, nucleus::PromiseState::Proposed);
    assert!(
        promises[0].rule_uid.is_some(),
        "automation-born promises name their rule"
    );
}

#[tokio::test]
async fn ask_consequence_enqueues_a_decision() {
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 2.0).await;
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.reorder-ask",
            head: "Ask before reorder",
            condition: "@apples.stock",
            gate: "<3",
            carry: "one",
            consequences: vec![(
                ConsequenceKind::Ask,
                None,
                Some(serde_json::json!({ "question": "send reorder proposal?" })),
            )],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&apples, -1.0).await.unwrap();
    let open = store::misc::open_decisions(&e.store.pool).await.unwrap();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].2, "send reorder proposal?");
}

#[tokio::test]
async fn proof_warns_on_loops_and_cascade_cap_survives_them() {
    let e = engine().await;
    let a = plain(&e, "a", 0.0).await;
    plain(&e, "b", 0.0).await;

    // A: when a changes, b += 1 ; B: when b changes, a += 1 — a deliberate loop
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.a-to-b",
            head: "a->b",
            condition: "@a",
            gate: "always",
            carry: "one",
            consequences: vec![(ConsequenceKind::AddQuantity, Some("@b".into()), None)],
        },
    )
    .await
    .unwrap();
    store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.b-to-a",
            head: "b->a",
            condition: "@b",
            gate: "always",
            carry: "one",
            consequences: vec![(ConsequenceKind::AddQuantity, Some("@a".into()), None)],
        },
    )
    .await
    .unwrap();

    // Proof: the loop is announced at load (blueprint VI.4)
    let warnings = e.reload_rules().await.unwrap();
    assert!(
        warnings.iter().any(|w| w.message.contains("loop")),
        "Proof names the loop: {warnings:?}"
    );

    // and the delivery cap keeps the engine alive through it
    let facts = e
        .append(NewFact::quantity(&a, 1.0, Cause::user_edit()), Utc::now())
        .await
        .unwrap();
    assert!(!facts.is_empty());
    assert!(facts.len() <= 300, "cascade is capped, not infinite");
}

#[tokio::test]
async fn effects_run_outside_evaluation_with_provenance() {
    let e = engine().await;
    let trigger = plain(&e, "trigger", 0.0).await;
    let rule_uid = store::rules::create(
        &e.store.pool,
        store::rules::NewRule {
            slug: "rules.echo",
            head: "Echo",
            condition: "@trigger",
            gate: "!=0",
            carry: "value",
            consequences: vec![(ConsequenceKind::RunCommand, Some("echo lince".into()), None)],
        },
    )
    .await
    .unwrap();
    e.reload_rules().await.unwrap();

    e.append_user(&trigger, 1.0).await.unwrap();
    let outcomes = e.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].ok);
    assert_eq!(outcomes[0].result, "lince");

    // result logged as a zero-delta provenance fact on the rule record
    let log = store::facts::for_record(&e.store.pool, &rule_uid, 5)
        .await
        .unwrap();
    assert!(
        log.iter()
            .any(|f| f.delta == 0.0 && f.cause.kind == CauseKind::Action)
    );
}
