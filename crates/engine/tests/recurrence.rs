use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use nucleus::karma::Consequence;
use nucleus::karma::{Cadence, CadenceStep, CivilWeekday, WeekdaySet};
use store::records::NewRecord;
use store::recurrence::{OccurrenceState, occurrence_request_id};

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

    let entry_uid = outcome.created.expect("an entry is created");
    let entry = store::entries::get(&e.store.pool, &entry_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(entry.revision, 1);
    assert_eq!(entry.record_uid, checking);
    let fact = &outcome.facts[0];
    let classification = store::ledger::fact_concept(&e.store.pool, &fact.uid)
        .await
        .unwrap();
    assert_eq!(classification.as_deref(), Some(rent.as_str()));
    assert_eq!(
        entry.occurred_at,
        store::facts::instant(at("2026-02-01T00:00:00Z"))
    );
}

#[tokio::test]
async fn the_same_occurrence_cannot_be_applied_twice() {
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

    assert_eq!(level(&e, &checking).await, "-1200.50");
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

    let rules = store::recurrence::all(&e.store.pool).await.unwrap();
    assert!(rules.is_empty(), "a refused rule must leave no row behind");
}

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

    apply(&e, &rule, "2026-01-03T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "-1");
}

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

    assert_eq!(level(&e, &task).await, "0");

    e.fire_due_rules(at("2026-03-02T07:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &task).await,
        "-1",
        "a declared rule must act on its own"
    );

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

    e.fire_due_rules(at("2026-03-02T09:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "0");

    e.fire_due_rules(at("2026-03-09T07:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-1");
    let _ = rule;
}

#[tokio::test]
async fn a_cell_that_slept_owes_every_date_it_missed() {
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

    e.fire_due_rules(at("2026-03-04T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-8");

    e.fire_due_rules(at("2026-03-04T09:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "-8");
}

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

    e.fire_due_rules(at("2026-03-04T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &stock).await, "8", "a blocked gate must not act");

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

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(level(&e, &task).await, "7");
}

#[tokio::test]
async fn a_condition_that_cannot_be_read_is_refused_when_it_is_written() {
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

    e.fire_due_rules(at("2026-03-03T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &task).await,
        "-2",
        "the declined date must not run"
    );

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

    assert!(
        store::recurrence::get(&e.store.pool, &rule)
            .await
            .unwrap()
            .is_none()
    );
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

    e.fire_due_rules(at("2026-03-05T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "the daily rule must have acted once, on the payday"
    );

    e.fire_due_rules(at("2026-03-08T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "a day with no payday in it must leave the record alone"
    );

    e.fire_due_rules(at("2026-03-09T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-2",
        "the second payday must act exactly once"
    );
}

#[tokio::test]
async fn a_rhythm_a_sleeping_cell_missed_is_counted_once_per_date() {
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

    e.fire_due_rules(at("2026-03-22T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-3",
        "three paydays passed, so three is the only honest answer"
    );

    e.fire_due_rules(at("2026-03-22T18:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-3",
        "a second wake must add nothing"
    );
}

#[tokio::test]
async fn a_rhythm_nobody_declared_is_zero_rather_than_a_refusal() {
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

    e.fire_due_rules(at("2026-03-02T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "the payday must set the level"
    );

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

    e.fire_due_rules(at("2026-03-06T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "0",
        "a day with no payday must be quiet"
    );

    e.fire_due_rules(at("2026-03-09T12:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &habit).await,
        "-1",
        "and the next week it comes back on its own"
    );
}

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
    .unwrap()
    .uid;
    e.act(
        Action::SetQuantity {
            target: uid.clone(),
            value: quantity,
        },
        None,
    )
    .await
    .unwrap();
    uid
}

async fn assert_concept(e: &Engine, subject: &str, concept: &str) {
    e.act(
        Action::AssertRecord {
            subject: subject.to_string(),
            predicate: concept.to_string(),
            object: None,
            quantity: None,
            unit: None,
        },
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn an_assertion_reads_as_the_sum_of_its_members_and_writes_to_all_of_them() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "foo", &[])
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "bar", &[])
        .await
        .unwrap();

    let left = plain(&e, "left", 3.0).await;
    let right = plain(&e, "right", 4.0).await;
    assert_concept(&e, &left, "foo").await;
    assert_concept(&e, &right, "foo").await;

    let first = plain(&e, "first", 0.0).await;
    let second = plain(&e, "second", 99.0).await;
    assert_concept(&e, &first, "bar").await;
    assert_concept(&e, &second, "bar").await;

    let ticker = plain(&e, "ticker", 0.0).await;
    let rule = conditional_rule(
        &e,
        &ticker,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "#foo * 2",
        "always",
        "value",
        vec![Consequence::SetQuantityWhere {
            assertion: "foo".to_string(),
            value: None,
        }],
    )
    .await;
    let _ = rule;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();

    assert_eq!(
        level(&e, &left).await,
        "14",
        "3 + 4 doubled, written to every member of the set the rule names"
    );
    assert_eq!(level(&e, &right).await, "14");
    assert_eq!(
        level(&e, &first).await,
        "0",
        "a Record outside the named assertion is untouched"
    );
    assert_eq!(level(&e, &second).await, "99");
}

#[tokio::test]
async fn an_assertion_nobody_has_declared_blocks_the_rule() {
    let e = engine().await;
    let ticker = plain(&e, "ticker", 0.0).await;
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: ticker.clone(),
                consequences: vec![Consequence::SetQuantityWhere {
                    assertion: "nosuch".to_string(),
                    value: None,
                }],
                condition: Some("@ticker".to_string()),
                gate: Some("always".to_string()),
                carry: Some("value".to_string()),
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: Some("2026-03-01T07:00:00Z".to_string()),
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "a consequence naming a concept nothing answers to must be refused when written"
    );
}

#[tokio::test]
async fn an_assertion_with_no_members_reads_as_zero_rather_than_refusing() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "empty", &[])
        .await
        .unwrap();
    let ticker = plain(&e, "ticker", 5.0).await;
    conditional_rule(
        &e,
        &ticker,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "#empty",
        "!=0",
        "value",
        vec![Consequence::AddQuantity {
            delta: Some(nucleus::DecimalValue::parse_inferred("1").unwrap()),
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &ticker).await,
        "5",
        "a declared concept with nothing asserted on it is zero, and zero blocks the gate"
    );
}

#[tokio::test]
async fn renaming_a_concept_leaves_the_rules_that_name_it_pointing_at_it() {
    let e = engine().await;
    let first = store::concepts::create(&e.store.pool, "target", &[])
        .await
        .unwrap();
    let held = plain(&e, "held", 0.0).await;
    assert_concept(&e, &held, "target").await;

    let ticker = plain(&e, "ticker", 7.0).await;
    conditional_rule(
        &e,
        &ticker,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "@ticker",
        "always",
        "value",
        vec![Consequence::SetQuantityWhere {
            assertion: "target".to_string(),
            value: None,
        }],
    )
    .await;

    store::concepts::rename(&e.store.pool, &first, "renamed")
        .await
        .unwrap();
    store::concepts::create(&e.store.pool, "target", &[])
        .await
        .unwrap();
    let impostor = plain(&e, "impostor", 0.0).await;
    assert_concept(&e, &impostor, "target").await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();

    assert_eq!(
        level(&e, &held).await,
        "7",
        "the rule follows the concept it was pointed at, whatever it is now called"
    );
    assert_eq!(
        level(&e, &impostor).await,
        "0",
        "a different concept that later takes the old name must not inherit the rule"
    );
}

#[tokio::test]
async fn any_name_a_concept_answers_to_reaches_the_same_concept() {
    let e = engine().await;
    let fruit = store::concepts::create(&e.store.pool, "fruit", &[])
        .await
        .unwrap();
    store::concepts::add_name(&e.store.pool, &fruit, "pt", "fruta")
        .await
        .unwrap();
    let apple = plain(&e, "apple", 3.0).await;
    assert_concept(&e, &apple, "fruit").await;

    let ticker = plain(&e, "ticker", 0.0).await;
    conditional_rule(
        &e,
        &ticker,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "#fruta * 2",
        "always",
        "value",
        vec![Consequence::SetQuantityWhere {
            assertion: "fruta".to_string(),
            value: None,
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &apple).await,
        "6",
        "a rule written in one language reads and writes the same concept as one written in another"
    );
}

#[tokio::test]
async fn a_name_two_concepts_answer_to_is_refused_rather_than_guessed() {
    let e = engine().await;
    let colour = store::concepts::create(&e.store.pool, "orange.colour", &[])
        .await
        .unwrap();
    let fruit = store::concepts::create(&e.store.pool, "orange.fruit", &[])
        .await
        .unwrap();
    store::concepts::add_name(&e.store.pool, &colour, "en", "orange")
        .await
        .unwrap();
    store::concepts::add_name(&e.store.pool, &fruit, "en", "orange")
        .await
        .unwrap();

    let ticker = plain(&e, "ticker", 1.0).await;
    let refused = e
        .act(
            Action::CreateRecurrence {
                target: ticker.clone(),
                consequences: vec![Consequence::SetQuantityWhere {
                    assertion: "orange".to_string(),
                    value: None,
                }],
                condition: Some("@ticker".to_string()),
                gate: Some("always".to_string()),
                carry: Some("value".to_string()),
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: Some("2026-03-01T07:00:00Z".to_string()),
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await;
    assert!(
        refused.is_err(),
        "a name two concepts answer to must be refused at authoring, not resolved by luck"
    );
}

#[tokio::test]
async fn a_capture_files_under_a_concept_named_rather_than_identified() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "groceries", &[])
        .await
        .unwrap();
    let wallet = plain(&e, "wallet", 0.0).await;
    let rule = e
        .act(
            Action::CreateRecurrence {
                target: wallet.clone(),
                consequences: vec![Consequence::CaptureEntry {
                    amount: nucleus::DecimalValue::parse_inferred("-12").unwrap(),
                    concept: Some("groceries".to_string()),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: Some("2026-03-01T07:00:00Z".to_string()),
                request_id: Some(nucleus::new_uid("req")),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .expect("a rule is created");
    let _ = rule;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &wallet).await,
        "-12",
        "a concept given by name must reach the capture, not be stored as a token nothing resolves"
    );
}

#[tokio::test]
async fn a_canonical_name_wins_over_another_concepts_alias_for_the_same_word() {
    let e = engine().await;
    store::concepts::create(&e.store.pool, "bank", &[])
        .await
        .unwrap();
    let river = store::concepts::create(&e.store.pool, "riverside", &[])
        .await
        .unwrap();
    store::concepts::add_name(&e.store.pool, &river, "en", "bank")
        .await
        .unwrap();

    let money = plain(&e, "money", 4.0).await;
    let shore = plain(&e, "shore", 0.0).await;
    assert_concept(&e, &money, "bank").await;
    assert_concept(&e, &shore, "riverside").await;

    let ticker = plain(&e, "ticker", 9.0).await;
    conditional_rule(
        &e,
        &ticker,
        Cadence::every_days(1),
        "2026-03-01T07:00:00Z",
        "@ticker",
        "always",
        "value",
        vec![Consequence::SetQuantityWhere {
            assertion: "bank".to_string(),
            value: None,
        }],
    )
    .await;

    e.fire_due_rules(at("2026-03-01T08:00:00Z")).await.unwrap();
    assert_eq!(
        level(&e, &money).await,
        "9",
        "a word that is one concept's canonical name means that concept, alias or no alias"
    );
    assert_eq!(level(&e, &shore).await, "0");
}
