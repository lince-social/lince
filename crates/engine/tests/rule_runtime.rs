use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use engine::{Engine, actions::Action};
use nucleus::karma::{Cadence, CadenceStep, Consequence};

mod support;

fn number(value: &str) -> nucleus::DecimalValue {
    nucleus::DecimalValue::parse_inferred(value).unwrap()
}

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_millis(Utc::now().timestamp_millis() + 100).unwrap()
}

async fn level(engine: &Engine, target: &str) -> nucleus::DecimalValue {
    store::facts::level(&engine.store.pool, target)
        .await
        .unwrap()
}

async fn frequency(engine: &Engine, anchor: DateTime<Utc>, milliseconds: u32) -> String {
    engine
        .act(
            Action::CreateFrequency {
                slug: "pulse".into(),
                head: None,
                every: CadenceStep {
                    milliseconds,
                    ..Default::default()
                },
                anchor_at: Some(anchor.to_rfc3339()),
                request_id: None,
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap()
}

async fn rule(
    engine: &Engine,
    target: &str,
    condition: &str,
    consequences: Vec<Consequence>,
) -> String {
    support::declare_rule(
        engine,
        target,
        Cadence::every_days(1),
        &now().to_rfc3339(),
        Some(condition),
        Some("!=0"),
        Some("value"),
        consequences,
    )
    .await
}

#[tokio::test]
async fn record_changes_each_execute_and_chain_without_a_period_lock() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 10.0).await;
    let counter = support::plain(&engine, "counter", 0.0).await;
    let mirror = support::plain(&engine, "mirror", 0.0).await;
    rule(
        &engine,
        &counter,
        "@source",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    rule(
        &engine,
        &mirror,
        "@counter * 2",
        vec![Consequence::SetQuantity { value: None }],
    )
    .await;
    for _ in 0..5 {
        engine.append_user(&source, -1.0).await.unwrap();
    }
    assert_eq!(level(&engine, &counter).await, number("5"));
    assert_eq!(level(&engine, &mirror).await, number("10"));
    let applications: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(applications, 10);
}

#[tokio::test]
async fn one_frequency_serves_many_rules_and_record_events_do_not_repeat_beats() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 3.0).await;
    let first = support::plain(&engine, "first", 0.0).await;
    let second = support::plain(&engine, "second", 0.0).await;
    let start = now();
    frequency(&engine, start + TimeDelta::seconds(1), 1_000).await;
    rule(
        &engine,
        &first,
        "-1 * freq(@pulse) * @source",
        vec![Consequence::AddQuantity { delta: None }],
    )
    .await;
    rule(
        &engine,
        &second,
        "freq(@pulse)",
        vec![Consequence::AddQuantity {
            delta: Some(number("2")),
        }],
    )
    .await;
    engine.advance_karma_time(start).await.unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(1))
        .await
        .unwrap();
    assert_eq!(level(&engine, &first).await, number("-3"));
    assert_eq!(level(&engine, &second).await, number("2"));
    engine.append_user(&source, 1.0).await.unwrap();
    assert_eq!(level(&engine, &first).await, number("-3"));
    engine
        .advance_karma_time(start + TimeDelta::seconds(1))
        .await
        .unwrap();
    assert_eq!(level(&engine, &first).await, number("-3"));
    engine
        .advance_karma_time(start + TimeDelta::seconds(2))
        .await
        .unwrap();
    assert_eq!(level(&engine, &first).await, number("-7"));
    let cursors: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_schedule_cursor")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(cursors, 1);
}

#[tokio::test]
async fn deleting_last_reader_disarms_and_resume_skips_inactive_time() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let start = now();
    let frequency = frequency(&engine, start + TimeDelta::seconds(1), 1_000).await;
    let uid = rule(
        &engine,
        &target,
        "freq(@pulse)",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    engine.advance_karma_time(start).await.unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(1))
        .await
        .unwrap();
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: uid.clone(),
                expected_revision: 1,
                request_id: "pause".into(),
                paused: true,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(2))
        .await
        .unwrap();
    assert_eq!(
        store::karma::frequencies::get_handle(&engine.store.pool, &frequency)
            .await
            .unwrap()
            .unwrap()
            .status,
        nucleus::karma::DefinitionStatus::Paused
    );
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: uid.clone(),
                expected_revision: 2,
                request_id: "resume".into(),
                paused: false,
            },
            None,
        )
        .await
        .unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(10))
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("1"));
    engine
        .advance_karma_time(start + TimeDelta::seconds(11))
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("2"));
    engine
        .act(Action::DeleteRecurrence { recurrence: uid }, None)
        .await
        .unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(12))
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("2"));
}

#[tokio::test]
async fn commands_wake_without_heartbeat_and_are_claimed_once() {
    let engine = Arc::new(support::engine().await);
    let source = support::plain(&engine, "source", 0.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    rule(
        &engine,
        &target,
        "@source",
        vec![
            Consequence::SetQuantity {
                value: Some(number("7")),
            },
            Consequence::RunCommand {
                command: "printf 'karma-command-ok'".into(),
            },
        ],
    )
    .await;
    let worker = engine.clone().start_effect_worker();
    engine.append_user(&source, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, number("7"));
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let done: i64 = store::sqlx::query_scalar("SELECT count(*) FROM effect_queue WHERE status = 'done' AND result = 'karma-command-ok'").fetch_one(&engine.store.pool).await.unwrap();
            if done == 1 { break; }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.unwrap();
    assert!(engine.run_due_effects().await.unwrap().is_empty());
    worker.abort();
}

#[tokio::test]
async fn deadline_director_changes_quantities_without_heartbeat() {
    let engine = Arc::new(support::engine().await);
    let target = support::plain(&engine, "target", 0.0).await;
    frequency(&engine, now() + TimeDelta::milliseconds(200), 50).await;
    rule(
        &engine,
        &target,
        "freq(@pulse)",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    let config =
        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("test-live".into()).unwrap();
    let director = engine.clone().start_karma_deadline_director(config);
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if level(&engine, &target).await.to_f64() >= 2.0 {
                break;
            }
            assert!(!director.is_finished(), "director stopped");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    director.abort();
}

#[tokio::test]
async fn revising_a_rule_replaces_its_live_dependencies() {
    let engine = support::engine().await;
    let first = support::plain(&engine, "first", 0.0).await;
    let second = support::plain(&engine, "second", 0.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    let uid = rule(
        &engine,
        &target,
        "@first",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    engine.append_user(&first, 1.0).await.unwrap();
    engine
        .act(
            Action::ReviseRecurrence {
                recurrence: uid,
                expected_revision: 1,
                request_id: "replace-condition".into(),
                consequences: vec![Consequence::AddQuantity {
                    delta: Some(number("10")),
                }],
                condition: Some("@second".into()),
                gate: Some("!=0".into()),
                carry: Some("value".into()),
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: None,
            },
            None,
        )
        .await
        .unwrap();
    engine.append_user(&first, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, number("1"));
    engine.append_user(&second, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, number("11"));
}

#[tokio::test]
async fn paused_rules_cancel_queued_commands_and_failed_batches_roll_back() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 0.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    let uid = rule(
        &engine,
        &target,
        "@source",
        vec![Consequence::RunCommand {
            command: "printf 'must-not-run'".into(),
        }],
    )
    .await;
    engine.append_user(&source, 1.0).await.unwrap();
    engine
        .act(
            Action::SetRecurrencePaused {
                recurrence: uid,
                expected_revision: 1,
                request_id: "pause-before-command".into(),
                paused: true,
            },
            None,
        )
        .await
        .unwrap();
    let outcomes = engine.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].ok);
    assert!(!outcomes[0].result.contains("must-not-run"));
    rule(
        &engine,
        &target,
        "@source",
        vec![
            Consequence::SetQuantity {
                value: Some(number("170141183460469231731687303715884105727")),
            },
            Consequence::AddQuantity {
                delta: Some(number("1")),
            },
        ],
    )
    .await;
    engine.append_user(&source, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, number("0"));
    let failures: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'failed'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(failures, 1);
}

#[tokio::test]
async fn signals_use_the_shared_clock_and_each_sample_is_reactive() {
    let engine = support::engine().await;
    let signal = engine
        .act(
            Action::CreateSignal {
                slug: "sample".into(),
                head: "Sample".into(),
                source_kind: "command".into(),
                source: "printf '42'".into(),
                schedule: "1s".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let target = support::plain(&engine, "target", 0.0).await;
    rule(
        &engine,
        &target,
        "signal(@sample)",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    let start = now();
    engine.advance_karma_time(start).await.unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(1))
        .await
        .unwrap();
    assert!(
        engine
            .run_due_effects()
            .await
            .unwrap()
            .iter()
            .all(|outcome| outcome.ok)
    );
    assert_eq!(level(&engine, &signal).await, number("42"));
    assert_eq!(level(&engine, &target).await, number("1"));
    engine
        .advance_karma_time(start + TimeDelta::seconds(2))
        .await
        .unwrap();
    engine.run_due_effects().await.unwrap();
    assert_eq!(level(&engine, &target).await, number("2"));
}

#[tokio::test]
async fn recurrence_without_a_condition_uses_a_frequency_and_manual_application_deduplicates() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let start = now();
    let due = start + TimeDelta::seconds(1);
    let uid = support::declare_rule(
        &engine,
        &target,
        Cadence::every(CadenceStep {
            seconds: 1,
            ..Default::default()
        }),
        &due.to_rfc3339(),
        None,
        None,
        None,
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    engine.advance_karma_time(start).await.unwrap();
    engine
        .act(
            Action::ApplyRecurrenceOccurrence {
                recurrence: uid,
                due_at: due.to_rfc3339(),
                amount: None,
                note: None,
            },
            None,
        )
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("1"));
    engine.advance_karma_time(due).await.unwrap();
    assert_eq!(level(&engine, &target).await, number("1"));
}

#[tokio::test]
async fn frequency_permission_alone_cannot_author_commands() {
    let engine = support::engine().await;
    let person = support::person(&engine, "rule-author").await;
    let role = store::auth::ensure_role(&engine.store.pool, "rule-author")
        .await
        .unwrap();
    for (subject, action) in [("frequency", "create"), ("record", "update")] {
        let permission = store::auth::ensure_permission(&engine.store.pool, subject, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person.uid, "rule-author", "hash", role)
        .await
        .unwrap();
    let target = support::plain(&engine, "target", 0.0).await;
    let result = engine
        .act(
            Action::CreateRecurrence {
                target,
                consequences: vec![Consequence::RunCommand {
                    command: "printf 'not-authorized'".into(),
                }],
                condition: None,
                gate: None,
                carry: None,
                note: None,
                cadence: Cadence::every_days(1),
                anchor_at: None,
                request_id: None,
            },
            Some(person.uid),
        )
        .await
        .unwrap_err();
    assert!(result.to_string().contains("organ:update"), "{result}");
    assert!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        store::misc::due_effects(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn unused_frequencies_do_not_arm_and_late_wakes_replay_intended_beats() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "target", 0.0).await;
    let start = now();
    frequency(&engine, start + TimeDelta::seconds(1), 1_000).await;
    engine.advance_karma_time(start).await.unwrap();
    let cursors: i64 = store::sqlx::query_scalar("SELECT count(*) FROM karma_schedule_cursor")
        .fetch_one(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(cursors, 0);
    rule(
        &engine,
        &target,
        "freq(@pulse)",
        vec![Consequence::AddQuantity {
            delta: Some(number("1")),
        }],
    )
    .await;
    engine.advance_karma_time(start).await.unwrap();
    engine
        .advance_karma_time(start + TimeDelta::seconds(10))
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("10"));
    let facts = store::facts::for_record(&engine.store.pool, &target, 20)
        .await
        .unwrap();
    assert_eq!(
        facts
            .iter()
            .filter(|fact| fact.cause.kind == nucleus::CauseKind::Rule)
            .count(),
        10
    );
    engine
        .advance_karma_time(start + TimeDelta::seconds(10))
        .await
        .unwrap();
    assert_eq!(level(&engine, &target).await, number("10"));
}
