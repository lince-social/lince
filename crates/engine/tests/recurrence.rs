use chrono::{DateTime, Utc};
use engine::Engine;
use engine::actions::Action;
use nucleus::RecordKind;
use nucleus::karma::Consequence;
use nucleus::karma::{Cadence, CivilWeekday, WeekdaySet};
use store::records::NewRecord;
use store::recurrence::OccurrenceState;

mod support;

fn capture(amount: &str, concept: Option<&str>) -> Vec<Consequence> {
    vec![Consequence::CaptureEntry {
        amount: nucleus::DecimalValue::parse_inferred(amount).expect("exact amount"),
        concept: concept.map(str::to_string),
    }]
}

async fn engine() -> Engine {
    support::karma::engine().await
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
async fn a_manual_occurrence_defers_its_entry_and_retries_do_not_repeat_it() {
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

    assert!(outcome.facts.is_empty());
    assert_eq!(level(&e, &checking).await, "0");
    let outcomes = e.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].ok, "{}", outcomes[0].result);
    assert_eq!(level(&e, &checking).await, "-1200");
    let entries = store::entries::list_all(&e.store.pool, 10).await.unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.revision, 1);
    assert_eq!(entry.record_uid, checking);
    let classification =
        store::ledger::fact_concept(&e.store.pool, entry.fact_uid.as_deref().unwrap())
            .await
            .unwrap();
    assert_eq!(classification.as_deref(), Some(rent.as_str()));
    apply(&e, &rule, "2026-02-01T00:00:00Z").await;
    assert!(e.run_due_effects().await.unwrap().is_empty());
    assert_eq!(
        store::entries::list_all(&e.store.pool, 10).await.unwrap(),
        entries
    );
    apply(&e, &rule, "2026-03-01T00:00:00Z").await;
    assert!(e.run_due_effects().await.unwrap()[0].ok);
    assert_eq!(level(&e, &checking).await, "-2400");
    assert_eq!(
        store::entries::list_all(&e.store.pool, 10)
            .await
            .unwrap()
            .len(),
        2
    );
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

    e.advance_karma_time(at("2026-03-04T08:00:00Z"))
        .await
        .unwrap();
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
    e.advance_karma_time(at("2026-03-04T09:00:00Z"))
        .await
        .unwrap();
    assert_ne!(
        level(&e, &stock).await,
        "2",
        "once the condition holds, the dates it skipped must still be there"
    );
    let _ = rule;
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
async fn a_paused_rule_does_not_apply_a_new_manual_occurrence() {
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
    e.act(
        Action::SetQuantityExact {
            target: task.clone(),
            amount: "0".into(),
        },
        None,
    )
    .await
    .unwrap();
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

    apply(&e, &rule, "2026-01-03T00:00:00Z").await;
    assert_eq!(level(&e, &task).await, "0", "a paused rule fires nothing");
}

#[tokio::test]
async fn a_manual_occurrence_defers_concept_changes_and_deduplicates_delivery() {
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
    let before = store::ledger::record_concepts(&e.store.pool, &task)
        .await
        .unwrap();
    assert!(before.contains(&wip));
    assert!(!before.contains(&done));
    let outcomes = e.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes.iter().all(|outcome| outcome.ok), "{outcomes:?}");
    let concepts = store::ledger::record_concepts(&e.store.pool, &task)
        .await
        .unwrap();
    assert!(!concepts.contains(&wip), "the old column is left");
    assert!(concepts.contains(&done), "the new column is entered");
    assert_eq!(level(&e, &task).await, "0");
    apply(&e, &rule, "2026-01-02T00:00:00Z").await;
    assert!(e.run_due_effects().await.unwrap().is_empty());
    assert_eq!(
        store::ledger::record_concepts(&e.store.pool, &task)
            .await
            .unwrap(),
        concepts
    );
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
async fn a_record_without_a_declared_frequency_is_refused_instead_of_reading_zero() {
    let e = engine().await;
    record(&e, "quiet").await;
    let habit = record(&e, "habit").await;
    let error = e
        .act(
            Action::CreateRecurrence {
                target: habit,
                consequences: capture("0", None),
                condition: Some("-1 * freq(@quiet)".into()),
                gate: Some("!=0".into()),
                carry: Some("value".into()),
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: Some("2026-03-01T00:00:00Z".into()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("karma_binding_unavailable:freq(@quiet)"),
        "{error}"
    );
    assert!(
        store::recurrence::all(&e.store.pool)
            .await
            .unwrap()
            .is_empty()
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

    e.advance_karma_time(at("2026-03-01T08:00:00Z"))
        .await
        .unwrap();
    assert_eq!(
        level(&e, &ticker).await,
        "5",
        "a declared concept with nothing asserted on it is zero, and zero blocks the gate"
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
