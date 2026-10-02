use chrono::{DateTime, Utc};
use engine::{Engine, actions::Action};
use nucleus::karma::Consequence;
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use store::sqlx::Row;

mod support;

fn at(offset: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(1_893_456_000_000 + offset).unwrap()
}

fn input(purpose: Purpose, delay: u64, amount: i128) -> BoundaryInput {
    BoundaryInput {
        purpose,
        date: DateInput::After {
            milliseconds: delay,
        },
        target: "room".into(),
        consequences: vec![Consequence::SetQuantity {
            value: Some(store::exact::integer(amount)),
        }],
    }
}

fn save(boundaries: Vec<BoundaryInput>, id: &str) -> Action {
    Action::SaveKarmaSchedule {
        schedule: None,
        expected_revision: None,
        name: "Room".into(),
        boundaries,
        request_id: id.into(),
    }
}

async fn fire(engine: &Engine, rule: &str, due: i64) -> Result<(), engine::EngineError> {
    engine
        .act_at(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.into(),
                due_at: at(due).to_rfc3339(),
                amount: None,
                note: None,
            },
            None,
            at(due),
        )
        .await
        .map(|_| ())
}

#[tokio::test]
async fn range_edits_keep_elapsed_dates_and_the_bound_target_after_renaming() {
    let engine = support::karma::engine().await;
    let original = support::plain(&engine, "room", 0.0).await;
    let mut start = input(Purpose::Start, 5_000, -1);
    let uid = engine
        .act_at(
            save(
                vec![start.clone(), input(Purpose::End, 10_000, 0)],
                "elapsed",
            ),
            None,
            at(0),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    engine
        .act_at(
            Action::SetSlug {
                target: original.clone(),
                slug: Some("renamed".into()),
            },
            None,
            at(1_000),
        )
        .await
        .unwrap();
    let other = support::plain(&engine, "room", 7.0).await;
    start.consequences = vec![Consequence::SetQuantity {
        value: Some(store::exact::integer(-2)),
    }];
    engine
        .act_at(
            Action::SaveKarmaSchedule {
                schedule: Some(uid.clone()),
                expected_revision: Some(1),
                name: "Extended".into(),
                boundaries: vec![start, input(Purpose::End, 20_000, 0)],
                request_id: "extend-elapsed".into(),
            },
            None,
            at(2_000),
        )
        .await
        .unwrap();
    let current = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let start = current
        .boundaries
        .iter()
        .find(|value| value.current && value.input.purpose == Purpose::Start)
        .unwrap();
    let end = current
        .boundaries
        .iter()
        .find(|value| value.current && value.input.purpose == Purpose::End)
        .unwrap();
    assert_eq!(start.intended_at_ms, at(5_000).timestamp_millis());
    assert_eq!(end.intended_at_ms, at(22_000).timestamp_millis());
    fire(&engine, &start.rule, 5_000).await.unwrap();
    assert_eq!(
        store::facts::level(&engine.store.pool, &original)
            .await
            .unwrap(),
        store::exact::integer(-2)
    );
    fire(&engine, &end.rule, 22_000).await.unwrap();
    assert!(
        store::facts::level(&engine.store.pool, &original)
            .await
            .unwrap()
            .is_zero()
    );
    assert_eq!(
        store::facts::level(&engine.store.pool, &other)
            .await
            .unwrap(),
        store::exact::integer(7)
    );
}

#[tokio::test]
async fn accepted_retry_survives_interruption_before_evaluation_and_database_reopen() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("recovered.sqlite");
    let engine = Engine::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    support::karma::authorize(&engine).await;
    let room = support::plain(&engine, "room", 0.0).await;
    let price = support::plain(&engine, "price", 0.0).await;
    let uid = engine
        .act_at(
            save(vec![input(Purpose::Once, 5_000, -1)], "retry"),
            None,
            at(0),
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let schedule = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let boundary = &schedule.boundaries[0];
    engine
        .act_at(
            Action::SaveKarmaRule {
                identity: None,
                rule: Some(boundary.rule.clone()),
                expected_revision: Some(1),
                fields: [
                    format!(
                        "freq(@{}) * extension(@price, \"inventory\", \"price\")",
                        boundary.frequency
                    ),
                    "!=0".into(),
                    "@room = -1".into(),
                ]
                .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text { source }),
                request_id: "require-price".into(),
            },
            None,
            at(1_000),
        )
        .await
        .unwrap();
    assert!(fire(&engine, &boundary.rule, 5_000).await.is_err());
    engine
        .act_at(
            Action::SetExtension {
                target: price,
                namespace: "inventory".into(),
                fds: serde_json::json!({"price":2}),
            },
            None,
            at(6_000),
        )
        .await
        .unwrap();
    let retry = Action::RetryKarmaSchedule {
        schedule: uid.clone(),
        expected_revision: 2,
        boundary: boundary.uid.clone(),
        request_id: "accepted-before-stop".into(),
    };
    let control = nucleus::execution::control::Control::new(0, None, None);
    let execution = nucleus::execution::Execution::new([8; 32], at(7_000).timestamp_millis())
        .unwrap()
        .controlled(control, "current".into());
    assert!(matches!(
        execution
            .scope(engine.act_at(retry.clone(), None, at(7_000)))
            .await,
        Err(engine::EngineError::ExecutionLimit(
            nucleus::execution::control::Limit::Evaluations
        ))
    ));
    let pending = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pending.boundaries[0].status, "pending");
    assert_eq!(pending.boundaries[0].attempt, 1);
    engine.store.pool.close().await;
    let recovered = Engine::open(&format!("sqlite://{}", path.display()))
        .await
        .unwrap();
    recovered.advance_karma_time(at(8_000)).await.unwrap();
    recovered.act_at(retry, None, at(9_000)).await.unwrap();
    let rows = store::sqlx::query("SELECT event_id, status, attempt FROM karma_rule_application WHERE rule_uid = ? ORDER BY attempt")
        .bind(&boundary.rule).fetch_all(&recovered.store.pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows[0].get::<String, _>("event_id"),
        rows[1].get::<String, _>("event_id")
    );
    assert_eq!(rows[1].get::<String, _>("status"), "applied");
    assert_eq!(rows[1].get::<i64, _>("attempt"), 1);
    assert_eq!(
        store::facts::level(&recovered.store.pool, &room)
            .await
            .unwrap(),
        store::exact::integer(-1)
    );
}

#[tokio::test]
async fn revoked_visibility_hides_history_and_refuses_schedule_edits_and_replay() {
    let engine = support::karma::engine().await;
    let target = support::plain(&engine, "room", 0.0).await;
    let person = support::person(&engine, "scheduler").await;
    let role = store::auth::ensure_role(&engine.store.pool, "scheduler")
        .await
        .unwrap();
    for (resource, action) in [
        ("frequency", "create"),
        ("frequency", "read"),
        ("frequency", "update"),
        ("record", "read"),
        ("record", "update"),
    ] {
        let permission = store::auth::ensure_permission(&engine.store.pool, resource, action)
            .await
            .unwrap();
        store::auth::grant(&engine.store.pool, role, permission)
            .await
            .unwrap();
    }
    store::auth::create_credential(&engine.store.pool, &person.uid, "scheduler", "hash", role)
        .await
        .unwrap();
    store::visibility::grant(&engine.store.pool, "actor", Some(&person.uid), &target)
        .await
        .unwrap();
    let action = save(vec![input(Purpose::Once, 5_000, -1)], "authorized");
    let uid = engine
        .act_at(action.clone(), Some(person.uid.clone()), at(0))
        .await
        .unwrap()
        .created
        .unwrap();
    let inspected = engine
        .act_at(
            Action::InspectKarmaSchedules {
                schedule: Some(uid.clone()),
            },
            Some(person.uid.clone()),
            at(1_000),
        )
        .await
        .unwrap();
    assert_eq!(inspected.data.unwrap().as_array().unwrap().len(), 1);
    store::sqlx::query("DELETE FROM visibility_rule WHERE target_uid = ?")
        .bind(&target)
        .execute(&engine.store.pool)
        .await
        .unwrap();
    assert!(
        engine
            .act_at(
                Action::InspectKarmaSchedules {
                    schedule: Some(uid.clone())
                },
                Some(person.uid.clone()),
                at(2_000)
            )
            .await
            .is_err()
    );
    let inspected = engine
        .act_at(
            Action::InspectKarmaSchedules { schedule: None },
            Some(person.uid.clone()),
            at(2_000),
        )
        .await
        .unwrap();
    assert!(inspected.data.unwrap().as_array().unwrap().is_empty());
    assert!(
        engine
            .act_at(action, Some(person.uid.clone()), at(2_000))
            .await
            .is_err()
    );
    assert!(
        engine
            .act_at(
                Action::CancelKarmaSchedule {
                    schedule: uid.clone(),
                    expected_revision: 1,
                    request_id: "revoked-cancel".into()
                },
                Some(person.uid),
                at(2_000)
            )
            .await
            .is_err()
    );
    engine.advance_karma_time(at(5_000)).await.unwrap();
    assert!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap()
            .is_zero()
    );
    let schedule = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert!(!schedule.cancelled);
    assert_eq!(schedule.revision, 1);
    assert_eq!(schedule.boundaries[0].status, "failed");
}

#[tokio::test]
async fn changing_a_partly_applied_boundary_cannot_repeat_committed_work() {
    let engine = support::karma::engine().await;
    let target = support::plain(&engine, "room", 0.0).await;
    let mut boundary = input(Purpose::Once, 5_000, 1);
    boundary.consequences = vec![
        Consequence::AddQuantity {
            delta: Some(store::exact::integer(1)),
        },
        Consequence::RunQuery {
            query: "pending-query".into(),
            params: None,
        },
    ];
    let uid = engine
        .act_at(save(vec![boundary.clone()], "partial"), None, at(0))
        .await
        .unwrap()
        .created
        .unwrap();
    let saved = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    fire(&engine, &saved.boundaries[0].rule, 5_000)
        .await
        .unwrap();
    boundary.date = DateInput::After {
        milliseconds: 10_000,
    };
    let result = engine
        .act_at(
            Action::SaveKarmaSchedule {
                schedule: Some(uid.clone()),
                expected_revision: Some(1),
                name: "Later".into(),
                boundaries: vec![boundary],
                request_id: "repeat-partial".into(),
            },
            None,
            at(6_000),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        store::karma_schedules::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    engine
        .act_at(
            Action::SaveKarmaRule {
                identity: None,
                rule: Some(saved.boundaries[0].rule.clone()),
                expected_revision: Some(1),
                fields: [
                    format!("freq(@{})", saved.boundaries[0].frequency),
                    "!=0".into(),
                    "@room += 2".into(),
                ]
                .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text { source }),
                request_id: "change-applied-rule".into(),
            },
            None,
            at(6_001),
        )
        .await
        .unwrap();
    fire(&engine, &saved.boundaries[0].rule, 5_000)
        .await
        .unwrap();
    engine
        .act_at(
            Action::CancelKarmaSchedule {
                schedule: uid,
                expected_revision: 2,
                request_id: "cancel-pending".into(),
            },
            None,
            at(7_000),
        )
        .await
        .unwrap();
    assert!(engine.run_due_effects().await.unwrap().is_empty());
    assert_eq!(
        store::facts::level(&engine.store.pool, &target)
            .await
            .unwrap(),
        store::exact::integer(1)
    );
    let cancellations: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_effect_outcome WHERE status = 'cancelled'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(cancellations, 1);
}

#[tokio::test]
async fn ordinary_rule_edits_refresh_the_range_and_refuse_a_stale_date_form() {
    let engine = support::karma::engine().await;
    support::plain(&engine, "room", 0.0).await;
    let old_inputs = vec![
        input(Purpose::Start, 5_000, -1),
        input(Purpose::End, 10_000, 0),
    ];
    let uid = engine
        .act_at(save(old_inputs.clone(), "shared-editor"), None, at(0))
        .await
        .unwrap()
        .created
        .unwrap();
    let old = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let start = &old.boundaries[0];
    let edit = Action::SaveKarmaRule {
        identity: None,
        rule: Some(start.rule.clone()),
        expected_revision: Some(1),
        fields: [
            format!("freq(@{})", start.frequency),
            "!=0".into(),
            "@room = -3".into(),
        ]
        .map(|source| nucleus::karma::rule_field::RuleFieldInput::Text { source }),
        request_id: "ordinary-edit".into(),
    };
    engine.act_at(edit.clone(), None, at(2_000)).await.unwrap();
    engine.act_at(edit, None, at(2_001)).await.unwrap();
    let current = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(current.revision, 2);
    let current_start = current
        .boundaries
        .iter()
        .find(|value| value.current && value.input.purpose == Purpose::Start)
        .unwrap();
    assert_eq!(
        current_start.input.consequences,
        vec![Consequence::SetQuantity {
            value: Some(store::exact::integer(-3))
        }]
    );
    assert!(
        engine
            .act_at(
                Action::SaveKarmaSchedule {
                    schedule: Some(uid.clone()),
                    expected_revision: Some(1),
                    name: "Stale".into(),
                    boundaries: old_inputs,
                    request_id: "stale-date-form".into(),
                },
                None,
                at(3_000)
            )
            .await
            .is_err()
    );
    engine
        .act_at(
            Action::SaveKarmaSchedule {
                schedule: Some(uid.clone()),
                expected_revision: Some(2),
                name: "Keep the edit".into(),
                boundaries: vec![current_start.input.clone(), input(Purpose::End, 20_000, 0)],
                request_id: "fresh-date-form".into(),
            },
            None,
            at(3_000),
        )
        .await
        .unwrap();
    let updated = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.revision, 3);
    let updated_start = updated
        .boundaries
        .iter()
        .find(|value| value.current && value.input.purpose == Purpose::Start)
        .unwrap();
    assert_eq!(updated_start.rule, start.rule);
    assert_eq!(updated_start.intended_at_ms, at(5_000).timestamp_millis());
    fire(&engine, &updated_start.rule, 5_000).await.unwrap();
    assert_eq!(
        store::records::resolve(&engine.store.pool, "room")
            .await
            .unwrap()
            .unwrap()
            .quantity,
        store::exact::integer(-3)
    );
}
