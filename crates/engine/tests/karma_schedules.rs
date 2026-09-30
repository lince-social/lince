use chrono::{DateTime, Utc};
use engine::{Engine, actions::Action};
use nucleus::karma::Consequence;
use nucleus::karma::scheduled_change::{BoundaryInput, DateInput, Purpose};
use store::sqlx::Row;

mod support;

#[tokio::test]
async fn ordinary_runtime_resolves_and_fires_a_brazilian_local_date() {
    use nucleus::karma::{CivilDateTime, FoldPolicy, GapPolicy, TimeZoneId};
    let engine = support::engine().await;
    support::plain(&engine, "room", 0.0).await;
    let preview = engine
        .act_at(
            Action::PreviewKarmaScheduleDates {
                date: CivilDateTime::parse_canonical("2030-01-01T09:00:00.000").unwrap(),
                timezone: TimeZoneId::new("America/Sao_Paulo").unwrap(),
                gap: GapPolicy::Pause,
                fold: FoldPolicy::First,
            },
            None,
            at(0),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(preview.as_array().unwrap().len(), 1);
    assert_eq!(preview[0]["at_ms"], at(43_200_000).timestamp_millis());
    let mut input = boundary(Purpose::Once, 43_200_000, -1);
    input.date = serde_json::from_value(preview[0]["date"].clone()).unwrap();
    let saved = save(&engine, None, None, vec![input], "brazil", 0)
        .await
        .data
        .unwrap();
    fire(
        &engine,
        saved["boundaries"][0]["rule"].as_str().unwrap(),
        43_200_000,
        43_200_000,
    )
    .await;
    assert_eq!(level(&engine).await, store::exact::integer(-1));
}

fn at(offset: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(1_893_456_000_000 + offset).unwrap()
}

fn boundary(purpose: Purpose, offset: i64, amount: i128) -> BoundaryInput {
    BoundaryInput {
        purpose,
        date: DateInput::Instant {
            at_ms: at(offset).timestamp_millis(),
        },
        target: "room".into(),
        consequences: vec![Consequence::SetQuantity {
            value: Some(store::exact::integer(amount)),
        }],
    }
}

async fn save(
    engine: &Engine,
    uid: Option<String>,
    revision: Option<i64>,
    boundaries: Vec<BoundaryInput>,
    request: &str,
    offset: i64,
) -> engine::actions::ActionOutcome {
    engine
        .act_at(
            Action::SaveKarmaSchedule {
                schedule: uid,
                expected_revision: revision,
                name: "Clean the room".into(),
                boundaries,
                request_id: request.into(),
            },
            None,
            at(offset),
        )
        .await
        .unwrap()
}

async fn fire(engine: &Engine, rule: &str, due: i64, observed: i64) {
    engine
        .act_at(
            Action::ApplyRecurrenceOccurrence {
                recurrence: rule.into(),
                due_at: at(due).to_rfc3339(),
                amount: None,
                note: None,
            },
            None,
            at(observed),
        )
        .await
        .unwrap();
}

async fn level(engine: &Engine) -> nucleus::DecimalValue {
    store::records::resolve(&engine.store.pool, "room")
        .await
        .unwrap()
        .unwrap()
        .quantity
}

#[tokio::test]
async fn a_range_uses_ordinary_rules_and_ends_at_zero_after_an_intervening_edit() {
    let engine = support::engine().await;
    support::plain(&engine, "room", 3.0).await;
    let inputs = vec![
        boundary(Purpose::Start, 5_000, -1),
        boundary(Purpose::End, 10_000, 0),
    ];
    let created = save(&engine, None, None, inputs.clone(), "range", 0).await;
    let uid = created.created.unwrap();
    let value = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(value.boundaries.len(), 2);
    assert_eq!(value.revision, 1);
    let replay = save(&engine, None, None, inputs, "range", 0).await;
    assert_eq!(replay.created.as_deref(), Some(uid.as_str()));
    fire(&engine, &value.boundaries[0].rule, 5_000, 5_000).await;
    assert_eq!(level(&engine).await, store::exact::integer(-1));
    let partial = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(partial.boundaries[0].status, "retired");
    assert!(
        store::recurrence::get(&engine.store.pool, &partial.boundaries[0].rule)
            .await
            .unwrap()
            .unwrap()
            .is_paused()
    );
    engine
        .act_at(
            Action::SetQuantityExact {
                target: "room".into(),
                amount: "7".into(),
            },
            None,
            at(7_000),
        )
        .await
        .unwrap();
    fire(&engine, &value.boundaries[1].rule, 10_000, 10_000).await;
    assert!(level(&engine).await.is_zero());
    fire(&engine, &value.boundaries[1].rule, 10_000, 11_000).await;
    let count: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM karma_rule_application WHERE status = 'applied'",
    )
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
}

#[tokio::test]
async fn expired_ranges_never_apply_the_start_and_cancellation_preserves_current_quantity() {
    let engine = support::engine().await;
    support::plain(&engine, "room", 3.0).await;
    let uid = save(
        &engine,
        None,
        None,
        vec![
            boundary(Purpose::Start, 5_000, -1),
            boundary(Purpose::End, 10_000, 0),
        ],
        "expired",
        0,
    )
    .await
    .created
    .unwrap();
    let value = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    fire(&engine, &value.boundaries[0].rule, 5_000, 20_000).await;
    assert_eq!(level(&engine).await, store::exact::integer(3));
    assert_eq!(
        store::karma_schedules::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .boundaries[0]
            .status,
        "expired"
    );
    fire(&engine, &value.boundaries[1].rule, 10_000, 20_000).await;
    assert!(level(&engine).await.is_zero());
    let uid = save(
        &engine,
        None,
        None,
        vec![boundary(Purpose::Once, 40_000, -1)],
        "cancel",
        20_000,
    )
    .await
    .created
    .unwrap();
    engine
        .act_at(
            Action::CancelKarmaSchedule {
                schedule: uid.clone(),
                expected_revision: 1,
                request_id: "cancel-it".into(),
            },
            None,
            at(21_000),
        )
        .await
        .unwrap();
    let value = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert!(value.cancelled);
    fire(&engine, &value.boundaries[0].rule, 40_000, 40_000).await;
    assert!(level(&engine).await.is_zero());
}

#[tokio::test]
async fn editing_both_dates_is_atomic_and_completed_starts_are_kept() {
    let engine = support::engine().await;
    support::plain(&engine, "room", 0.0).await;
    let start = boundary(Purpose::Start, 5_000, -1);
    let uid = save(
        &engine,
        None,
        None,
        vec![start.clone(), boundary(Purpose::End, 10_000, 0)],
        "original",
        0,
    )
    .await
    .created
    .unwrap();
    let old = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let replacement = save(
        &engine,
        Some(uid.clone()),
        Some(1),
        vec![
            boundary(Purpose::Start, 6_000, -2),
            boundary(Purpose::End, 12_000, 0),
        ],
        "replace",
        1_000,
    )
    .await;
    assert_eq!(replacement.data.unwrap()["revision"], 2);
    fire(&engine, &old.boundaries[0].rule, 5_000, 5_000).await;
    assert!(level(&engine).await.is_zero());
    let current = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let start = current
        .boundaries
        .iter()
        .find(|boundary| boundary.current && boundary.input.purpose == Purpose::Start)
        .unwrap();
    fire(&engine, &start.rule, 6_000, 6_000).await;
    assert_eq!(level(&engine).await, store::exact::integer(-2));
    save(
        &engine,
        Some(uid.clone()),
        Some(2),
        vec![start.input.clone(), boundary(Purpose::End, 15_000, 0)],
        "extend",
        7_000,
    )
    .await;
    let after = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        after
            .boundaries
            .iter()
            .filter(|boundary| boundary.current)
            .count(),
        2
    );
    assert_eq!(
        after
            .boundaries
            .iter()
            .filter(|boundary| boundary.current && boundary.input.purpose == Purpose::Start)
            .next()
            .unwrap()
            .rule,
        start.rule
    );
    let result = engine
        .act_at(
            Action::SaveKarmaSchedule {
                schedule: Some(uid.clone()),
                expected_revision: Some(3),
                name: "Invalid change".into(),
                boundaries: vec![
                    boundary(Purpose::Start, 20_000, -1),
                    boundary(Purpose::End, 19_000, 0),
                ],
                request_id: "invalid".into(),
            },
            None,
            at(8_000),
        )
        .await;
    assert!(result.is_err());
    assert_eq!(
        store::karma_schedules::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .revision,
        3
    );
}

#[tokio::test]
async fn failed_effects_retry_without_reapplying_quantities_and_keep_each_outcome() {
    let engine = support::engine().await;
    let target = support::plain(&engine, "room", 0.0).await;
    let mut input = boundary(Purpose::Once, 5_000, -1);
    input.consequences.push(Consequence::RunQuery {
        query: "missing-query".into(),
        params: None,
    });
    let uid = save(&engine, None, None, vec![input], "failed-effect", 0)
        .await
        .created
        .unwrap();
    let value = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    fire(&engine, &value.boundaries[0].rule, 5_000, 5_000).await;
    assert_eq!(
        store::karma_schedules::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .boundaries[0]
            .status,
        "applied"
    );
    let outcomes = engine.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(outcomes[0].ok, "{outcomes:?}");
    assert_eq!(
        store::karma_schedules::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap()
            .boundaries[0]
            .status,
        "applied"
    );
    let outcomes = engine.run_due_effects().await.unwrap();
    assert_eq!(outcomes.len(), 1);
    assert!(!outcomes[0].ok);
    let failed = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.boundaries[0].status, "failed");
    let retry = Action::RetryKarmaSchedule {
        schedule: uid.clone(),
        expected_revision: 1,
        boundary: failed.boundaries[0].uid.clone(),
        request_id: "retry".into(),
    };
    engine.act_at(retry.clone(), None, at(6_000)).await.unwrap();
    engine.act_at(retry, None, at(6_000)).await.unwrap();
    engine.run_due_effects().await.unwrap();
    let attempts = store::sqlx::query("SELECT status FROM karma_effect_outcome ORDER BY attempt")
        .fetch_all(&engine.store.pool)
        .await
        .unwrap();
    assert_eq!(attempts.len(), 3);
    assert_eq!(
        attempts
            .iter()
            .filter(|row| row.get::<String, _>("status") == "failed")
            .count(),
        2
    );
    let facts: i64 = store::sqlx::query_scalar(
        "SELECT count(*) FROM fact WHERE record_uid = ? AND cause_kind = 'rule'",
    )
    .bind(&target)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(facts, 1);
    assert_eq!(level(&engine).await, store::exact::integer(-1));
}

#[tokio::test]
async fn failed_evaluations_append_an_attempt_for_the_original_occurrence() {
    let engine = support::engine().await;
    support::plain(&engine, "room", 0.0).await;
    let price = support::plain(&engine, "price", 0.0).await;
    let uid = save(
        &engine,
        None,
        None,
        vec![boundary(Purpose::Once, 5_000, -1)],
        "reading-failure",
        0,
    )
    .await
    .created
    .unwrap();
    let saved = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let boundary = &saved.boundaries[0];
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
                request_id: "add-reading".into(),
            },
            None,
            at(1_000),
        )
        .await
        .unwrap();
    assert!(
        engine
            .act_at(
                Action::ApplyRecurrenceOccurrence {
                    recurrence: boundary.rule.clone(),
                    due_at: at(5_000).to_rfc3339(),
                    amount: None,
                    note: None
                },
                None,
                at(5_000)
            )
            .await
            .is_err()
    );
    let failed = store::karma_schedules::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.boundaries[0].status, "failed");
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
    assert!(level(&engine).await.is_zero());
    let retry = Action::RetryKarmaSchedule {
        schedule: uid.clone(),
        expected_revision: 2,
        boundary: boundary.uid.clone(),
        request_id: "retry-reading".into(),
    };
    engine.act_at(retry.clone(), None, at(7_000)).await.unwrap();
    engine.act_at(retry, None, at(8_000)).await.unwrap();
    let rows = store::sqlx::query("SELECT event_id, status, attempt, intended_at FROM karma_rule_application WHERE rule_uid = ? ORDER BY attempt").bind(&boundary.rule).fetch_all(&engine.store.pool).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("status"), "failed");
    assert_eq!(rows[1].get::<String, _>("status"), "applied");
    assert_eq!(
        rows[0].get::<String, _>("event_id"),
        rows[1].get::<String, _>("event_id")
    );
    assert_eq!(rows[1].get::<i64, _>("attempt"), 1);
    assert_eq!(
        rows[0].get::<String, _>("intended_at"),
        rows[1].get::<String, _>("intended_at")
    );
    assert_eq!(level(&engine).await, store::exact::integer(-1));
}

#[tokio::test]
async fn local_dates_use_installed_rules_and_explicit_gap_and_fold_choices() {
    use nucleus::karma::{
        ArtifactTimeZoneProvider, CivilDateTime, FoldPolicy, GapPolicy, TimeZoneArtifact,
        TimeZoneDefinition, TimeZoneId, TimeZoneProvider, TimestampMs, TzdbVersion,
        UtcOffsetSegment,
    };
    let engine = support::engine().await;
    support::plain(&engine, "room", 0.0).await;
    let first = TimestampMs::from_millis(at(0).timestamp_millis()).unwrap();
    let second = TimestampMs::from_millis(at(172_800_000).timestamp_millis()).unwrap();
    let timezone = TimeZoneId::new("Test/Clock").unwrap();
    let definition = TimeZoneDefinition::new(vec![
        UtcOffsetSegment::new(None, Some(first), 0).unwrap(),
        UtcOffsetSegment::new(Some(first), Some(second), 3600).unwrap(),
        UtcOffsetSegment::new(Some(second), None, 0).unwrap(),
    ])
    .unwrap();
    let provider = std::sync::Arc::new(
        ArtifactTimeZoneProvider::from_artifact(
            TimeZoneArtifact::new(
                TzdbVersion::new("test-clock.1").unwrap(),
                [(timezone.clone(), definition)].into(),
            )
            .unwrap(),
        )
        .unwrap(),
    );
    let revision = provider.revision().clone();
    let base =
        engine::karma_runtime::KarmaDeadlineDirectorConfig::for_host("date-test".into()).unwrap();
    let providers: Vec<std::sync::Arc<dyn TimeZoneProvider>> = vec![
        provider,
        engine::karma_timezone::utc_time_zone_provider().unwrap(),
    ];
    engine
        .install_karma_runtime_config(
            engine::karma_runtime::KarmaDeadlineDirectorConfig::new(
                base.host,
                base.grant,
                base.workload,
                base.calibration,
                base.demand_capacity,
                base.clock,
                base.worker_id,
                base.lease_duration,
                base.calendar_catch_up_budget,
                providers,
            )
            .unwrap(),
        )
        .unwrap();
    let gap = CivilDateTime::parse_canonical("2030-01-01T00:30:00.000").unwrap();
    assert!(
        engine
            .act_at(
                Action::PreviewKarmaScheduleDates {
                    date: gap,
                    timezone: timezone.clone(),
                    gap: GapPolicy::Pause,
                    fold: FoldPolicy::First
                },
                None,
                at(-3_600_000)
            )
            .await
            .is_err()
    );
    let preview = engine
        .act_at(
            Action::PreviewKarmaScheduleDates {
                date: gap,
                timezone: timezone.clone(),
                gap: GapPolicy::ShiftForward,
                fold: FoldPolicy::First,
            },
            None,
            at(-3_600_000),
        )
        .await
        .unwrap()
        .data
        .unwrap();
    assert_eq!(preview[0]["at_ms"], at(0).timestamp_millis());
    let mut input = boundary(Purpose::Once, 0, -1);
    input.date = serde_json::from_value(preview[0]["date"].clone()).unwrap();
    let saved = save(&engine, None, None, vec![input], "gap", -3_600_000).await;
    assert_eq!(
        saved.data.unwrap()["boundaries"][0]["intended_at_ms"],
        at(0).timestamp_millis()
    );
    let fold = CivilDateTime::parse_canonical("2030-01-03T00:30:00.000").unwrap();
    for (choice, offset) in [
        (FoldPolicy::First, 171_000_000),
        (FoldPolicy::Second, 174_600_000),
    ] {
        let mut input = boundary(Purpose::Once, offset, -1);
        input.date = DateInput::Local {
            date: fold,
            timezone: timezone.clone(),
            tzdb: revision.clone(),
            gap: GapPolicy::Pause,
            fold: choice,
        };
        let saved = save(
            &engine,
            None,
            None,
            vec![input],
            &format!("fold-{offset}"),
            1_000,
        )
        .await;
        assert_eq!(
            saved.data.unwrap()["boundaries"][0]["intended_at_ms"],
            at(offset).timestamp_millis()
        );
    }
}
