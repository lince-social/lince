use std::{
    collections::BTreeMap, future::Future, num::NonZeroU32, pin::Pin, sync::Arc, time::Duration,
};

use chrono::{TimeZone, Utc};
use nucleus::karma::{
    ArmedDeadline, CanonicalHash, DeadlineAdmission, DeadlineEntry, DeadlineIndex,
    DemandedDeadline, DispatcherResourceGrant, DurationMs, EvaluationLimits, HostTimerCapabilities,
    MAX_OCCURRENCE_BATCH_PAGE_TICKS, ScheduleCursorLifecycle, ScheduleDemandCapacity,
    ScheduleWorkloadUpperBounds, SchedulerCalibration, TimeZoneProvider, TimestampMs, TzdbRevision,
    plan_demanded_deadlines,
};
use tokio::sync::watch;

use crate::{Engine, EngineError};

pub type DeadlineSleep<'a> =
    Pin<Box<dyn Future<Output = Result<DeadlineClockWake, EngineError>> + Send + 'a>>;

/// Fills `store`'s boundary-input seam (blueprint E0.1).
///
/// `protein` is built on top of `store`, so the run path cannot execute a saved
/// Protein itself. `engine` sits above both, so it is the one layer that can —
/// it implements the trait `store` declared and hands it down.
pub struct SavedProteinInputs<'a> {
    pub store: &'a store::Store,
}

#[async_trait::async_trait]
impl store::karma::runs::ExternalInputResolver for SavedProteinInputs<'_> {
    async fn saved_protein(
        &self,
        view_uid: &str,
    ) -> Result<Option<nucleus::karma::LiteralValue>, store::StoreError> {
        let rows = match protein::execute_saved(self.store, view_uid, None).await {
            Ok(rows) => rows,
            // A view that cannot execute is a refusal the run reports by name,
            // not an error that aborts the whole processing turn: one broken
            // saved query must not stop every other Program from running.
            Err(_) => return Ok(None),
        };
        Ok(scalar_boundary_value(&rows))
    }
}

/// Reduce a Protein's rows to the single number an Input node can read.
///
/// Deliberately strict: exactly one row, holding either a bare number or an
/// object with exactly one numeric field. Anything else — no rows, many rows,
/// several numeric columns — is ambiguous, and guessing which number the author
/// meant is how a rule quietly computes against the wrong one.
fn scalar_boundary_value(rows: &[serde_json::Value]) -> Option<nucleus::karma::LiteralValue> {
    let [row] = rows else {
        return None;
    };
    let value = match row {
        serde_json::Value::Object(fields) => {
            let mut numeric = fields.values().filter(|value| decimal_of(value).is_some());
            let only = numeric.next()?;
            if numeric.next().is_some() {
                return None; // ambiguous: which column did the author mean?
            }
            only
        }
        other => other,
    };
    decimal_of(value).map(|value| nucleus::karma::LiteralValue::Decimal { value })
}

/// A JSON string is preferred and stays exact; a JSON number goes through the
/// named lossy door, because Protein still renders quantities as `f64` until
/// E0.1's read side is exact end to end.
fn decimal_of(value: &serde_json::Value) -> Option<nucleus::DecimalValue> {
    match value {
        serde_json::Value::String(text) => nucleus::DecimalValue::parse_inferred(text).ok(),
        serde_json::Value::Number(number) => number.as_f64().map(nucleus::fact::decimal_from_f64),
        _ => None,
    }
}

pub trait DeadlineClock: Send + Sync {
    fn now(&self) -> Result<TimestampMs, EngineError>;
    fn sleep_until(&self, deadline: TimestampMs) -> DeadlineSleep<'_>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeadlineClockWake {
    Reached {
        observed_at: TimestampMs,
    },
    ClockDiscontinuity {
        expected_at: TimestampMs,
        observed_at: TimestampMs,
    },
}

#[derive(Debug, Clone)]
pub struct TokioDeadlineClock {
    discontinuity_tolerance: DurationMs,
}

impl TokioDeadlineClock {
    pub fn new(discontinuity_tolerance: DurationMs) -> Result<Self, EngineError> {
        if discontinuity_tolerance.get() < 0 {
            return Err(EngineError::Conflict {
                code: "karma_clock_tolerance_invalid",
                message: "deadline clock discontinuity tolerance cannot be negative".to_string(),
            });
        }
        Ok(Self {
            discontinuity_tolerance,
        })
    }
}

impl DeadlineClock for TokioDeadlineClock {
    fn now(&self) -> Result<TimestampMs, EngineError> {
        system_timestamp()
    }

    fn sleep_until(&self, deadline: TimestampMs) -> DeadlineSleep<'_> {
        Box::pin(async move {
            let started_wall = self.now()?;
            let started_monotonic = tokio::time::Instant::now();
            let delay_ms = deadline
                .as_millis()
                .saturating_sub(started_wall.as_millis());
            if delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(delay_ms as u64)).await;
            }
            let observed_at = self.now()?;
            let elapsed_ms =
                i64::try_from(started_monotonic.elapsed().as_millis()).unwrap_or(i64::MAX);
            let expected_at = started_wall
                .checked_add(DurationMs::new(elapsed_ms))
                .ok_or_else(|| EngineError::Conflict {
                    code: "karma_clock_boundary",
                    message: "deadline clock monotonic projection overflowed".to_string(),
                })?;
            let drift = i128::from(observed_at.as_millis())
                .saturating_sub(i128::from(expected_at.as_millis()))
                .abs();
            if drift > i128::from(self.discontinuity_tolerance.get()) {
                Ok(DeadlineClockWake::ClockDiscontinuity {
                    expected_at,
                    observed_at,
                })
            } else {
                Ok(DeadlineClockWake::Reached { observed_at })
            }
        })
    }
}

/// One Cell-wide scheduling budget and the exact calendar artifacts available
/// to it. All elapsed and civil-calendar work is admitted together; providers
/// are keyed by their immutable revision rather than a mutable timezone name.
#[derive(Clone)]
pub struct KarmaDeadlineDirectorConfig {
    pub host: HostTimerCapabilities,
    pub grant: DispatcherResourceGrant,
    pub workload: ScheduleWorkloadUpperBounds,
    pub calibration: SchedulerCalibration,
    pub demand_capacity: ScheduleDemandCapacity,
    pub clock: Arc<dyn DeadlineClock>,
    pub worker_id: String,
    pub lease_duration: DurationMs,
    pub calendar_catch_up_budget: NonZeroU32,
    pub occurrence_expansion: KarmaOccurrenceExpansionLimits,
    pub program_processing: KarmaProgramProcessingLimits,
    providers: BTreeMap<TzdbRevision, Arc<dyn TimeZoneProvider>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KarmaOccurrenceExpansionLimits {
    pub page_size: NonZeroU32,
    pub recovery_source_limit: NonZeroU32,
}

impl KarmaOccurrenceExpansionLimits {
    pub fn new(
        page_size: NonZeroU32,
        recovery_source_limit: NonZeroU32,
    ) -> Result<Self, EngineError> {
        if page_size.get() > MAX_OCCURRENCE_BATCH_PAGE_TICKS {
            return Err(EngineError::Conflict {
                code: "karma_occurrence_expansion_page_invalid",
                message: format!(
                    "Karma occurrence expansion page exceeds {MAX_OCCURRENCE_BATCH_PAGE_TICKS} items"
                ),
            });
        }
        Ok(Self {
            page_size,
            recovery_source_limit,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KarmaProgramProcessingLimits {
    pub evaluation: EvaluationLimits,
    pub member_page_size: NonZeroU32,
}

impl KarmaDeadlineDirectorConfig {
    pub fn new(
        host: HostTimerCapabilities,
        grant: DispatcherResourceGrant,
        workload: ScheduleWorkloadUpperBounds,
        calibration: SchedulerCalibration,
        demand_capacity: ScheduleDemandCapacity,
        clock: Arc<dyn DeadlineClock>,
        worker_id: String,
        lease_duration: DurationMs,
        calendar_catch_up_budget: NonZeroU32,
        providers: impl IntoIterator<Item = Arc<dyn TimeZoneProvider>>,
    ) -> Result<Self, EngineError> {
        validate_runtime_identity(&worker_id, lease_duration)?;
        let mut by_revision = BTreeMap::new();
        for provider in providers {
            let revision = provider.revision().clone();
            if by_revision.insert(revision.clone(), provider).is_some() {
                return Err(EngineError::Conflict {
                    code: "karma_duplicate_calendar_provider",
                    message: format!(
                        "more than one calendar provider claims tzdb {} ({})",
                        revision.version.as_str(),
                        revision.digest.as_str()
                    ),
                });
            }
        }
        Ok(Self {
            host,
            grant,
            workload,
            calibration,
            demand_capacity,
            clock,
            worker_id,
            lease_duration,
            calendar_catch_up_budget,
            occurrence_expansion: KarmaOccurrenceExpansionLimits {
                page_size: NonZeroU32::new(MAX_OCCURRENCE_BATCH_PAGE_TICKS)
                    .expect("expansion page maximum is non-zero"),
                recovery_source_limit: NonZeroU32::new(16)
                    .expect("default recovery source limit is non-zero"),
            },
            program_processing: KarmaProgramProcessingLimits {
                evaluation: EvaluationLimits::default(),
                member_page_size: NonZeroU32::new(16)
                    .expect("default Program member page size is non-zero"),
            },
            providers: by_revision,
        })
    }

    pub fn provider(&self, revision: &TzdbRevision) -> Option<&dyn TimeZoneProvider> {
        self.providers.get(revision).map(AsRef::as_ref)
    }

    pub fn provider_revisions(&self) -> impl Iterator<Item = &TzdbRevision> {
        self.providers.keys()
    }
}

impl Engine {
    /// Start the single Cell-wide deadline director. It rebuilds the durable
    /// directory only at boot or after an explicit change notification. A
    /// normal firing removes and reinserts only the completed cursor.
    pub fn start_karma_deadline_director(
        self: Arc<Self>,
        config: KarmaDeadlineDirectorConfig,
    ) -> tokio::task::JoinHandle<Result<(), EngineError>> {
        let changed = self.karma_deadline_changed.subscribe();
        tokio::spawn(run_deadline_director(self, config, changed))
    }
}

struct DeadlineDirectory {
    index: DeadlineIndex,
    calendar_provider_by_activation: BTreeMap<CanonicalHash, TzdbRevision>,
    lease_recovery_at: Option<TimestampMs>,
    pending_occurrence_expansion: bool,
    pending_program_processing: bool,
}

async fn run_deadline_director(
    engine: Arc<Engine>,
    config: KarmaDeadlineDirectorConfig,
    mut changed: watch::Receiver<u64>,
) -> Result<(), EngineError> {
    validate_runtime_identity(&config.worker_id, config.lease_duration)?;
    let mut directory = rebuild_directory(&engine, &config).await?;
    let mut lease_recovery_at = directory.lease_recovery_at;

    loop {
        let next_wake = earliest(directory.index.next_host_wake(), lease_recovery_at);
        if directory.pending_occurrence_expansion || directory.pending_program_processing {
            let expansion_now = config.clock.now()?;
            if next_wake.is_none_or(|deadline| deadline > expansion_now) {
                let background_now = chrono_timestamp(expansion_now)?;
                if directory.pending_occurrence_expansion {
                    store::karma::expansions::expand_pending_schedule_occurrences(
                        &engine.store.pool,
                        config.occurrence_expansion.recovery_source_limit,
                        config.occurrence_expansion.page_size,
                        background_now,
                    )
                    .await?;
                }
                directory.pending_occurrence_expansion =
                    store::karma::expansions::has_pending_schedule_occurrences(&engine.store.pool)
                        .await?;
                if store::karma::runs::has_pending_occurrences(&engine.store.pool).await? {
                    store::karma::runs::process_next_occurrence(
                        &engine.store.pool,
                        config.program_processing.evaluation,
                        config.program_processing.member_page_size,
                        background_now,
                        Some(&SavedProteinInputs {
                            store: &engine.store,
                        }),
                    )
                    .await?;
                }
                directory.pending_program_processing =
                    store::karma::runs::has_pending_occurrences(&engine.store.pool).await?;
                tokio::task::yield_now().await;
                continue;
            }
        }
        let Some(next_wake) = next_wake else {
            await_directory_change(&mut changed).await?;
            directory = rebuild_directory(&engine, &config).await?;
            lease_recovery_at = directory.lease_recovery_at;
            continue;
        };
        let wake = tokio::select! {
            result = config.clock.sleep_until(next_wake) => result?,
            result = changed.changed() => {
                result.map_err(|_| wake_channel_closed())?;
                directory = rebuild_directory(&engine, &config).await?;
                lease_recovery_at = directory.lease_recovery_at;
                continue;
            },
        };
        let observed_timestamp = match wake {
            DeadlineClockWake::Reached { observed_at } => observed_at,
            DeadlineClockWake::ClockDiscontinuity { .. } => {
                directory = rebuild_directory(&engine, &config).await?;
                lease_recovery_at = directory.lease_recovery_at;
                continue;
            }
        };
        let observed_at = chrono_timestamp(observed_timestamp)?;
        if lease_recovery_at.is_some_and(|expiry| expiry <= observed_timestamp) {
            directory = rebuild_directory(&engine, &config).await?;
            lease_recovery_at = directory.lease_recovery_at;
            continue;
        }

        let due = directory.index.pop_due(observed_timestamp);
        let mut reload = false;
        for armed in due.due {
            let lane_resolution_ms = armed.lane_resolution_ms;
            let degraded = armed.degraded;
            let provider_revision = directory
                .calendar_provider_by_activation
                .get(armed.entry.activation_hash())
                .cloned();
            match process_deadline(
                &engine,
                &config,
                armed.entry,
                provider_revision.as_ref(),
                observed_at,
            )
            .await?
            {
                DeadlineProcess::Rearm(entry) => {
                    directory.index.upsert(ArmedDeadline {
                        entry,
                        lane_resolution_ms,
                        degraded,
                    });
                }
                DeadlineProcess::RetryAt(expiry) => {
                    lease_recovery_at = earliest(lease_recovery_at, Some(expiry));
                }
                DeadlineProcess::Reload => reload = true,
                DeadlineProcess::Disarmed => {}
            }
        }
        directory.pending_occurrence_expansion =
            store::karma::expansions::has_pending_schedule_occurrences(&engine.store.pool).await?;
        directory.pending_program_processing =
            store::karma::runs::has_pending_occurrences(&engine.store.pool).await?;
        if reload {
            directory = rebuild_directory(&engine, &config).await?;
            lease_recovery_at = directory.lease_recovery_at;
        }
    }
}

async fn process_deadline(
    engine: &Engine,
    config: &KarmaDeadlineDirectorConfig,
    entry: DeadlineEntry,
    provider_revision: Option<&TzdbRevision>,
    observed_at: chrono::DateTime<Utc>,
) -> Result<DeadlineProcess, EngineError> {
    let claim = store::karma::schedules::claim_due(
        &engine.store.pool,
        entry.activation_hash(),
        entry.cursor_revision(),
        &config.worker_id,
        observed_at,
        config.lease_duration,
    )
    .await?;
    let lease = match claim {
        store::karma::schedules::ScheduleClaim::Claimed(lease) => lease,
        store::karma::schedules::ScheduleClaim::Contended { lease_expires_at } => {
            return Ok(DeadlineProcess::RetryAt(lease_expires_at));
        }
        store::karma::schedules::ScheduleClaim::Stale { .. }
        | store::karma::schedules::ScheduleClaim::Inactive => {
            return Ok(DeadlineProcess::Reload);
        }
    };
    let completed_at = chrono_timestamp(config.clock.now()?)?;
    let completion =
        if let Some(revision) = provider_revision {
            let provider = config.provider(revision).ok_or_else(|| EngineError::Conflict {
            code: "karma_calendar_provider_disappeared",
            message: format!(
                "calendar provider {} ({}) disappeared from an immutable runtime configuration",
                revision.version.as_str(),
                revision.digest.as_str()
            ),
        })?;
            store::karma::schedules::complete_calendar(
                &engine.store.pool,
                &lease,
                provider,
                observed_at,
                completed_at,
                config.calendar_catch_up_budget,
            )
            .await?
        } else {
            store::karma::schedules::complete_elapsed(
                &engine.store.pool,
                &lease,
                observed_at,
                completed_at,
            )
            .await?
        };
    if let store::karma::schedules::CursorCompletion::Completed {
        occurrence: Some(occurrence),
        ..
    } = &completion
    {
        store::karma::expansions::expand_schedule_occurrence(
            &engine.store.pool,
            &occurrence.occurrence_hash,
            config.occurrence_expansion.page_size,
            completed_at,
        )
        .await?;
    }
    completion_process(completion)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum DeadlineProcess {
    Rearm(DeadlineEntry),
    RetryAt(TimestampMs),
    Reload,
    Disarmed,
}

fn completion_process(
    completion: store::karma::schedules::CursorCompletion,
) -> Result<DeadlineProcess, EngineError> {
    match completion {
        store::karma::schedules::CursorCompletion::LostLease => Ok(DeadlineProcess::Reload),
        store::karma::schedules::CursorCompletion::Completed { cursor, .. } => {
            if cursor.lifecycle != ScheduleCursorLifecycle::Armed {
                return Ok(DeadlineProcess::Disarmed);
            }
            let deadline = cursor.deadline.ok_or_else(|| EngineError::Conflict {
                code: "karma_armed_cursor_without_deadline",
                message: "armed Karma cursor returned without a deadline".to_string(),
            })?;
            Ok(DeadlineProcess::Rearm(deadline))
        }
    }
}

async fn rebuild_directory(
    engine: &Engine,
    config: &KarmaDeadlineDirectorConfig,
) -> Result<DeadlineDirectory, EngineError> {
    let now = chrono_timestamp(config.clock.now()?)?;
    let demand_policy = store::karma::schedules::ScheduleDemandPolicy {
        workload: config.workload,
        calibration: config.calibration,
    };
    store::karma::schedules::reconcile_active_schedule_cursors(
        &engine.store.pool,
        demand_policy,
        now,
    )
    .await?;
    for provider in config.providers.values() {
        store::karma::schedules::reconcile_active_calendar_cursors(
            &engine.store.pool,
            provider.as_ref(),
            demand_policy,
            now,
        )
        .await?;
    }
    store::karma::schedules::recover_expired_schedule_leases(&engine.store.pool, now).await?;
    store::karma::expansions::expand_pending_schedule_occurrences(
        &engine.store.pool,
        config.occurrence_expansion.recovery_source_limit,
        config.occurrence_expansion.page_size,
        now,
    )
    .await?;
    let pending_occurrence_expansion =
        store::karma::expansions::has_pending_schedule_occurrences(&engine.store.pool).await?;
    if store::karma::runs::has_pending_occurrences(&engine.store.pool).await? {
        store::karma::runs::process_next_occurrence(
            &engine.store.pool,
            config.program_processing.evaluation,
            config.program_processing.member_page_size,
            now,
            Some(&SavedProteinInputs {
                store: &engine.store,
            }),
        )
        .await?;
    }
    let pending_program_processing =
        store::karma::runs::has_pending_occurrences(&engine.store.pool).await?;

    let mut entries =
        store::karma::schedules::list_armed_demanded_deadlines(&engine.store.pool).await?;
    let mut calendar_provider_by_activation = BTreeMap::new();
    for (revision, provider) in &config.providers {
        let calendar_entries = store::karma::schedules::list_armed_demanded_calendar_deadlines(
            &engine.store.pool,
            provider.as_ref(),
        )
        .await?;
        for entry in calendar_entries {
            if calendar_provider_by_activation
                .insert(entry.entry.activation_hash().clone(), revision.clone())
                .is_some()
            {
                return Err(EngineError::Conflict {
                    code: "karma_calendar_provider_overlap",
                    message: format!(
                        "calendar activation {} was resolved by multiple providers",
                        entry.entry.activation_hash().as_str()
                    ),
                });
            }
            entries.push(entry);
        }
    }

    let index = admitted_index(
        engine,
        entries,
        &config.host,
        &config.grant,
        config.demand_capacity,
        now,
    )
    .await?;
    let lease_recovery_at =
        store::karma::schedules::next_active_schedule_lease_expiry(&engine.store.pool).await?;
    Ok(DeadlineDirectory {
        index,
        calendar_provider_by_activation,
        lease_recovery_at,
        pending_occurrence_expansion,
        pending_program_processing,
    })
}

async fn admitted_index(
    engine: &Engine,
    entries: Vec<DemandedDeadline>,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    demand_capacity: ScheduleDemandCapacity,
    now: chrono::DateTime<Utc>,
) -> Result<DeadlineIndex, EngineError> {
    let mut by_activation = entries
        .into_iter()
        .map(|entry| (entry.entry.activation_hash().clone(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut plan = loop {
        match plan_demanded_deadlines(
            by_activation.values().cloned(),
            host,
            grant,
            demand_capacity,
        ) {
            Ok(plan) => break plan,
            Err(error) => {
                let Some(entry) = by_activation.remove(&error.activation_hash) else {
                    return Err(EngineError::Conflict {
                        code: "karma_deadline_plan",
                        message: error.to_string(),
                    });
                };
                store::karma::schedules::park_deadline_admission(
                    &engine.store.pool,
                    entry.entry.activation_hash(),
                    entry.entry.cursor_revision(),
                    error.reason,
                    now,
                )
                .await?;
            }
        }
    };
    for admission in &plan.admissions {
        match admission {
            DeadlineAdmission::Admitted {
                activation_hash,
                lane_resolution_ms,
                degraded,
            } => {
                if let Some(entry) = by_activation.get(activation_hash)
                    && !store::karma::schedules::record_deadline_admission(
                        &engine.store.pool,
                        activation_hash,
                        entry.entry.cursor_revision(),
                        *lane_resolution_ms,
                        *degraded,
                        now,
                    )
                    .await?
                {
                    plan.index.remove(activation_hash);
                }
            }
            DeadlineAdmission::Paused {
                activation_hash,
                reason,
            }
            | DeadlineAdmission::Rejected {
                activation_hash,
                reason,
            } => {
                if let Some(entry) = by_activation.get(activation_hash) {
                    store::karma::schedules::park_deadline_admission(
                        &engine.store.pool,
                        activation_hash,
                        entry.entry.cursor_revision(),
                        *reason,
                        now,
                    )
                    .await?;
                    plan.index.remove(activation_hash);
                }
            }
        }
    }
    Ok(plan.index)
}

async fn await_directory_change(changed: &mut watch::Receiver<u64>) -> Result<(), EngineError> {
    changed.changed().await.map_err(|_| wake_channel_closed())
}

fn wake_channel_closed() -> EngineError {
    EngineError::Conflict {
        code: "karma_deadline_wake_closed",
        message: "Karma deadline change channel closed".to_string(),
    }
}

fn earliest(left: Option<TimestampMs>, right: Option<TimestampMs>) -> Option<TimestampMs> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.min(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn system_timestamp() -> Result<TimestampMs, EngineError> {
    TimestampMs::from_millis(Utc::now().timestamp_millis()).map_err(|error| EngineError::Conflict {
        code: "karma_clock_boundary",
        message: error.to_string(),
    })
}

fn chrono_timestamp(value: TimestampMs) -> Result<chrono::DateTime<Utc>, EngineError> {
    Utc.timestamp_millis_opt(value.as_millis())
        .single()
        .ok_or_else(|| EngineError::Conflict {
            code: "karma_clock_boundary",
            message: "deadline clock timestamp is outside chrono range".to_string(),
        })
}

fn validate_runtime_identity(
    worker_id: &str,
    lease_duration: DurationMs,
) -> Result<(), EngineError> {
    if lease_duration.get() <= 0 {
        return Err(EngineError::Conflict {
            code: "karma_invalid_lease",
            message: "Karma deadline lease duration must be positive".to_string(),
        });
    }
    if worker_id.is_empty()
        || worker_id.len() > 200
        || worker_id.trim() != worker_id
        || worker_id.chars().any(char::is_control)
    {
        return Err(EngineError::Conflict {
            code: "karma_invalid_worker",
            message: "Karma deadline worker id must contain 1 to 200 trimmed non-control bytes"
                .to_string(),
        });
    }
    Ok(())
}
