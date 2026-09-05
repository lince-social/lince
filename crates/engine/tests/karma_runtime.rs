use std::{
    collections::{BTreeMap, BTreeSet},
    num::{NonZeroU32, NonZeroU64, NonZeroUsize},
    sync::Arc,
    time::Duration,
};

use chrono::{TimeZone, Utc};
use engine::karma_runtime::{
    DeadlineClock, DeadlineClockWake, DeadlineSleep, KarmaDeadlineDirectorConfig,
    TokioDeadlineClock,
};
use engine::{Engine, EngineError};
use nucleus::karma::{
    CadenceAst, CadenceBound, CadenceStepAst, CanonicalHash, CivilDateTime, DefinitionStatus,
    DispatcherResourceGrant, DurationBinding, DurationMs, FoldPolicy, FrequencyAst,
    FrequencyCadenceAst, FrequencyParameterDefinition, FrequencySchema, FrequencyTimerAst,
    GapPolicy, HostTimerCapabilities, InactiveGapPolicy, InvalidDay, LocalId, LocalTimeResolution,
    MissedPolicy, OverloadPolicy, PositiveIntegerBinding, RationalRate, RephasePolicy,
    ScheduleCursorLifecycle, ScheduleDemandCapacity, ScheduleWorkloadUpperBounds,
    SchedulerCalibration, Slug, TimeZoneId, TimeZoneProvider, TimestampMs, TzdbRevision,
    TzdbVersion,
};
use store::karma::frequencies::{
    ActivateFrequencyInput, CreateFrequencyInput, FrequencyMutationCommit,
};

#[tokio::test]
async fn tickless_runner_reconciles_sleeps_claims_and_rearms_without_heartbeat() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-fast".to_string(),
                    frequency: frequency(anchor),
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1), nonzero(10)], NonZeroUsize::new(1_000).unwrap())
            .unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(8).unwrap(),
            NonZeroUsize::new(1_000).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        tokio_clock(),
        "engine-runtime-test".to_string(),
        DurationMs::new(100),
        nonzero(16),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();
    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());
    tokio::task::yield_now().await;
    let active = committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: "runtime-activate-fast".to_string(),
                    frequency_uid: created.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: created.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                &runtime,
                now,
            )
            .await
            .unwrap(),
    );
    let activation_hash = active.active_activation_hash.unwrap();

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
            )
            .bind(activation_hash.as_str())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if count >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("tickless runner should emit two occurrences");

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if store::karma::occurrences::list(&engine.store.pool)
                .await
                .unwrap()
                .len()
                >= 2
            {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("deadline batches should enter semantic occurrence ingress");
    assert!(
        !store::karma::expansions::has_pending_schedule_occurrences(&engine.store.pool)
            .await
            .unwrap()
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let epochs = store::karma::runs::list_epochs(&engine.store.pool)
                .await
                .unwrap();
            if epochs.len() >= 2 && epochs.iter().all(|epoch| epoch.completed) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("semantic occurrences should advance through Program epochs");

    let cursor = store::karma::schedules::get_cursor(&engine.store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert!(cursor.cursor_revision >= 3);
    assert!(cursor.last_occurrence_sequence >= 2);
    runner.abort();
}

#[tokio::test]
async fn host_default_runtime_serves_utc_and_fires_without_any_configuration() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let runtime = KarmaDeadlineDirectorConfig::for_host("engine-host-default".to_string()).unwrap();

    let revision = runtime.provider_revisions().next().cloned().unwrap();
    assert_eq!(
        revision.version.as_str(),
        engine::karma_timezone::UTC_TZDB_VERSION
    );
    let provider = runtime.provider(&revision).unwrap();
    for name in engine::karma_timezone::UTC_TIME_ZONE_IDS {
        assert_eq!(
            provider
                .resolve_local(
                    &TimeZoneId::new(name).unwrap(),
                    CivilDateTime::parse_canonical("2026-09-02T09:00:00.000").unwrap(),
                )
                .unwrap(),
            LocalTimeResolution::Unique {
                instant: TimestampMs::parse_canonical("2026-09-02T09:00:00.000Z").unwrap()
            }
        );
    }

    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "host-default-create".to_string(),
                    frequency: host_paced_frequency(anchor),
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());
    tokio::task::yield_now().await;
    let active = committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: "host-default-activate".to_string(),
                    frequency_uid: created.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: created.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                &runtime,
                now,
            )
            .await
            .unwrap(),
    );
    let activation_hash = active.active_activation_hash.unwrap();

    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
            )
            .bind(activation_hash.as_str())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if count >= 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("the host default runtime should fire an ordinary Frequency");
    runner.abort();
}

fn host_paced_frequency(anchor: TimestampMs) -> FrequencyAst {
    FrequencyAst {
        slug: Slug::new("runtime.host-default").unwrap(),
        purpose: "Prove the host default runtime configuration".to_string(),
        parameters: BTreeMap::from([(
            LocalId::new("interval").unwrap(),
            FrequencyParameterDefinition::Duration {
                default: DurationMs::new(100),
                minimum: DurationMs::new(100),
                maximum: DurationMs::new(10_000),
            },
        )]),
        timer: FrequencyTimerAst {
            required_resolution: duration(100),
            max_lateness: duration(1_000),
            coalesce_window: duration(0),
        },
        ..frequency(anchor)
    }
}

fn frequency(anchor: TimestampMs) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("runtime.fast").unwrap(),
        purpose: "Prove the tickless elapsed runner".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::from([(
            LocalId::new("interval").unwrap(),
            FrequencyParameterDefinition::Duration {
                default: DurationMs::new(20),
                minimum: DurationMs::new(1),
                maximum: DurationMs::new(1_000),
            },
        )]),
        cadence: FrequencyCadenceAst::Elapsed {
            interval: DurationBinding::Parameter {
                parameter: LocalId::new("interval").unwrap(),
            },
            anchor,
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(50),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Replay { max: nonzero(16) },
        inactive_gap: InactiveGapPolicy::SkipToNextAnchor,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}

fn duration(milliseconds: i64) -> DurationBinding {
    DurationBinding::Literal {
        value: DurationMs::new(milliseconds),
    }
}

fn committed(commit: FrequencyMutationCommit) -> store::karma::frequencies::FrequencyHandleRow {
    match commit {
        FrequencyMutationCommit::Committed { handle, .. } => handle,
        other => panic!("expected committed Frequency mutation, got {other:?}"),
    }
}

fn nonzero(value: u32) -> NonZeroU32 {
    NonZeroU32::new(value).unwrap()
}

fn baseline_workload() -> ScheduleWorkloadUpperBounds {
    ScheduleWorkloadUpperBounds::new(0, nonzero(1), 0, 256)
}

fn baseline_calibration() -> SchedulerCalibration {
    SchedulerCalibration::new(nonzero(1_000))
}

fn generous_capacity() -> ScheduleDemandCapacity {
    let maximum = RationalRate::new(u64::MAX, 1).unwrap();
    ScheduleDemandCapacity {
        semantic_ticks_per_second: maximum,
        timer_wakes_per_second: maximum,
        scheduler_cpu_ns_per_second: maximum,
        evaluator_fuel_per_second: maximum,
        writes_per_second: maximum,
        effects_per_second: maximum,
        trace_bytes_per_second: maximum,
    }
}

fn wake_limited_capacity(wakes_per_second: u64) -> ScheduleDemandCapacity {
    ScheduleDemandCapacity {
        timer_wakes_per_second: RationalRate::new(wakes_per_second, 1).unwrap(),
        ..generous_capacity()
    }
}

fn tokio_clock() -> Arc<dyn DeadlineClock> {
    Arc::new(TokioDeadlineClock::new(DurationMs::new(250)).unwrap())
}

#[derive(Debug, Clone, Copy)]
struct ManualClockSnapshot {
    now: TimestampMs,
    discontinuity_epoch: u64,
}

#[derive(Clone)]
struct ManualDeadlineClock {
    state: tokio::sync::watch::Sender<ManualClockSnapshot>,
}

impl ManualDeadlineClock {
    fn new(now: TimestampMs) -> Self {
        let (state, _) = tokio::sync::watch::channel(ManualClockSnapshot {
            now,
            discontinuity_epoch: 0,
        });
        Self { state }
    }

    fn advance_to(&self, now: TimestampMs) {
        self.state.send_modify(|state| {
            assert!(now >= state.now, "manual clock cannot move backwards");
            state.now = now;
        });
    }

    fn discontinue_to(&self, now: TimestampMs) {
        self.state.send_modify(|state| {
            state.now = now;
            state.discontinuity_epoch = state
                .discontinuity_epoch
                .checked_add(1)
                .expect("manual clock discontinuity epoch exhausted");
        });
    }
}

impl DeadlineClock for ManualDeadlineClock {
    fn now(&self) -> Result<TimestampMs, EngineError> {
        Ok(self.state.borrow().now)
    }

    fn sleep_until(&self, deadline: TimestampMs) -> DeadlineSleep<'_> {
        Box::pin(async move {
            let mut state = self.state.subscribe();
            let starting_epoch = state.borrow().discontinuity_epoch;
            loop {
                let snapshot = *state.borrow_and_update();
                if snapshot.discontinuity_epoch != starting_epoch {
                    return Ok(DeadlineClockWake::ClockDiscontinuity {
                        expected_at: deadline,
                        observed_at: snapshot.now,
                    });
                }
                if snapshot.now >= deadline {
                    return Ok(DeadlineClockWake::Reached {
                        observed_at: snapshot.now,
                    });
                }
                state.changed().await.map_err(|_| EngineError::Conflict {
                    code: "manual_clock_closed",
                    message: "manual deadline clock closed while a deadline was armed".to_string(),
                })?;
            }
        })
    }
}

#[tokio::test]
async fn manual_clock_reaches_exact_deadlines_without_wall_time() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let anchor = TimestampMs::parse_canonical("2026-01-01T00:00:00.000Z").unwrap();
    let now = Utc
        .timestamp_millis_opt(anchor.as_millis())
        .single()
        .unwrap();
    let clock = Arc::new(ManualDeadlineClock::new(anchor));
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1)], NonZeroUsize::new(100).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        clock.clone(),
        "manual-clock-director".to_string(),
        DurationMs::new(100),
        nonzero(16),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();
    let active = create_active_elapsed(
        &engine,
        &runtime,
        "runtime.manual-clock",
        "manual-clock",
        anchor,
        3,
        now,
    )
    .await;
    let activation_hash = active.active_activation_hash.unwrap();
    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());
    tokio::task::yield_now().await;

    clock.advance_to(anchor.checked_add(DurationMs::new(2)).unwrap());
    tokio::task::yield_now().await;
    let before_deadline: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
    )
    .bind(activation_hash.as_str())
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(before_deadline, 0);

    clock.advance_to(anchor.checked_add(DurationMs::new(3)).unwrap());
    wait_for_occurrences(&engine, &activation_hash, 1).await;
    let cursor = store::karma::schedules::get_cursor(&engine.store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        cursor.deadline.unwrap().next_intended_at(),
        anchor.checked_add(DurationMs::new(6)).unwrap()
    );
    runner.abort();
}

#[tokio::test]
async fn manual_clock_reports_discontinuities_as_typed_wakes() {
    let start = TimestampMs::parse_canonical("2026-01-01T00:00:00.000Z").unwrap();
    let deadline = start
        .checked_add(DurationMs::new(5 * 60 * 60 * 1_000))
        .unwrap();
    let clock = Arc::new(ManualDeadlineClock::new(start));
    let sleeper_clock = clock.clone();
    let sleeper = tokio::spawn(async move { sleeper_clock.sleep_until(deadline).await });
    tokio::task::yield_now().await;
    let observed_at = start.checked_add(DurationMs::new(7_000)).unwrap();
    clock.discontinue_to(observed_at);

    assert_eq!(
        sleeper.await.unwrap().unwrap(),
        DeadlineClockWake::ClockDiscontinuity {
            expected_at: deadline,
            observed_at,
        }
    );
}

#[derive(Clone)]
struct FakeCalendarProvider {
    revision: TzdbRevision,
    resolutions: BTreeMap<CivilDateTime, LocalTimeResolution>,
}

impl TimeZoneProvider for FakeCalendarProvider {
    fn revision(&self) -> &TzdbRevision {
        &self.revision
    }

    fn resolve_local(
        &self,
        _timezone: &TimeZoneId,
        local: CivilDateTime,
    ) -> Result<LocalTimeResolution, nucleus::karma::KarmaBoundaryError> {
        self.resolutions.get(&local).copied().ok_or_else(|| {
            nucleus::karma::KarmaBoundaryError::invalid_input(format!(
                "fixture has no calendar resolution for {local}"
            ))
        })
    }

    fn minimum_interval_ms(
        &self,
        _schedule: &nucleus::karma::CalendarSchedule,
    ) -> Result<NonZeroU64, nucleus::karma::KarmaBoundaryError> {
        Ok(NonZeroU64::new(86_400_000).unwrap())
    }
}

#[tokio::test]
async fn provider_scoped_calendar_runner_arms_and_commits_without_elapsed_fallback() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let now = Utc::now();
    let first = TimestampMs::from_millis(now.timestamp_millis() + 30).unwrap();
    let second = TimestampMs::from_millis(now.timestamp_millis() + 86_400_030).unwrap();
    let revision = TzdbRevision {
        version: TzdbVersion::new("runtime-test-1").unwrap(),
        digest: CanonicalHash::parse(format!("sha256:{}", "8".repeat(64))).unwrap(),
    };
    let provider = Arc::new(FakeCalendarProvider {
        revision: revision.clone(),
        resolutions: BTreeMap::from([
            (
                CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap(),
                LocalTimeResolution::Unique { instant: first },
            ),
            (
                CivilDateTime::parse_canonical("2026-01-02T08:00:00.000").unwrap(),
                LocalTimeResolution::Unique { instant: second },
            ),
        ]),
    });
    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-calendar".to_string(),
                    frequency: calendar_frequency(revision),
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1)], NonZeroUsize::new(100).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        tokio_clock(),
        "calendar-runtime-test".to_string(),
        DurationMs::new(100),
        nonzero(8),
        [provider.clone() as Arc<dyn TimeZoneProvider>],
    )
    .unwrap();
    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());
    tokio::task::yield_now().await;
    let active = committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: "runtime-activate-calendar".to_string(),
                    frequency_uid: created.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: created.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                &runtime,
                now,
            )
            .await
            .unwrap(),
    );
    let activation_hash = active.active_activation_hash.unwrap();
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
            )
            .bind(activation_hash.as_str())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if count == 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("calendar runner should emit its provider-resolved occurrence");
    let cursor = store::karma::schedules::get_calendar_cursor(
        &engine.store.pool,
        &activation_hash,
        provider.as_ref(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        cursor.deadline.unwrap().next_intended_at(),
        second,
        "the next civil boundary remains provider-derived"
    );
    runner.abort();
}

#[tokio::test]
async fn activation_admission_is_atomic_for_reject_and_explicit_for_pause() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(10)], NonZeroUsize::new(8).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(8).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        tokio_clock(),
        "admission-test".to_string(),
        DurationMs::new(100),
        nonzero(8),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();

    let mut rejected_definition = frequency(anchor);
    rejected_definition.slug = Slug::new("runtime.reject").unwrap();
    rejected_definition.overload = OverloadPolicy::RejectActivation;
    let rejected = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-reject".to_string(),
                    frequency: rejected_definition,
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let error = engine
        .activate_karma_frequency(
            ActivateFrequencyInput {
                request_id: "runtime-activate-reject".to_string(),
                frequency_uid: rejected.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: rejected.head_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: None,
            },
            &runtime,
            now,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("UnsupportedResolution"));
    let rejected_after =
        store::karma::frequencies::get_handle(&engine.store.pool, &rejected.record_uid)
            .await
            .unwrap()
            .unwrap();
    assert_eq!(rejected_after.status, DefinitionStatus::Proven);
    assert_eq!(rejected_after.handle_revision, 1);
    assert!(rejected_after.active_activation_hash.is_none());
    let rejected_activation_count: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM karma_frequency_activation WHERE frequency_uid = ?",
    )
    .bind(&rejected.record_uid)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(rejected_activation_count, 0);

    let mut paused_definition = frequency(anchor);
    paused_definition.slug = Slug::new("runtime.pause-on-admission").unwrap();
    let paused = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-pause-admission".to_string(),
                    frequency: paused_definition,
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let paused = committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: "runtime-activate-pause-admission".to_string(),
                    frequency_uid: paused.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: paused.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                &runtime,
                now,
            )
            .await
            .unwrap(),
    );
    assert_eq!(paused.status, DefinitionStatus::Active);
    let paused_cursor = store::karma::schedules::get_cursor(
        &engine.store.pool,
        paused.active_activation_hash.as_ref().unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(paused_cursor.lifecycle, ScheduleCursorLifecycle::Paused);
    assert!(
        paused_cursor
            .last_error_json
            .as_deref()
            .unwrap()
            .contains("unsupported-resolution")
    );
}

#[tokio::test]
async fn exact_wake_rate_capacity_rejects_dense_activation_without_a_cadence_cutoff() {
    let engine = Engine::open_memory().await.unwrap();
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1)], NonZeroUsize::new(100).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        wake_limited_capacity(1),
        tokio_clock(),
        "rate-admission-test".to_string(),
        DurationMs::new(100),
        nonzero(16),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();
    let mut definition = frequency(anchor);
    definition.slug = Slug::new("runtime.rate-reject").unwrap();
    definition.overload = OverloadPolicy::RejectActivation;
    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-rate-reject".to_string(),
                    frequency: definition,
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let error = engine
        .activate_karma_frequency(
            ActivateFrequencyInput {
                request_id: "runtime-activate-rate-reject".to_string(),
                frequency_uid: created.record_uid.clone(),
                expected_handle_revision: 1,
                revision_hash: created.head_revision_hash,
                parameter_overrides: BTreeMap::new(),
                actor_person_uid: None,
            },
            &runtime,
            now,
        )
        .await
        .unwrap_err();
    assert!(error.to_string().contains("WakeRateLimit"));
    let handle = store::karma::frequencies::get_handle(&engine.store.pool, &created.record_uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(handle.status, DefinitionStatus::Proven);
    assert!(handle.active_activation_hash.is_none());
    let cursors: i64 = store::sqlx::query_scalar(
        "SELECT COUNT(*) FROM karma_schedule_cursor WHERE frequency_uid = ?",
    )
    .bind(&created.record_uid)
    .fetch_one(&engine.store.pool)
    .await
    .unwrap();
    assert_eq!(cursors, 0);
}

#[tokio::test]
async fn director_arms_a_persisted_lease_expiry_and_recovers_after_restart() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1)], NonZeroUsize::new(100).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        tokio_clock(),
        "lease-recovery-director".to_string(),
        DurationMs::new(100),
        nonzero(16),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();
    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: "runtime-create-lease-recovery".to_string(),
                    frequency: frequency(anchor),
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    let active = committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: "runtime-activate-lease-recovery".to_string(),
                    frequency_uid: created.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: created.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                &runtime,
                now,
            )
            .await
            .unwrap(),
    );
    let activation_hash = active.active_activation_hash.unwrap();
    let cursor = store::karma::schedules::get_cursor(&engine.store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    let due = cursor.deadline.unwrap().next_intended_at();
    let due_at = Utc.timestamp_millis_opt(due.as_millis()).single().unwrap();
    let abandoned = store::karma::schedules::claim_due(
        &engine.store.pool,
        &activation_hash,
        cursor.cursor_revision,
        "abandoned-worker",
        due_at,
        DurationMs::new(50),
    )
    .await
    .unwrap();
    assert!(matches!(
        abandoned,
        store::karma::schedules::ScheduleClaim::Claimed(_)
    ));

    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
            )
            .bind(activation_hash.as_str())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if count >= 1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("director should wake once at the abandoned lease expiry");
    let recovered = store::karma::schedules::get_cursor(&engine.store.pool, &activation_hash)
        .await
        .unwrap()
        .unwrap();
    assert!(recovered.lease_fencing_token >= 2);
    runner.abort();
}

#[tokio::test]
async fn dense_rearm_does_not_requery_an_unrelated_sparse_cursor() {
    let engine = Arc::new(Engine::open_memory().await.unwrap());
    let now = Utc::now();
    let anchor = TimestampMs::from_millis(now.timestamp_millis()).unwrap();
    let runtime = KarmaDeadlineDirectorConfig::new(
        HostTimerCapabilities::new([nonzero(1)], NonZeroUsize::new(100).unwrap()).unwrap(),
        DispatcherResourceGrant::new(
            nonzero(1),
            NonZeroUsize::new(4).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            false,
        ),
        baseline_workload(),
        baseline_calibration(),
        generous_capacity(),
        tokio_clock(),
        "dense-sparse-director".to_string(),
        DurationMs::new(100),
        nonzero(16),
        std::iter::empty::<Arc<dyn TimeZoneProvider>>(),
    )
    .unwrap();
    let fast =
        create_active_elapsed(&engine, &runtime, "runtime.dense", "dense", anchor, 20, now).await;
    let sparse = create_active_elapsed(
        &engine,
        &runtime,
        "runtime.sparse",
        "sparse",
        anchor,
        18_000_000,
        now,
    )
    .await;
    let fast_hash = fast.active_activation_hash.unwrap();
    let sparse_hash = sparse.active_activation_hash.unwrap();
    let runner = engine
        .clone()
        .start_karma_deadline_director(runtime.clone());

    wait_for_occurrences(&engine, &fast_hash, 1).await;
    store::sqlx::query(
        "UPDATE karma_schedule_cursor SET next_intended_at = 'not-a-timestamp'
         WHERE activation_hash = ?",
    )
    .bind(sparse_hash.as_str())
    .execute(&engine.store.pool)
    .await
    .unwrap();
    wait_for_occurrences(&engine, &fast_hash, 4).await;
    assert!(
        !runner.is_finished(),
        "normal dense completion must not rebuild or decode sparse cursor state"
    );
    runner.abort();
}

async fn create_active_elapsed(
    engine: &Engine,
    runtime: &KarmaDeadlineDirectorConfig,
    slug: &str,
    request_suffix: &str,
    anchor: TimestampMs,
    interval_ms: i64,
    now: chrono::DateTime<Utc>,
) -> store::karma::frequencies::FrequencyHandleRow {
    let mut definition = frequency(anchor);
    definition.slug = Slug::new(slug).unwrap();
    definition.parameters.insert(
        LocalId::new("interval").unwrap(),
        FrequencyParameterDefinition::Duration {
            default: DurationMs::new(interval_ms),
            minimum: DurationMs::new(1),
            maximum: DurationMs::new(interval_ms.max(1_000)),
        },
    );
    let created = committed(
        engine
            .create_karma_frequency(
                CreateFrequencyInput {
                    request_id: format!("runtime-create-{request_suffix}"),
                    frequency: definition,
                    owner_person_uid: None,
                    actor_person_uid: None,
                },
                now,
            )
            .await
            .unwrap(),
    );
    committed(
        engine
            .activate_karma_frequency(
                ActivateFrequencyInput {
                    request_id: format!("runtime-activate-{request_suffix}"),
                    frequency_uid: created.record_uid,
                    expected_handle_revision: 1,
                    revision_hash: created.head_revision_hash,
                    parameter_overrides: BTreeMap::new(),
                    actor_person_uid: None,
                },
                runtime,
                now,
            )
            .await
            .unwrap(),
    )
}

async fn wait_for_occurrences(engine: &Engine, activation_hash: &CanonicalHash, minimum: i64) {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let count: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM karma_schedule_occurrence WHERE activation_hash = ?",
            )
            .bind(activation_hash.as_str())
            .fetch_one(&engine.store.pool)
            .await
            .unwrap();
            if count >= minimum {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("deadline director did not produce the expected occurrences");
}

fn calendar_frequency(revision: TzdbRevision) -> FrequencyAst {
    FrequencyAst {
        schema: FrequencySchema::V1,
        slug: Slug::new("runtime.calendar").unwrap(),
        purpose: "Prove the provider-scoped calendar runner".to_string(),
        tags: BTreeSet::new(),
        parameters: BTreeMap::new(),
        cadence: FrequencyCadenceAst::Calendar {
            cadence: CadenceAst {
                every: CadenceStepAst {
                    days: Some(PositiveIntegerBinding::Literal { value: nonzero(1) }),
                    ..Default::default()
                },
                land_on: None,
                invalid_day: InvalidDay::Clamp,
                bound: CadenceBound::Unbounded,
            },
            anchor: CivilDateTime::parse_canonical("2026-01-01T08:00:00.000").unwrap(),
            timezone: TimeZoneId::new("America/Sao_Paulo").unwrap(),
            tzdb: revision,
            gap: GapPolicy::ShiftForward,
            fold: FoldPolicy::Both,
        },
        timer: FrequencyTimerAst {
            required_resolution: duration(1),
            max_lateness: duration(100),
            coalesce_window: duration(0),
        },
        missed: MissedPolicy::Coalesce,
        inactive_gap: InactiveGapPolicy::ReplayByMissedPolicy,
        rephase: RephasePolicy::PreserveAnchor,
        overload: OverloadPolicy::PauseAndAsk,
    }
}
