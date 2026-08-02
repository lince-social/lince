//! Recurrence through the Actions a surface actually calls.
//!
//! A recurring rule declares; it does not move anything. What these defend is
//! the boundary between the two: applying a date writes an ordinary entry that
//! nothing downstream can tell apart from a hand-typed one, and applying it
//! twice is impossible.

use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use nucleus::karma::Consequence;
use nucleus::karma::{Cadence, CadenceStep, CivilWeekday, WeekdaySet};
use store::records::NewRecord;
use store::recurrence::{OccurrenceState, occurrence_request_id};

/// The one-consequence shape every rule had before a rule could do more than
/// move a number.
fn capture(amount: &str, concept: Option<&str>) -> Vec<Consequence> {
    vec![Consequence::CaptureEntry {
        amount: nucleus::DecimalValue::parse_inferred(amount).expect("exact amount"),
        concept: concept.map(str::to_string),
    }]
}

async fn engine() -> Engine {
    let engine = Engine::open_memory().await.expect("engine opens");
    store::organs::ensure_local(&engine.store.pool, "http://recurrence.test")
        .await
        .expect("local Organ");
    engine
}

/// A resource and a concept. The scenario reads as a running balance because
/// that is legible; nothing under test knows what it is counting.
async fn setup(e: &Engine) -> (String, String) {
    let rent = store::concepts::create(&e.store.pool, "rent", &[])
        .await
        .unwrap();
    let checking = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("checking"),
            kind: RecordKind::Plain,
            head: "checking",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    (checking, rent)
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("test timestamp should parse")
        .with_timezone(&Utc)
}

async fn monthly(e: &Engine, target: &str, concept: &str, amount: &str) -> String {
    e.act(
        Action::CreateRecurrence {
            target: target.to_string(),
            consequences: capture(amount, Some(concept)),
            condition: None,
            gate: None,
            carry: None,
            note: Some("rent".to_string()),
            cadence: Cadence::every_months(1),
            anchor_at: Some("2026-01-01T00:00:00Z".to_string()),
            request_id: Some("rule-1".to_string()),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .expect("a rule is created")
}

async fn level(e: &Engine, record_uid: &str) -> String {
    store::facts::level(&e.store.pool, record_uid)
        .await
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn declaring_a_rule_moves_nothing() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let outcome = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: capture("-1200", Some(&rent)),
                condition: None,
                gate: None,
                carry: None,
                note: Some("rent".to_string()),
                cadence: Cadence::every_months(1),
                anchor_at: Some("2026-01-01T00:00:00Z".to_string()),
                request_id: Some("rule-1".to_string()),
            },
            None,
        )
        .await
        .unwrap();

    // A declaration is not a change. If this ever appends a Fact, every
    // balance in the system starts counting a quantity nobody has moved yet.
    assert!(outcome.facts.is_empty(), "declaring must append no Fact");
    assert_eq!(level(&e, &checking).await, "0");
    assert!(outcome.created.is_some());
}

#[tokio::test]
async fn applying_an_occurrence_writes_an_ordinary_entry() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    let outcome = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.clone(),
                due_at: "2026-02-01T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(outcome.facts.len(), 1, "applying appends exactly one Fact");
    assert_eq!(level(&e, &checking).await, "-1200");

    // The entry is a normal entry: revisable and voidable like any other.
    let entry_uid = outcome.created.expect("an entry is created");
    let entry = store::entries::get(&e.store.pool, &entry_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.revision, 1);
    assert_eq!(entry.record_uid, checking);
    // The Fact carries the rule's classification, so it lands in the same
    // total a hand-typed rent would.
    let fact = &outcome.facts[0];
    let classification = store::ledger::fact_concept(&e.store.pool, &fact.uid)
        .await
        .unwrap();
    assert_eq!(classification.as_deref(), Some(rent.as_str()));
    // Occurred-at is the due date, not the moment somebody clicked.
    assert_eq!(
        entry.occurred_at,
        store::facts::instant(at("2026-02-01T00:00:00Z"))
    );
}

#[tokio::test]
async fn the_same_occurrence_cannot_be_applied_twice() {
    // The failure this prevents is the expensive one: a double-click, a retried
    // request, or two open surfaces paying one month's rent twice.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    let first = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.clone(),
                due_at: "2026-02-01T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();
    let second = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.clone(),
                due_at: "2026-02-01T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(first.facts.len(), 1);
    assert!(
        second.facts.is_empty(),
        "a replay must append nothing at all"
    );
    assert_eq!(level(&e, &checking).await, "-1200");
}

#[tokio::test]
async fn two_different_dates_of_one_rule_are_two_different_changes() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    for date in ["2026-02-01T00:00:00Z", "2026-03-01T00:00:00Z"] {
        e.act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.clone(),
                due_at: date.to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();
    }
    assert_eq!(level(&e, &checking).await, "-2400");
}

#[tokio::test]
async fn a_date_the_rule_does_not_produce_is_refused() {
    // Otherwise "apply" is just a capture wearing a rule's name, and the
    // timeline would show an applied date no cadence explains.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    let refused = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule,
                due_at: "2026-02-17T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "an invented date must not be applicable");
    assert_eq!(level(&e, &checking).await, "0");
}

#[tokio::test]
async fn one_occurrence_can_come_in_higher_than_the_standing_rule() {
    // The bill that was 1200 every month and is 1350 this month. Overriding one
    // date must not silently rewrite the rule for every future month.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
            amount: Some("-1350".to_string()),
            note: Some("rent plus water".to_string()),
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(level(&e, &checking).await, "-1350");
    let still = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        still
            .consequences
            .declared_delta()
            .expect("a capture rule declares an amount")
            .to_string(),
        "-1200",
        "one month's override must not edit the rule"
    );
}

#[tokio::test]
async fn an_applied_date_reports_what_it_actually_carried() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
            amount: Some("-1350".to_string()),
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    let stored = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    let found = store::recurrence::occurrences(
        &e.store.pool,
        &stored,
        at("2026-01-01T00:00:00Z"),
        at("2026-04-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();

    let february = found
        .iter()
        .find(|item| item.due_at == at("2026-02-01T00:00:00Z"))
        .expect("February is a date this rule produces");
    assert_eq!(february.state, OccurrenceState::Applied);
    // What moved is what is reported — not the rule's standing figure.
    assert_eq!(
        february
            .amount
            .expect("a capture occurrence carries an amount")
            .to_string(),
        "-1350"
    );
    assert!(february.entry_uid.is_some());
}

#[tokio::test]
async fn skipping_a_date_records_a_decision_rather_than_a_silence() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;

    e.act(
        Action::SkipRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
            note: Some("landlord waived it".to_string()),
        },
        None,
    )
    .await
    .unwrap();

    let stored = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    let found = store::recurrence::occurrences(
        &e.store.pool,
        &stored,
        at("2026-01-01T00:00:00Z"),
        at("2026-04-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(
        found.iter().map(|i| i.state.as_str()).collect::<Vec<_>>(),
        vec!["due", "skipped", "due"]
    );
    assert_eq!(level(&e, &checking).await, "0", "a skip moves nothing");

    e.act(
        Action::UnskipRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
        },
        None,
    )
    .await
    .unwrap();
    let after = store::recurrence::occurrences(
        &e.store.pool,
        &stored,
        at("2026-01-01T00:00:00Z"),
        at("2026-04-01T00:00:00Z"),
        at("2026-03-15T00:00:00Z"),
    )
    .await
    .unwrap();
    assert!(
        after
            .iter()
            .all(|item| item.state != OccurrenceState::Skipped)
    );
}

#[tokio::test]
async fn revising_a_rule_leaves_what_already_ran_alone() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    e.act(
        Action::ReviseRecurrence {
            recurrence: rule.clone(),
            expected_revision: 1,
            request_id: "rule-revise-1".to_string(),
            consequences: capture("-1300", Some(&rent)),
            condition: None,
            gate: None,
            carry: None,
            note: Some("rent went up".to_string()),
            cadence: Cadence::every_months(1),
            anchor_at: None,
        },
        None,
    )
    .await
    .unwrap();

    // February already happened at 1200 and is a Fact. The rule now expects
    // 1300 from here on; neither statement corrects the other.
    assert_eq!(level(&e, &checking).await, "-1200");
    let stored = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored
            .consequences
            .declared_delta()
            .expect("a capture rule declares an amount")
            .to_string(),
        "-1300"
    );
    assert_eq!(stored.revision, 2);
    // The anchor was not silently reset, so future dates keep their phase.
    assert_eq!(
        stored.anchor_at,
        store::facts::instant(at("2026-01-01T00:00:00Z"))
    );
}

#[tokio::test]
async fn a_stale_rule_edit_is_refused() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    e.act(
        Action::SetRecurrencePaused {
            recurrence: rule.clone(),
            expected_revision: 1,
            request_id: "pause-1".to_string(),
            paused: true,
        },
        None,
    )
    .await
    .unwrap();

    let stale = e
        .act(
            Action::ReviseRecurrence {
                recurrence: rule.clone(),
                expected_revision: 1,
                request_id: "rule-revise-stale".to_string(),
                consequences: capture("-9999", Some(&rent)),
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_months(1),
                anchor_at: None,
            },
            None,
        )
        .await;
    assert!(stale.is_err(), "an edit against an old revision must lose");
}

#[tokio::test]
async fn a_rule_applied_change_is_correctable_like_any_other() {
    // The point of applying through the ordinary capture path: nothing
    // downstream needs to know a rule was involved.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    let entry = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule,
                due_at: "2026-02-01T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    e.act(
        Action::ReviseEntry {
            entry: entry.clone(),
            expected_revision: 1,
            request_id: "fix-1".to_string(),
            amount: "-1250".to_string(),
            note: Some("was actually 1250".to_string()),
            at: None,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(level(&e, &checking).await, "-1250");

    e.act(
        Action::VoidEntry {
            entry,
            expected_revision: 2,
            request_id: "void-1".to_string(),
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(level(&e, &checking).await, "0");
}

#[tokio::test]
async fn the_occurrence_key_is_what_makes_applying_idempotent() {
    // Documents the mechanism the design leans on, so a future change to the
    // key format cannot quietly remove the protection.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-02-01T00:00:00Z".to_string(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    let key = occurrence_request_id(&rule, at("2026-02-01T00:00:00Z"));
    let replayed = store::entries::replayed(&e.store.pool, &key).await.unwrap();
    assert!(
        replayed.is_some(),
        "the applied entry must be findable by the occurrence key"
    );
    let _ = checking;
}

#[tokio::test]
async fn a_compound_rule_that_lands_on_a_weekday_applies_end_to_end() {
    // The fine control asked for, proven through the Action a surface calls
    // rather than only in the pure kernel: a step summed from four units, then
    // rolled forward onto a chosen weekday.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;

    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: capture("-1200.50", Some(&rent)),
                condition: None,
                gate: None,
                carry: None,
                note: Some("compound".to_string()),
                cadence: Cadence::every(CadenceStep {
                    months: 1,
                    days: 1,
                    seconds: 1,
                    milliseconds: 10,
                    ..Default::default()
                })
                .landing_on(WeekdaySet::new([CivilWeekday::Friday]).unwrap()),
                anchor_at: Some("2026-01-01T00:00:00Z".to_string()),
                request_id: Some("rule-compound".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .expect("a rule is created");

    let stored = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .expect("the rule is readable");
    // 1 January 2026 is a Thursday; the anchor itself rolls to the Friday.
    let due = at("2026-01-02T00:00:00Z");
    let found = store::recurrence::occurrences(
        &e.store.pool,
        &stored,
        at("2026-01-01T00:00:00Z"),
        at("2026-03-01T00:00:00Z"),
        at("2026-06-01T00:00:00Z"),
    )
    .await
    .unwrap();
    assert_eq!(found.dates[0].due_at, due);
    assert!(!found.truncated, "a monthly step fits the window");

    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: due.to_rfc3339(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    // It moved the resource, exactly and once.
    assert_eq!(level(&e, &checking).await, "-1200.50");
    // And the occurrence key names the landed instant, not the unlanded one.
    let replay = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.clone(),
                due_at: due.to_rfc3339(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();
    assert!(
        replay.facts.is_empty(),
        "the same landed date cannot be applied twice"
    );
    assert_eq!(level(&e, &checking).await, "-1200.50");
}

#[tokio::test]
async fn a_date_the_landing_rule_moved_past_is_not_applicable() {
    // The base date a landing rule rolled off is no longer a date the rule
    // produces. Accepting it would let one occurrence be applied twice, once
    // under each instant.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: capture("-10", Some(&rent)),
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_months(1)
                    .landing_on(WeekdaySet::new([CivilWeekday::Friday]).unwrap()),
                anchor_at: Some("2026-01-01T00:00:00Z".to_string()),
                request_id: Some("rule-landed".to_string()),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();

    // The Thursday the step landed on before the roll.
    let refused = e
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule,
                due_at: "2026-01-01T00:00:00Z".to_string(),
                amount: None,
                note: None,
            },
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "an unlanded base date is not an occurrence"
    );
}

#[tokio::test]
async fn a_rule_that_never_advances_is_refused_before_it_is_written() {
    // The sand guards this in the form, but the form is not the boundary. A
    // cleared-out step deserializes fine — `{"every":{}}` is a valid Cadence
    // shape — so if the Action wrote first and validated later, the result
    // would be a rule that persists and then errors on every read of it.
    let e = engine().await;
    let (checking, rent) = setup(&e).await;

    let empty: Cadence =
        serde_json::from_str(r#"{"every":{}}"#).expect("an empty step still parses");
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: checking.clone(),
                consequences: capture("-10", Some(&rent)),
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: empty,
                anchor_at: None,
                request_id: Some("rule-empty".to_string()),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a step with no components is not a rule");

    // And nothing was written on the way to refusing.
    let rules = store::recurrence::all(&e.store.pool).await.unwrap();
    assert!(rules.is_empty(), "a refused rule must leave no row behind");
}

// ---- rules that change state, not just numbers
//
// The original rule could only add a number to a Record, which is the first
// caller's shape rather than a statement about rules. These prove the shapes
// that shape could not express.

/// A task Record and two status concepts, the vocabulary a kanban column or a
/// relation trail buckets by.
async fn board(e: &Engine) -> (String, String, String) {
    let wip = store::concepts::create(&e.store.pool, "wip", &[])
        .await
        .unwrap();
    let done = store::concepts::create(&e.store.pool, "done", &[])
        .await
        .unwrap();
    let task = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("water.the.plants"),
            kind: RecordKind::Plain,
            head: "Water the plants",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    (task, wip, done)
}

async fn apply(e: &Engine, rule: &str, due_at: &str) {
    e.act(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule.to_string(),
            due_at: due_at.to_string(),
            amount: None,
            note: None,
        },
        None,
    )
    .await
    .unwrap();
}

async fn rule_with(e: &Engine, target: &str, consequences: Vec<Consequence>) -> String {
    e.act(
        Action::CreateRecurrence {
            target: target.to_string(),
            consequences,
            condition: None,
            gate: None,
            carry: None,
            note: None,
            cadence: Cadence::every_days(1),
            anchor_at: Some("2026-01-01T00:00:00Z".to_string()),
            request_id: Some(nucleus::new_uid("req")),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .expect("a rule is created")
}

#[tokio::test]
async fn a_rule_can_re_arm_a_task_without_carrying_an_amount() {
    let e = engine().await;
    let (task, _, _) = board(&e).await;

    // "Set to -1" is what makes a task a Need again. Unlike a capture it is not
    // cumulative, which is the whole reason it is a separate consequence.
    let rule = rule_with(
        &e,
        &task,
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;

    apply(&e, &rule, "2026-01-02T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "-1");

    // A second day sets it to -1 again rather than to -2.
    apply(&e, &rule, "2026-01-03T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "-1");
}

// ---- rules that act without anybody pressing anything

/// Build a rule on `target` with a chosen cadence and anchor.
async fn rule_every(
    e: &Engine,
    target: &str,
    cadence: Cadence,
    anchor_at: &str,
    consequences: Vec<Consequence>,
) -> String {
    e.act(
        Action::CreateRecurrence {
            target: target.to_string(),
            consequences,
            condition: None,
            gate: None,
            carry: None,
            note: None,
            cadence,
            anchor_at: Some(anchor_at.to_string()),
            request_id: Some(nucleus::new_uid("req")),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .expect("a rule is created")
}

#[tokio::test]
async fn a_weekly_habit_re_arms_itself_with_nobody_pressing_apply() {
    // The whole point of the pillar. A person declares "every week this becomes
    // a Need again" once; the wheel does it every week after that. Needing a
    // click each Monday would mean the declaration said nothing.
    let e = engine().await;
    let (task, _, _) = board(&e).await;

    let rule = rule_every(
        &e,
        &task,
        Cadence::every_weeks(1),
        "2026-03-02T07:00:00Z",
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;

    // Declaring moves nothing, exactly as before.
    assert_eq!(level(&e, &task).await, "0");

    // The first Monday arrives and nobody is here.
    e.fire_due_rules(at("2026-03-02T07:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &task).await,
        "-1",
        "a declared rule must act on its own"
    );

    // The person ticks it off.
    e.act(
        Action::SetQuantity {
            target: task.clone(),
            value: 0.0,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(level(&e, &task).await, "0");

    // Same beat again: the date already ran, so nothing repeats it. Without
    // this the wheel would undo the person's tick on the very next heartbeat.
    e.fire_due_rules(at("2026-03-02T09:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "0");

    // Next week it re-arms by itself.
    e.fire_due_rules(at("2026-03-09T07:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-1");
    let _ = rule;
}

#[tokio::test]
async fn a_cell_that_slept_owes_every_date_it_missed() {
    // Three missed rents are three rents. Collapsing them to one would quietly
    // decide that time spent asleep costs nothing.
    let e = engine().await;
    let (task, _, _) = board(&e).await;

    rule_every(
        &e,
        &task,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("-2").unwrap()),
        }],
    )
    .await;

    // Woken on the fourth day: the 1st, 2nd, 3rd and 4th are all owed.
    e.fire_due_rules(at("2026-03-04T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-8");

    // And a second beat in the same window owes nothing further.
    e.fire_due_rules(at("2026-03-04T09:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-8");
}

/// A rule with an *if*: cadence, condition, gate, carry, consequences.
#[allow(clippy::too_many_arguments)]
async fn conditional_rule(
    e: &Engine,
    target: &str,
    cadence: Cadence,
    anchor_at: &str,
    condition: &str,
    gate: &str,
    carry: &str,
    consequences: Vec<Consequence>,
) -> String {
    e.act(
        Action::CreateRecurrence {
            target: target.to_string(),
            consequences,
            condition: Some(condition.to_string()),
            gate: Some(gate.to_string()),
            carry: Some(carry.to_string()),
            note: None,
            cadence,
            anchor_at: Some(anchor_at.to_string()),
            request_id: Some(nucleus::new_uid("req")),
        },
        None,
    )
    .await
    .unwrap()
    .created
    .expect("a rule is created")
}

#[tokio::test]
async fn a_rule_can_look_before_it_acts() {
    // "Every day, but only when stock is low." The date arriving is half a
    // reason; the condition is the other half, and it is asked against the
    // world as it stands at the moment of firing.
    let e = engine().await;
    let stock = store::records::create(
        &e.store.pool,
        NewRecord {
            slug: Some("apples.stock"),
            kind: RecordKind::Plain,
            head: "Apples",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    e.act(
        Action::SetQuantity {
            target: stock.clone(),
            value: 8.0,
        },
        None,
    )
    .await
    .unwrap();

    let rule = conditional_rule(
        &e,
        &stock,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "@apples.stock",
        "<3",
        "one",
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("10").unwrap()),
        }],
    )
    .await;

    // Stock is 8, so the gate blocks and every date passes untouched.
    e.fire_due_rules(at("2026-03-04T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &stock).await, "8", "a blocked gate must not act");

    // A blocked date is not spent: the answer can change without the rule
    // changing, so it is asked again rather than being marked done.
    e.act(
        Action::SetQuantity {
            target: stock.clone(),
            value: 2.0,
        },
        None,
    )
    .await
    .unwrap();
    e.fire_due_rules(at("2026-03-04T09:00:00Z")).await.unwrap();
    assert_ne!(
        level(&e, &stock).await,
        "2",
        "once the condition holds, the dates it skipped must still be there"
    );
    let _ = rule;
}

#[tokio::test]
async fn the_carry_decides_what_the_consequence_receives() {
    // The canonical rule, ported off f64: `-1 * freq(...)` used to carry -1
    // into a Record. Here the same separation shows without a timer — the
    // condition computes 8, the gate passes, and the carry hands over -1
    // instead, because what to test and what to write are two decisions.
    let e = engine().await;
    let (task, _, _) = board(&e).await;
    e.act(
        Action::SetQuantity {
            target: task.clone(),
            value: 8.0,
        },
        None,
    )
    .await
    .unwrap();

    conditional_rule(
        &e,
        &task,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "@water.the.plants",
        ">0",
        "const:-1",
        vec![Consequence::CaptureEntry {
            amount: nucleus::DecimalValue::parse_inferred("0").unwrap(),
            concept: None,
        }],
    )
    .await;

    // One date fires and the carried -1 is what moves, not the 8 it tested.
    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "7");
}

#[tokio::test]
async fn a_condition_that_cannot_be_read_is_refused_when_it_is_written() {
    // Not at 3am inside a heartbeat with nobody watching.
    let e = engine().await;
    let (task, _, _) = board(&e).await;
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: task.clone(),
                consequences: vec![Consequence::AddQuantity {
                    delta: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
                }],
                condition: Some("@a +".to_string()),
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
    assert!(refused.is_err(), "an unreadable condition must not store");

    // And a gate with nothing to gate is refused too: dropping it silently
    // would turn a rule that fires sometimes into one that fires always.
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: task.clone(),
                consequences: vec![Consequence::AddQuantity {
                    delta: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
                }],
                condition: None,
                gate: Some("<3".to_string()),
                carry: None,
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: None,
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a gate needs a condition to act on");
}

#[tokio::test]
async fn a_skipped_date_is_not_applied_by_the_wheel() {
    // Skipping ahead of the beat is how a person says "not this one" now that
    // due dates apply themselves. If the wheel ignored a skip, declining would
    // be a button that does nothing but relabel what happens anyway.
    let e = engine().await;
    let (task, _, _) = board(&e).await;

    let rule = rule_every(
        &e,
        &task,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;
    e.act(
        Action::SkipRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-03-02T07:00:00Z".to_string(),
            note: None,
        },
        None,
    )
    .await
    .unwrap();

    // The 1st and 3rd are owed; the 2nd was declined.
    e.fire_due_rules(at("2026-03-03T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &task).await,
        "-2",
        "the declined date must not run"
    );

    // Taking the decision back makes it owed again.
    e.act(
        Action::UnskipRecurrenceOccurrence {
            recurrence: rule.clone(),
            due_at: "2026-03-02T07:00:00Z".to_string(),
        },
        None,
    )
    .await
    .unwrap();
    e.fire_due_rules(at("2026-03-03T09:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-3");
}

#[tokio::test]
async fn a_paused_rule_acts_for_nobody() {
    // Pausing means "stop acting for me", including for dates that already fell
    // due. A pause that only hid the future while the wheel kept firing would
    // be the most surprising possible reading of the word.
    let e = engine().await;
    let (task, _, _) = board(&e).await;

    let rule = rule_every(
        &e,
        &task,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        vec![Consequence::SetQuantity {
            value: Some(nucleus::DecimalValue::parse_inferred("-1").unwrap()),
        }],
    )
    .await;
    let revision = store::recurrence::get(&e.store.pool, &rule)
        .await
        .unwrap()
        .unwrap()
        .revision;
    e.act(
        Action::SetRecurrencePaused {
            recurrence: rule.clone(),
            expected_revision: revision,
            request_id: nucleus::new_uid("req"),
            paused: true,
        },
        None,
    )
    .await
    .unwrap();

    e.fire_due_rules(at("2026-03-04T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "0", "a paused rule fires nothing");
}

#[tokio::test]
async fn moving_a_card_between_columns_is_one_rule() {
    let e = engine().await;
    let (task, wip, done) = board(&e).await;
    e.act(
        Action::AssertRecord {
            subject: task.clone(),
            predicate: wip.clone(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();

    let rule = rule_with(
        &e,
        &task,
        vec![
            Consequence::RemoveConcept {
                concept: wip.clone(),
            },
            Consequence::AddConcept {
                concept: done.clone(),
            },
        ],
    )
    .await;
    apply(&e, &rule, "2026-01-02T00:00:00Z").await;

    let concepts = store::ledger::record_concepts(&e.store.pool, &task)
        .await
        .unwrap();
    assert!(!concepts.contains(&wip), "the old column is left");
    assert!(concepts.contains(&done), "the new column is entered");
    // Nothing about a quantity was declared, so nothing moved one.
    assert_eq!(level(&e, &task).await, "0");
}

#[tokio::test]
async fn a_rule_that_moves_no_quantity_still_records_that_it_ran() {
    let e = engine().await;
    let (task, _, done) = board(&e).await;
    let rule = rule_with(
        &e,
        &task,
        vec![Consequence::AddConcept {
            concept: done.clone(),
        }],
    )
    .await;

    // Applying is recorded by an entry carrying the occurrence's request id.
    // A concept-only rule appends no amount, so without a zero-delta entry
    // nothing would mark the date done: it would read as due forever and
    // re-apply every time somebody pressed the button.
    apply(&e, &rule, "2026-01-02T00:00:00Z").await;
    let applied = store::recurrence::applied(
        &e.store.pool,
        &rule,
        DateTime::parse_from_rfc3339("2026-01-02T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc),
    )
    .await
    .unwrap();
    assert!(applied.is_some(), "the date is marked applied");
}

#[tokio::test]
async fn applying_a_state_changing_rule_twice_changes_nothing_the_second_time() {
    let e = engine().await;
    let (task, _, _) = board(&e).await;
    // `add-quantity` is the consequence that would actually double, which is
    // why the guard has to run before any consequence rather than relying on
    // the entry's UNIQUE request id to refuse the capture afterwards.
    let rule = rule_with(
        &e,
        &task,
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("5").unwrap()),
        }],
    )
    .await;

    apply(&e, &rule, "2026-01-02T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "5");
    apply(&e, &rule, "2026-01-02T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "5", "the second apply is a no-op");
}

#[tokio::test]
async fn a_rule_with_nothing_to_do_is_refused_at_the_action_boundary() {
    let e = engine().await;
    let (task, _, _) = board(&e).await;
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: task,
                consequences: Vec::new(),
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: None,
                request_id: Some("empty-1".to_string()),
            },
            None,
        )
        .await;
    assert!(refused.is_err(), "a rule must do something");
}

#[tokio::test]
async fn deleting_a_rule_removes_its_future_and_keeps_what_it_already_did() {
    let e = engine().await;
    let (checking, rent) = setup(&e).await;
    let rule = monthly(&e, &checking, &rent, "-1200").await;
    apply(&e, &rule, "2026-02-01T00:00:00Z").await;
    assert_eq!(level(&e, &checking).await, "-1200");

    e.act(
        Action::DeleteRecurrence {
            recurrence: rule.clone(),
        },
        None,
    )
    .await
    .unwrap();

    // The rule is gone from every read surface.
    assert!(
        store::recurrence::get(&e.store.pool, &rule)
            .await
            .unwrap()
            .is_none()
    );
    // What it already applied is an ordinary entry and an ordinary Fact. The
    // rule proposed that change; it never owned it.
    assert_eq!(level(&e, &checking).await, "-1200");
}

#[tokio::test]
async fn deleting_a_rule_that_is_not_there_is_an_error_not_a_silence() {
    let e = engine().await;
    let refused = e
        .act(
            Action::DeleteRecurrence {
                recurrence: "rec_nope".to_string(),
            },
            None,
        )
        .await;
    assert!(refused.is_err());
}

/// A plain Record, ready to be given a rhythm or acted on.
async fn record(e: &Engine, slug: &str) -> String {
    store::records::create(
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
    .unwrap()
    .uid
}

#[tokio::test]
async fn a_rhythm_is_a_number_a_rule_can_multiply() {
    // The unification, in one rule. A schedule is not a separate kind of object
    // a rule can only *be* — it is a reading a rule can *do arithmetic on*.
    //
    // `-1 * freq(@payday)` is worth zero on the six days nothing lands and -1
    // on the seventh. So a rule that is looked at every single day acts exactly
    // weekly, using nothing but the threshold every rule already has. No second
    // trigger, no timer object, no separate table: the gate does it.
    let e = engine().await;
    let payday = record(&e, "payday").await;
    let habit = record(&e, "habit").await;

    // The rhythm: every seven days from a Monday. It is an ordinary rule, which
    // is the point — there is nothing else a schedule could be.
    rule_every(
        &e,
        &payday,
        Cadence::every_days(7),
        "2026-03-02T00:00:00Z",
        capture("0", None),
    )
    .await;

    // The reader: looked at daily, one day earlier, so its dates and the
    // rhythm's are deliberately out of phase.
    conditional_rule(
        &e,
        &habit,
        Cadence::every_days(1),
        "2026-03-01T00:00:00Z",
        "-1 * freq(@payday)",
        "!=0",
        "value",
        capture("0", None),
    )
    .await;

    // Four days in, one payday has passed.
    e.fire_due_rules(at("2026-03-05T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "the daily rule must have acted once, on the payday"
    );

    // Three more days, still the same week: the gate keeps blocking.
    e.fire_due_rules(at("2026-03-08T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "a day with no payday in it must leave the record alone"
    );

    // The next Monday lands and it acts again.
    e.fire_due_rules(at("2026-03-09T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-2",
        "the second payday must act exactly once"
    );
}

#[tokio::test]
async fn a_rhythm_a_sleeping_cell_missed_is_counted_once_per_date() {
    // The failure this rules out is the one that makes catch-up untrustworthy:
    // a Cell offline for three weeks wakes up and either forgets two paydays or
    // counts twenty-one of them.
    //
    // Neither can happen, because each date a rule owes reads over the gap back
    // to *its own* previous date. Those windows tile the timeline exactly — no
    // instant falls in two of them, and none falls in none.
    let e = engine().await;
    let payday = record(&e, "payday").await;
    let habit = record(&e, "habit").await;

    rule_every(
        &e,
        &payday,
        Cadence::every_days(7),
        "2026-03-02T00:00:00Z",
        capture("0", None),
    )
    .await;
    conditional_rule(
        &e,
        &habit,
        Cadence::every_days(1),
        "2026-03-01T00:00:00Z",
        "-1 * freq(@payday)",
        "!=0",
        "value",
        capture("0", None),
    )
    .await;

    // Asleep from the first of March to the twenty-second: three Mondays.
    e.fire_due_rules(at("2026-03-22T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-3",
        "three paydays passed, so three is the only honest answer"
    );

    // And waking again changes nothing: every date it owed is spent.
    e.fire_due_rules(at("2026-03-22T18:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-3",
        "a second wake must add nothing"
    );
}

#[tokio::test]
async fn a_rhythm_nobody_declared_is_zero_rather_than_a_refusal() {
    // A condition may name a Record that has no rule on it yet. That is not a
    // typo to refuse — it is "that rhythm has not happened", which is true, and
    // it lets the arithmetic be written before the schedule it will watch.
    let e = engine().await;
    let quiet = record(&e, "quiet").await;
    let habit = record(&e, "habit").await;

    conditional_rule(
        &e,
        &habit,
        Cadence::every_days(1),
        "2026-03-01T00:00:00Z",
        "-1 * freq(@quiet)",
        "!=0",
        "value",
        capture("0", None),
    )
    .await;

    e.fire_due_rules(at("2026-03-10T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "0",
        "a Record with no rhythm on it must read as zero, not fire"
    );
    // The Record exists; nothing was refused. Prove the reference resolved by
    // giving it a rhythm and watching the same rule start acting.
    rule_every(
        &e,
        &quiet,
        Cadence::every_days(7),
        "2026-03-11T00:00:00Z",
        capture("0", None),
    )
    .await;
    e.fire_due_rules(at("2026-03-12T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "once the rhythm exists the same unchanged rule must act on it"
    );
}

#[tokio::test]
async fn a_date_applied_by_hand_moves_the_record_once() {
    // The same date, pressed rather than fired. A rule whose condition reads
    // the Record it acts on is the shape that catches a double-apply: the
    // change it makes comes straight back round to it, and while the apply is
    // still running the entry marking the date done does not exist yet.
    //
    // Both roads to an apply must therefore be one firing. When only the
    // heartbeat was guarded, pressing apply in the inbox moved the number
    // twice, and it was the *manual* path — the one a person watches — that
    // was wrong.
    let e = engine().await;
    let stock = record(&e, "apples.stock").await;
    e.act(
        Action::SetQuantity {
            target: stock.clone(),
            value: 2.0,
        },
        None,
    )
    .await
    .unwrap();

    let rule = conditional_rule(
        &e,
        &stock,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "@apples.stock",
        "<3",
        "one",
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("10").unwrap()),
        }],
    )
    .await;

    e.act_at(
        Action::ApplyRecurrenceOccurrence {
            recurrence: rule,
            due_at: "2026-03-01T07:00:00Z".to_string(),
            amount: None,
            note: None,
        },
        None,
        at("2026-03-01T08:00:00Z"),
    )
    .await
    .unwrap();

    assert_eq!(
        level(&e, &stock).await,
        "12",
        "one press is one application, not two"
    );
}

#[tokio::test]
async fn a_rule_can_set_a_record_to_a_figure_its_own_arithmetic_worked_out() {
    // The sentence this whole merge exists for: "the quantity of a record
    // changes according to a frequency, automatically, forever."
    //
    // `-1 * freq(@payday)` is the reading. On a payday it is worth -1, on every
    // other day exactly zero — so the ordinary `!=0` gate makes a rule that is
    // looked at daily act weekly, and `set-quantity` with no figure of its own
    // receives the number the reading worked out. No timer object, no second
    // table, no second kind of trigger.
    let e = engine().await;
    let payday = record(&e, "payday").await;
    let habit = record(&e, "habit").await;

    rule_every(
        &e,
        &payday,
        Cadence::every_days(7),
        "2026-03-02T00:00:00Z",
        capture("0", None),
    )
    .await;
    conditional_rule(
        &e,
        &habit,
        Cadence::every_days(1),
        "2026-03-01T00:00:00Z",
        "-1 * freq(@payday)",
        "!=0",
        "value",
        vec![Consequence::SetQuantity { value: None }],
    )
    .await;

    // Monday: the reading is -1, so the Record is set to -1.
    e.fire_due_rules(at("2026-03-02T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "the payday must set the level"
    );

    // The person answers it: back to zero.
    e.act(
        Action::SetQuantity {
            target: habit.clone(),
            value: 0.0,
        },
        None,
    )
    .await
    .unwrap();
    assert_eq!(level(&e, &habit).await, "0");

    // The rest of the week the reading is zero, so nothing touches it — which
    // is the difference between a habit and a nag.
    e.fire_due_rules(at("2026-03-06T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "0",
        "a day with no payday must be quiet"
    );

    // Next Monday it re-arms itself, with nobody pressing anything.
    e.fire_due_rules(at("2026-03-09T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "and the next week it comes back on its own"
    );
}
