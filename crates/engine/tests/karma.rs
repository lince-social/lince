//! Integration tests for the engine (blueprint Stages 1–2 acceptance).

use chrono::{DateTime, TimeDelta, Utc};
use engine::Engine;
use nucleus::karma::{Cadence, Consequence};
use nucleus::{NewFact, RecordKind};

mod support;
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
        e.append_user(&uid, quantity).await.expect("a starting level");
    }
    uid
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
            .unwrap()
            .map(|q| q.to_f64()),
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
            .unwrap()
            .map(|q| q.to_f64()),
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

/// A rule that reads something, in the one shape rules have.
///
/// Anchored just before the moment under test, so exactly one date is due:
/// these are tests about what a rule decides, not about how many dates a
/// sixty-day catch-up window contains.
async fn watching(
    e: &Engine,
    target: &str,
    condition: &str,
    gate: &str,
    carry: &str,
    consequences: Vec<Consequence>,
) -> String {
    let anchor = (Utc::now() - TimeDelta::minutes(1)).to_rfc3339();
    support::declare_rule(
        e,
        target,
        Cadence::every_days(1),
        &anchor,
        Some(condition),
        Some(gate),
        Some(carry),
        consequences,
    )
    .await
}

fn dec(text: &str) -> nucleus::DecimalValue {
    nucleus::DecimalValue::parse_inferred(text).expect("an exact number")
}

async fn level(e: &Engine, uid: &str) -> f64 {
    store::facts::level(&e.store.pool, uid)
        .await
        .unwrap()
        .to_f64()
}

#[tokio::test]
async fn a_rule_fires_the_moment_the_world_changes_and_says_why() {
    // The reactive half. A rule watching a level must act when the level moves,
    // not on the next beat — "when stock drops below three" has to mean the
    // moment it drops. And the Fact it writes has to name the rule, or an
    // automatic change is a change nobody can account for.
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 8.0).await;
    let alert = plain(&e, "alerts.low-apples", 0.0).await;

    let declared = watching(
        &e,
        &alert,
        "@apples.stock",
        "<3",
        "one",
        vec![Consequence::SetQuantity { value: Some(dec("1")) }],
    )
    .await;

    // Still plenty: the gate blocks and nothing moves.
    e.append_user(&apples, -3.0).await.unwrap();
    assert_eq!(level(&e, &alert).await, 0.0, "a blocked gate must not act");

    // Down to 2, and the alert raises itself with no beat in between.
    e.append_user(&apples, -3.0).await.unwrap();
    assert_eq!(level(&e, &alert).await, 1.0, "the change itself must fire it");

    // The change is an ordinary entry — deliberately, so a balance reads the
    // same whether a person or a rule moved it. What makes it accountable is
    // that the date it answered is now spent: the rule can say which of its
    // occurrences produced this, and cannot produce it twice.
    let rule = store::recurrence::get(&e.store.pool, &declared)
        .await
        .unwrap()
        .expect("the rule is stored");
    let dates = store::recurrence::occurrences(
        &e.store.pool,
        &rule,
        Utc::now() - TimeDelta::days(1),
        Utc::now() + TimeDelta::days(1),
        Utc::now(),
    )
    .await
    .unwrap();
    assert!(
        dates
            .iter()
            .any(|date| date.state == store::recurrence::OccurrenceState::Applied),
        "the date the rule answered must be recorded as spent"
    );
}

#[tokio::test]
async fn a_rule_acts_at_most_once_per_period_however_often_it_is_poked() {
    // What used to be a `debounce` column. It is the cadence now: a rule may
    // act on its dates, and a date is spent once. So the thing that decides how
    // often a rule may fire is the same thing that decides when it fires,
    // declared in one place instead of two that could disagree.
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 10.0).await;
    let counter = plain(&e, "counter", 0.0).await;

    watching(
        &e,
        &counter,
        "@apples.stock",
        ">0",
        "one",
        vec![Consequence::AddQuantity { delta: Some(dec("1")) }],
    )
    .await;

    for _ in 0..5 {
        e.append_user(&apples, -1.0).await.unwrap();
    }
    assert_eq!(
        level(&e, &counter).await,
        1.0,
        "five pokes inside one day are one act"
    );
}

#[tokio::test]
async fn a_rule_can_be_read_as_a_named_cell() {
    // `value(@x)`: one rule computes a number and others read it, instead of
    // each restating the formula and drifting apart at the first edit. The
    // gate of the rule being read is deliberately ignored — reading what a
    // rule computes is not the same as letting it act.
    let e = engine().await;
    let _income = plain(&e, "income", 100.0).await;
    let budget = plain(&e, "budget", 0.0).await;
    let mirror = plain(&e, "mirror", 0.0).await;

    // A cell: half of income. Its own gate would block, and that must not
    // stop another rule from reading the number.
    watching(
        &e,
        &budget,
        "@income / 2",
        "<0",
        "value",
        vec![Consequence::SetQuantity { value: Some(dec("0")) }],
    )
    .await;
    watching(
        &e,
        &mirror,
        "value(@budget)",
        "always",
        "value",
        vec![Consequence::CaptureEntry {
            amount: dec("0"),
            concept: None,
        }],
    )
    .await;

    e.fire_due_rules(Utc::now()).await.unwrap();
    assert_eq!(
        level(&e, &mirror).await,
        50.0,
        "the cell's arithmetic must be readable even when its own gate blocks"
    );
}

#[tokio::test]
async fn the_two_flow_directions_can_be_read_apart() {
    // `sum_pos` and `sum_neg` over a window. What came in and what went out are
    // different questions, and a rule that could only see the net would answer
    // neither.
    let e = engine().await;
    let account = plain(&e, "account", 0.0).await;
    let inflow = plain(&e, "inflow", 0.0).await;

    e.append_user(&account, 100.0).await.unwrap();
    e.append_user(&account, -30.0).await.unwrap();
    e.append_user(&account, 50.0).await.unwrap();

    watching(
        &e,
        &inflow,
        "sum_pos(@account, 30d)",
        "always",
        "value",
        vec![Consequence::CaptureEntry {
            amount: dec("0"),
            concept: None,
        }],
    )
    .await;

    e.fire_due_rules(Utc::now()).await.unwrap();
    assert_eq!(
        level(&e, &inflow).await,
        150.0,
        "only what came in, not the net of 120"
    );
}

#[tokio::test]
async fn a_paused_rule_stops_acting_and_stops_being_read() {
    // Pausing means "stop acting for me", and it has to be complete: a paused
    // rule must not fire, and must not have its rhythm counted by somebody
    // else's arithmetic either.
    let e = engine().await;
    let apples = plain(&e, "apples.stock", 10.0).await;
    let counter = plain(&e, "counter", 0.0).await;

    let rule = watching(
        &e,
        &counter,
        "@apples.stock",
        ">0",
        "one",
        vec![Consequence::AddQuantity { delta: Some(dec("1")) }],
    )
    .await;

    e.act(
        engine::actions::Action::SetRecurrencePaused {
            recurrence: rule,
            expected_revision: 1,
            request_id: nucleus::new_uid("req"),
            paused: true,
        },
        None,
    )
    .await
    .unwrap();

    e.append_user(&apples, -1.0).await.unwrap();
    e.fire_due_rules(Utc::now()).await.unwrap();
    assert_eq!(level(&e, &counter).await, 0.0, "a paused rule acts for nobody");
}

// ------------------------------------------------------- outward consequences

#[tokio::test]
async fn a_rule_can_propose_an_obligation_instead_of_moving_a_number() {
    // A promise is the honest shape for "this is expected": it projects, it can
    // be kept or broken, and nothing has moved until it is kept.
    let e = engine().await;
    let rent = plain(&e, "rent", 0.0).await;

    support::declare_rule(
        &e,
        &rent,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        None,
        None,
        None,
        vec![Consequence::EmitPromise {
            delta: Some(dec("-1200")),
            window_end: Some("2026-04-01T00:00:00Z".into()),
            party: None,
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    let promises = store::misc::list_promises(&e.store.pool).await.unwrap();
    assert!(
        promises.iter().any(|p| p.delta == -1200.0),
        "the rule must have proposed the obligation"
    );
    assert_eq!(
        level(&e, &rent).await,
        0.0,
        "and must not have moved the number yet"
    );
}

#[tokio::test]
async fn a_rule_can_ask_instead_of_deciding() {
    // The one consequence that deliberately does not decide. Automation that
    // can ask is what lets a rule handle the cases it should not settle alone.
    let e = engine().await;
    let stock = plain(&e, "stock", 0.0).await;

    support::declare_rule(
        &e,
        &stock,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        None,
        None,
        None,
        vec![Consequence::Ask {
            question: Some("Reorder?".into()),
            options: vec!["yes".into(), "later".into()],
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    let decisions = store::misc::list_decisions(&e.store.pool).await.unwrap();
    assert!(
        decisions.iter().any(|d| d.question == "Reorder?"),
        "the question must be waiting in the queue"
    );
}

#[tokio::test]
async fn what_leaves_the_cell_is_queued_rather_than_run_mid_evaluation() {
    // A rule that shelled out inside its own evaluation could change the world
    // and then have its transaction rolled back, and would leave nowhere to
    // check a grant. So it commits an effect and a separate worker carries it.
    let e = engine().await;
    let watched = plain(&e, "watched", 0.0).await;

    support::declare_rule(
        &e,
        &watched,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        None,
        None,
        None,
        vec![Consequence::RunCommand {
            command: "echo hello".into(),
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    let queued = store::misc::due_effects(&e.store.pool).await.unwrap();
    assert!(
        queued.iter().any(|effect| effect.kind == "command"),
        "the command must be queued, not already run"
    );
}

#[tokio::test]
async fn an_outward_payload_that_is_not_readable_is_refused_where_it_is_written() {
    // Not at 3am inside a heartbeat with nobody watching.
    let e = engine().await;
    let watched = plain(&e, "watched", 0.0).await;
    let refused = e
        .act(
            engine::actions::Action::CreateRecurrence {
                target: watched,
                consequences: vec![Consequence::RunCommand { command: "  ".into() }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: None,
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a consequence with nothing to run must not store");
}
