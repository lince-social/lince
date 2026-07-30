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
use nucleus::karma::{Cadence, CadenceStep, CivilWeekday, WeekdaySet};
use store::records::NewRecord;
use store::recurrence::{OccurrenceState, occurrence_request_id};

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
            amount: amount.to_string(),
            concept: Some(concept.to_string()),
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
                amount: "-1200".to_string(),
                concept: Some(rent),
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
    assert_eq!(entry.occurred_at, store::facts::instant(at("2026-02-01T00:00:00Z")));
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
        still.amount.to_string(),
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
    assert_eq!(february.amount.to_string(), "-1350");
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
    assert!(after.iter().all(|item| item.state != OccurrenceState::Skipped));
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
            amount: "-1300".to_string(),
            concept: Some(rent),
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
    assert_eq!(stored.amount.to_string(), "-1300");
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
                amount: "-9999".to_string(),
                concept: Some(rent),
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
                amount: "-1200.50".to_string(),
                concept: Some(rent),
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
                amount: "-10".to_string(),
                concept: Some(rent),
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
    assert!(refused.is_err(), "an unlanded base date is not an occurrence");
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
                amount: "-10".to_string(),
                concept: Some(rent),
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
