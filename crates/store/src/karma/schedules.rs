use std::num::NonZeroU32;

use chrono::{DateTime, Utc};
use nucleus::karma::{
    CalendarBoundary, CalendarCatchUpPause, CalendarCursor, CalendarCursorResolution,
    CalendarDiscontinuity, CalendarEmission, CalendarScheduleOccurrence, CanonicalHash,
    CompiledSchedule, DeadlineAdmission, DeadlineEntry, DeadlineRejectionReason, DemandedDeadline,
    DispatcherResourceGrant, DurationMs, ElapsedScheduleOccurrence, FrequencyActivationEpoch,
    HostTimerCapabilities, OccurrenceBatch, OccurrenceBatchEmission, OverloadPolicy,
    ScheduleCursor, ScheduleCursorLifecycle, ScheduleDemand, ScheduleDemandCapacity,
    ScheduleWorkloadUpperBounds, SchedulerCalibration, TimeZoneProvider, TimerPolicy, TimestampMs,
    advance_calendar_cursor, canonical_json_bytes, plan_demanded_deadlines,
    resolve_calendar_cursor,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::frequencies;
use crate::StoreError;

const MAX_WORKER_ID_BYTES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleCursorRow {
    pub activation_hash: CanonicalHash,
    pub frequency_uid: String,
    pub cursor_revision: u64,
    pub lifecycle: ScheduleCursorLifecycle,
    pub cursor: StoredScheduleCursor,
    pub deadline: Option<DeadlineEntry>,
    pub timer: TimerPolicy,
    pub overload_policy: OverloadPolicy,
    pub demand: ScheduleDemand,
    pub admitted_resolution_ms: Option<NonZeroU32>,
    pub admission_degraded: bool,
    pub admitted_at: Option<String>,
    pub lease_fencing_token: u64,
    pub lease_owner: Option<String>,
    pub lease_expires_at: Option<TimestampMs>,
    pub last_occurrence_sequence: u64,
    pub last_error_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduleDemandPolicy {
    pub workload: ScheduleWorkloadUpperBounds,
    pub calibration: SchedulerCalibration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cadence", rename_all = "kebab-case")]
pub enum StoredScheduleCursor {
    Elapsed {
        cursor: ScheduleCursor,
    },
    Calendar {
        cursor: CalendarCursor,
    },
    CalendarPaused {
        #[serde(skip_serializing_if = "Option::is_none")]
        previous: Option<CalendarBoundary>,
        reason: CalendarDiscontinuity,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<CalendarDiscontinuity>,
    },
}

impl StoredScheduleCursor {
    fn elapsed(&self) -> Option<&ScheduleCursor> {
        match self {
            Self::Elapsed { cursor } => Some(cursor),
            Self::Calendar { .. } | Self::CalendarPaused { .. } => None,
        }
    }

    fn last_intended_at(&self) -> Option<TimestampMs> {
        match self {
            Self::Elapsed { cursor } => cursor.last_intended_at(),
            Self::Calendar { cursor } => cursor.previous().map(|value| value.intended_at),
            Self::CalendarPaused { previous, .. } => previous.map(|value| value.intended_at),
        }
    }

    fn next_intended_at(&self) -> Option<TimestampMs> {
        match self {
            Self::Elapsed { cursor } => Some(cursor.next_intended_at()),
            Self::Calendar { cursor } => Some(cursor.next().intended_at),
            Self::CalendarPaused { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorMaterialization {
    Created(ScheduleCursorRow),
    Existing(ScheduleCursorRow),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleLease {
    pub cursor: ScheduleCursorRow,
    pub worker_id: String,
    pub fencing_token: u64,
    pub expires_at: TimestampMs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleClaim {
    Claimed(ScheduleLease),
    Stale { current_cursor_revision: u64 },
    Contended { lease_expires_at: TimestampMs },
    Inactive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredScheduleOccurrence {
    pub occurrence_hash: CanonicalHash,
    pub occurrence: ScheduleOccurrencePayload,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cadence", rename_all = "kebab-case")]
pub enum ScheduleOccurrencePayload {
    Elapsed {
        occurrence: ElapsedScheduleOccurrence,
    },
    Calendar {
        occurrence: CalendarScheduleOccurrence,
    },
}

impl ScheduleOccurrencePayload {
    pub const fn sequence(&self) -> u64 {
        match self {
            Self::Elapsed { occurrence } => occurrence.sequence,
            Self::Calendar { occurrence } => occurrence.sequence,
        }
    }

    fn activation_hash(&self) -> &CanonicalHash {
        match self {
            Self::Elapsed { occurrence } => &occurrence.activation_hash,
            Self::Calendar { occurrence } => &occurrence.activation_hash,
        }
    }

    fn claimed_cursor_revision(&self) -> u64 {
        match self {
            Self::Elapsed { occurrence } => occurrence.claimed_cursor_revision,
            Self::Calendar { occurrence } => occurrence.claimed_cursor_revision,
        }
    }

    fn lease_fencing_token(&self) -> u64 {
        match self {
            Self::Elapsed { occurrence } => occurrence.lease_fencing_token,
            Self::Calendar { occurrence } => occurrence.lease_fencing_token,
        }
    }

    fn observed_at(&self) -> TimestampMs {
        match self {
            Self::Elapsed { occurrence } => occurrence.observed_at,
            Self::Calendar { occurrence } => occurrence.observed_at,
        }
    }

    fn cadence_name(&self) -> &'static str {
        match self {
            Self::Elapsed { .. } => "elapsed",
            Self::Calendar { .. } => "calendar",
        }
    }

    fn occurrence_json(&self) -> Result<String, StoreError> {
        match self {
            Self::Elapsed { occurrence } => canonical_string(occurrence),
            Self::Calendar { occurrence } => canonical_string(occurrence),
        }
    }

    fn projection(&self) -> Result<OccurrenceProjection, StoreError> {
        match self {
            Self::Elapsed { occurrence } => Ok(OccurrenceProjection {
                emission_kind: match occurrence.batch.emission {
                    OccurrenceBatchEmission::Individual => "individual",
                    OccurrenceBatchEmission::Coalesced => "coalesced",
                },
                first_intended_at: occurrence.batch.range.first(),
                last_intended_at: occurrence.batch.range.last().map_err(boundary)?,
                covered_boundary_count: occurrence.batch.covered_boundary_count(),
                semantic_occurrence_count: occurrence.batch.semantic_occurrence_count(),
            }),
            Self::Calendar { occurrence } => {
                let emission = occurrence.catch_up.emission.as_ref().ok_or_else(|| {
                    protocol("stored calendar schedule occurrence has no emission")
                })?;
                let (emission_kind, boundaries, semantic_occurrence_count) = match emission {
                    CalendarEmission::Individual(boundaries) => {
                        ("individual", boundaries, boundaries.len())
                    }
                    CalendarEmission::Coalesced(boundaries) => ("coalesced", boundaries, 1),
                };
                let first = boundaries.first().ok_or_else(|| {
                    protocol("stored calendar occurrence emission cannot be empty")
                })?;
                let last = boundaries.last().expect("non-empty calendar emission");
                Ok(OccurrenceProjection {
                    emission_kind,
                    first_intended_at: first.intended_at,
                    last_intended_at: last.intended_at,
                    covered_boundary_count: u64::try_from(boundaries.len())
                        .map_err(|_| protocol("calendar occurrence boundary count overflowed"))?,
                    semantic_occurrence_count: u64::try_from(semantic_occurrence_count)
                        .map_err(|_| protocol("calendar semantic occurrence count overflowed"))?,
                })
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OccurrenceProjection {
    emission_kind: &'static str,
    first_intended_at: TimestampMs,
    last_intended_at: TimestampMs,
    covered_boundary_count: u64,
    semantic_occurrence_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorCompletion {
    Completed {
        cursor: ScheduleCursorRow,
        occurrence: Option<StoredScheduleOccurrence>,
    },
    LostLease,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScheduleReconciliation {
    pub created_elapsed: u64,
    pub existing_elapsed: u64,
    pub calendar_pending_provider: u64,
    pub superseded: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalendarScheduleReconciliation {
    pub matching_active: u64,
    pub created: u64,
    pub existing: u64,
    pub paused: u64,
}

struct PreparedActivationCursor {
    cadence_kind: &'static str,
    cursor: StoredScheduleCursor,
    lifecycle: ScheduleCursorLifecycle,
    last_intended_at: Option<TimestampMs>,
    next_intended_at: Option<TimestampMs>,
    timer: TimerPolicy,
    overload: OverloadPolicy,
    deadline: Option<DeadlineEntry>,
    demand: ScheduleDemand,
}

/// Atomically supersede the previous cursor, protect incumbent admissions, and
/// install the new cursor inside the Frequency mutation transaction.
pub(crate) async fn install_admitted_activation_cursor_tx(
    tx: &mut Transaction<'_, Sqlite>,
    activation_hash: &CanonicalHash,
    epoch: &FrequencyActivationEpoch,
    calendar_provider: Option<&dyn TimeZoneProvider>,
    demand_policy: ScheduleDemandPolicy,
    demand_capacity: ScheduleDemandCapacity,
    host: &HostTimerCapabilities,
    grant: &DispatcherResourceGrant,
    at: &str,
) -> Result<(), StoreError> {
    let prepared =
        prepare_activation_cursor(activation_hash, epoch, calendar_provider, demand_policy)?;
    let mut lifecycle = prepared.lifecycle;
    let mut last_error_json = None;
    let mut admitted_resolution_ms = None;
    let mut admission_degraded = false;

    if let Some(candidate) = &prepared.deadline {
        let incumbent_rows = sqlx::query(
            "SELECT cursor.*
             FROM karma_schedule_cursor cursor
             JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
             WHERE cursor.lifecycle = 'armed'
               AND frequency.status = 'active'
               AND frequency.active_activation_hash = cursor.activation_hash
               AND cursor.frequency_uid != ?
             ORDER BY cursor.next_intended_at, cursor.required_resolution_ms,
                      cursor.activation_hash",
        )
        .bind(epoch.frequency_uid())
        .fetch_all(&mut **tx)
        .await?;
        let incumbents = incumbent_rows
            .into_iter()
            .map(map_demanded_deadline)
            .collect::<Result<Vec<_>, _>>()?;
        let mut planned = incumbents.clone();
        planned.push(DemandedDeadline::new(candidate.clone(), prepared.demand).map_err(boundary)?);
        let plan =
            plan_demanded_deadlines(planned, host, grant, demand_capacity).map_err(|error| {
                protocol(format!(
                    "Karma Frequency activation admission failed for {}: {:?}",
                    error.activation_hash.as_str(),
                    error.reason
                ))
            })?;

        for incumbent in &incumbents {
            let retained = plan.admissions.iter().any(|admission| {
                matches!(
                    admission,
                    DeadlineAdmission::Admitted { activation_hash, .. }
                        if activation_hash == incumbent.entry.activation_hash()
                )
            });
            if !retained {
                return Err(protocol(format!(
                    "Karma Frequency activation would displace incumbent deadline {}",
                    incumbent.entry.activation_hash().as_str()
                )));
            }
        }

        let admission = plan
            .admissions
            .iter()
            .find(|admission| match admission {
                DeadlineAdmission::Admitted {
                    activation_hash, ..
                }
                | DeadlineAdmission::Rejected {
                    activation_hash, ..
                }
                | DeadlineAdmission::Paused {
                    activation_hash, ..
                } => activation_hash == candidate.activation_hash(),
            })
            .ok_or_else(|| protocol("candidate deadline has no admission result"))?;
        match admission {
            DeadlineAdmission::Admitted {
                lane_resolution_ms,
                degraded,
                ..
            } => {
                admitted_resolution_ms = Some(*lane_resolution_ms);
                admission_degraded = *degraded;
            }
            DeadlineAdmission::Paused { reason, .. } => {
                lifecycle = ScheduleCursorLifecycle::Paused;
                last_error_json = Some(admission_error_json(*reason)?);
            }
            DeadlineAdmission::Rejected { reason, .. } => {
                return Err(protocol(format!(
                    "Karma Frequency activation was rejected: {reason:?}"
                )));
            }
        }
    }

    supersede_other_cursors_tx(tx, epoch.frequency_uid(), activation_hash, at).await?;
    sqlx::query(
        "INSERT INTO karma_schedule_cursor
            (activation_hash, frequency_uid, cursor_revision, cadence_kind, lifecycle,
             cursor_json, last_intended_at, next_intended_at,
             required_resolution_ms, max_lateness_ms, coalesce_window_ms,
             overload_policy, demand_json, admitted_resolution_ms,
             admission_degraded, admitted_at, last_error_json, created_at, updated_at)
         VALUES (?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(activation_hash.as_str())
    .bind(epoch.frequency_uid())
    .bind(prepared.cadence_kind)
    .bind(lifecycle.as_str())
    .bind(canonical_string(&prepared.cursor)?)
    .bind(prepared.last_intended_at.map(|value| value.to_string()))
    .bind(prepared.next_intended_at.map(|value| value.to_string()))
    .bind(i64::from(prepared.timer.required_resolution_ms()))
    .bind(i64::from(prepared.timer.max_lateness_ms()))
    .bind(i64::from(prepared.timer.coalesce_window_ms()))
    .bind(overload_name(prepared.overload))
    .bind(canonical_string(&prepared.demand)?)
    .bind(admitted_resolution_ms.map(|value| i64::from(value.get())))
    .bind(if admission_degraded { 1_i64 } else { 0_i64 })
    .bind(admitted_resolution_ms.map(|_| at))
    .bind(last_error_json)
    .bind(at)
    .bind(at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn prepare_activation_cursor(
    activation_hash: &CanonicalHash,
    epoch: &FrequencyActivationEpoch,
    calendar_provider: Option<&dyn TimeZoneProvider>,
    demand_policy: ScheduleDemandPolicy,
) -> Result<PreparedActivationCursor, StoreError> {
    match &epoch.compiled().schedule {
        CompiledSchedule::Elapsed { schedule } => {
            let cursor = schedule
                .initial_cursor(epoch.activated_at())
                .map_err(boundary)?;
            let deadline = DeadlineEntry::new(
                activation_hash.clone(),
                1,
                cursor.next_intended_at(),
                schedule.timer(),
                schedule.overload_policy(),
            )
            .map_err(boundary)?;
            let demand = ScheduleDemand::for_elapsed(
                schedule,
                demand_policy.workload,
                demand_policy.calibration,
            )
            .map_err(boundary)?;
            Ok(PreparedActivationCursor {
                cadence_kind: "elapsed",
                cursor: StoredScheduleCursor::Elapsed {
                    cursor: cursor.clone(),
                },
                lifecycle: ScheduleCursorLifecycle::Armed,
                last_intended_at: cursor.last_intended_at(),
                next_intended_at: Some(cursor.next_intended_at()),
                timer: schedule.timer(),
                overload: schedule.overload_policy(),
                deadline: Some(deadline),
                demand,
            })
        }
        CompiledSchedule::Calendar { schedule } => {
            let provider = calendar_provider.ok_or_else(|| {
                protocol("calendar Frequency activation requires its pinned timezone provider")
            })?;
            let resolution = resolve_calendar_cursor(schedule, provider, None).map_err(boundary)?;
            let demand = ScheduleDemand::for_calendar(
                schedule,
                provider,
                demand_policy.workload,
                demand_policy.calibration,
            )
            .map_err(boundary)?;
            let (cursor, lifecycle, last_intended_at, next_intended_at) = match resolution {
                CalendarCursorResolution::Armed(cursor) => (
                    StoredScheduleCursor::Calendar {
                        cursor: cursor.clone(),
                    },
                    ScheduleCursorLifecycle::Armed,
                    cursor.previous().map(|value| value.intended_at),
                    Some(cursor.next().intended_at),
                ),
                CalendarCursorResolution::Paused {
                    previous,
                    reason,
                    skipped,
                } => (
                    StoredScheduleCursor::CalendarPaused {
                        previous,
                        reason,
                        skipped,
                    },
                    ScheduleCursorLifecycle::Paused,
                    previous.map(|value| value.intended_at),
                    None,
                ),
            };
            let deadline = next_intended_at
                .map(|next| {
                    DeadlineEntry::new(
                        activation_hash.clone(),
                        1,
                        next,
                        schedule.timer,
                        schedule.overload,
                    )
                    .map_err(boundary)
                })
                .transpose()?;
            Ok(PreparedActivationCursor {
                cadence_kind: "calendar",
                cursor,
                lifecycle,
                last_intended_at,
                next_intended_at,
                timer: schedule.timer,
                overload: schedule.overload,
                deadline,
                demand,
            })
        }
    }
}

/// One-shot boot reconciliation. Live mutation handlers should materialize the
/// activation named by their committed result directly; this is not a poller.
pub async fn reconcile_active_schedule_cursors(
    pool: &SqlitePool,
    demand_policy: ScheduleDemandPolicy,
    now: DateTime<Utc>,
) -> Result<ScheduleReconciliation, StoreError> {
    let at = canonical_timestamp(now)?;
    let superseded = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = cursor_revision + 1, lifecycle = 'superseded',
             lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE lifecycle != 'superseded' AND NOT EXISTS (
             SELECT 1 FROM karma_frequency frequency
             WHERE frequency.record_uid = karma_schedule_cursor.frequency_uid
               AND frequency.status = 'active'
               AND frequency.active_activation_hash = karma_schedule_cursor.activation_hash
         )",
    )
    .bind(&at)
    .execute(pool)
    .await?
    .rows_affected();
    let active_hashes = sqlx::query_scalar::<_, String>(
        "SELECT active_activation_hash FROM karma_frequency
         WHERE status = 'active' AND active_activation_hash IS NOT NULL
         ORDER BY active_activation_hash",
    )
    .fetch_all(pool)
    .await?;
    let mut result = ScheduleReconciliation {
        superseded,
        ..ScheduleReconciliation::default()
    };
    for encoded in active_hashes {
        let activation_hash = CanonicalHash::parse(encoded).map_err(boundary)?;
        let activation = frequencies::get_activation(pool, &activation_hash)
            .await?
            .ok_or_else(|| protocol("active Karma Frequency activation is missing"))?;
        match &activation.epoch.compiled().schedule {
            CompiledSchedule::Elapsed { .. } => {
                match materialize_elapsed_cursor(pool, &activation_hash, demand_policy, now).await?
                {
                    CursorMaterialization::Created(_) => result.created_elapsed += 1,
                    CursorMaterialization::Existing(_) => result.existing_elapsed += 1,
                }
            }
            CompiledSchedule::Calendar { .. } => result.calendar_pending_provider += 1,
        }
    }
    Ok(result)
}

pub async fn reconcile_active_calendar_cursors(
    pool: &SqlitePool,
    provider: &dyn TimeZoneProvider,
    demand_policy: ScheduleDemandPolicy,
    now: DateTime<Utc>,
) -> Result<CalendarScheduleReconciliation, StoreError> {
    let active_hashes = sqlx::query_scalar::<_, String>(
        "SELECT active_activation_hash FROM karma_frequency
         WHERE status = 'active' AND active_activation_hash IS NOT NULL
         ORDER BY active_activation_hash",
    )
    .fetch_all(pool)
    .await?;
    let mut result = CalendarScheduleReconciliation::default();
    for encoded in active_hashes {
        let activation_hash = CanonicalHash::parse(encoded).map_err(boundary)?;
        let activation = frequencies::get_activation(pool, &activation_hash)
            .await?
            .ok_or_else(|| protocol("active Karma Frequency activation is missing"))?;
        let CompiledSchedule::Calendar { schedule } = &activation.epoch.compiled().schedule else {
            continue;
        };
        if &schedule.tzdb != provider.revision() {
            continue;
        }
        result.matching_active += 1;
        let materialized =
            materialize_calendar_cursor(pool, &activation_hash, None, provider, demand_policy, now)
                .await?;
        match materialized {
            CursorMaterialization::Created(row) => {
                result.created += 1;
                if row.lifecycle == ScheduleCursorLifecycle::Paused {
                    result.paused += 1;
                }
            }
            CursorMaterialization::Existing(row) => {
                result.existing += 1;
                if row.lifecycle == ScheduleCursorLifecycle::Paused {
                    result.paused += 1;
                }
            }
        }
    }
    Ok(result)
}

pub async fn materialize_elapsed_cursor(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    demand_policy: ScheduleDemandPolicy,
    now: DateTime<Utc>,
) -> Result<CursorMaterialization, StoreError> {
    let activation = frequencies::get_activation(pool, activation_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let CompiledSchedule::Elapsed { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "elapsed cursor materialization requires an elapsed Frequency activation",
        ));
    };
    let cursor = schedule
        .initial_cursor(activation.epoch.activated_at())
        .map_err(boundary)?;
    let deadline = DeadlineEntry::new(
        activation_hash.clone(),
        1,
        cursor.next_intended_at(),
        schedule.timer(),
        schedule.overload_policy(),
    )
    .map_err(boundary)?;
    let demand =
        ScheduleDemand::for_elapsed(schedule, demand_policy.workload, demand_policy.calibration)
            .map_err(boundary)?;
    let stored_cursor = StoredScheduleCursor::Elapsed {
        cursor: cursor.clone(),
    };
    let at = canonical_timestamp(now)?;
    let mut tx = pool.begin().await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_frequency
            WHERE record_uid = ? AND status = 'active' AND active_activation_hash = ?
         )",
    )
    .bind(activation.epoch.frequency_uid())
    .bind(activation_hash.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        return Err(protocol(
            "only the currently active Frequency epoch can materialize a cursor",
        ));
    }

    supersede_other_cursors_tx(
        &mut tx,
        activation.epoch.frequency_uid(),
        activation_hash,
        &at,
    )
    .await?;
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO karma_schedule_cursor
            (activation_hash, frequency_uid, cursor_revision, cadence_kind, lifecycle,
             cursor_json, last_intended_at, next_intended_at,
             required_resolution_ms, max_lateness_ms, coalesce_window_ms,
             overload_policy, demand_json, created_at, updated_at)
         VALUES (?, ?, 1, 'elapsed', 'armed', ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(activation_hash.as_str())
    .bind(activation.epoch.frequency_uid())
    .bind(canonical_string(&stored_cursor)?)
    .bind(cursor.next_intended_at().to_string())
    .bind(i64::from(deadline.required_resolution_ms().get()))
    .bind(i64::from(deadline.max_lateness_ms()))
    .bind(i64::from(deadline.coalesce_window_ms()))
    .bind(overload_name(deadline.overload_policy()))
    .bind(canonical_string(&demand)?)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row = get_cursor_tx(&mut tx, activation_hash)
        .await?
        .expect("materialized cursor exists");
    validate_cursor_for_elapsed(&row, schedule)?;
    if (inserted && (row.cursor_revision != 1 || row.cursor != stored_cursor))
        || row.timer != schedule.timer()
        || row.overload_policy != schedule.overload_policy()
        || row.demand != demand
    {
        return Err(protocol(
            "existing Karma schedule cursor disagrees with its activation",
        ));
    }
    tx.commit().await?;
    Ok(if inserted {
        CursorMaterialization::Created(row)
    } else {
        CursorMaterialization::Existing(row)
    })
}

pub async fn materialize_calendar_cursor(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    previous: Option<CalendarBoundary>,
    provider: &dyn TimeZoneProvider,
    demand_policy: ScheduleDemandPolicy,
    now: DateTime<Utc>,
) -> Result<CursorMaterialization, StoreError> {
    let activation = frequencies::get_activation(pool, activation_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let CompiledSchedule::Calendar { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "calendar cursor materialization requires a calendar Frequency activation",
        ));
    };
    let resolution = resolve_calendar_cursor(schedule, provider, previous).map_err(boundary)?;
    let demand = ScheduleDemand::for_calendar(
        schedule,
        provider,
        demand_policy.workload,
        demand_policy.calibration,
    )
    .map_err(boundary)?;
    let (stored_cursor, lifecycle, last_intended_at, next_intended_at) = match resolution {
        CalendarCursorResolution::Armed(cursor) => (
            StoredScheduleCursor::Calendar {
                cursor: cursor.clone(),
            },
            ScheduleCursorLifecycle::Armed,
            cursor.previous().map(|value| value.intended_at),
            Some(cursor.next().intended_at),
        ),
        CalendarCursorResolution::Paused {
            previous,
            reason,
            skipped,
        } => (
            StoredScheduleCursor::CalendarPaused {
                previous,
                reason,
                skipped,
            },
            ScheduleCursorLifecycle::Paused,
            previous.map(|value| value.intended_at),
            None,
        ),
    };
    let at = canonical_timestamp(now)?;
    let mut tx = pool.begin().await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_frequency
            WHERE record_uid = ? AND status = 'active' AND active_activation_hash = ?
         )",
    )
    .bind(activation.epoch.frequency_uid())
    .bind(activation_hash.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        return Err(protocol(
            "only the currently active Frequency epoch can materialize a calendar cursor",
        ));
    }
    supersede_other_cursors_tx(
        &mut tx,
        activation.epoch.frequency_uid(),
        activation_hash,
        &at,
    )
    .await?;
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO karma_schedule_cursor
            (activation_hash, frequency_uid, cursor_revision, cadence_kind, lifecycle,
             cursor_json, last_intended_at, next_intended_at,
             required_resolution_ms, max_lateness_ms, coalesce_window_ms,
             overload_policy, demand_json, created_at, updated_at)
         VALUES (?, ?, 1, 'calendar', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(activation_hash.as_str())
    .bind(activation.epoch.frequency_uid())
    .bind(lifecycle.as_str())
    .bind(canonical_string(&stored_cursor)?)
    .bind(last_intended_at.map(|value| value.to_string()))
    .bind(next_intended_at.map(|value| value.to_string()))
    .bind(i64::from(schedule.timer.required_resolution_ms()))
    .bind(i64::from(schedule.timer.max_lateness_ms()))
    .bind(i64::from(schedule.timer.coalesce_window_ms()))
    .bind(overload_name(schedule.overload))
    .bind(canonical_string(&demand)?)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    let row = get_cursor_tx(&mut tx, activation_hash)
        .await?
        .expect("materialized calendar cursor exists");
    validate_cursor_for_calendar(&row, schedule, provider)?;
    if (inserted && (row.cursor_revision != 1 || row.cursor != stored_cursor))
        || row.demand != demand
    {
        return Err(protocol(
            "new Karma calendar cursor disagrees with its provider resolution",
        ));
    }
    tx.commit().await?;
    Ok(if inserted {
        CursorMaterialization::Created(row)
    } else {
        CursorMaterialization::Existing(row)
    })
}

pub async fn list_armed_deadlines(pool: &SqlitePool) -> Result<Vec<DeadlineEntry>, StoreError> {
    sqlx::query(
        "SELECT cursor.*
         FROM karma_schedule_cursor cursor
         JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
         WHERE cursor.lifecycle = 'armed' AND cursor.cadence_kind = 'elapsed'
           AND frequency.status = 'active'
           AND frequency.active_activation_hash = cursor.activation_hash
         ORDER BY cursor.next_intended_at, cursor.required_resolution_ms,
                  cursor.activation_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_deadline)
    .collect()
}

pub async fn list_armed_demanded_deadlines(
    pool: &SqlitePool,
) -> Result<Vec<DemandedDeadline>, StoreError> {
    sqlx::query(
        "SELECT cursor.*
         FROM karma_schedule_cursor cursor
         JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
         WHERE cursor.lifecycle = 'armed' AND cursor.cadence_kind = 'elapsed'
           AND frequency.status = 'active'
           AND frequency.active_activation_hash = cursor.activation_hash
         ORDER BY cursor.next_intended_at, cursor.required_resolution_ms,
                  cursor.activation_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_demanded_deadline)
    .collect()
}

pub async fn list_armed_calendar_deadlines(
    pool: &SqlitePool,
    provider: &dyn TimeZoneProvider,
) -> Result<Vec<DeadlineEntry>, StoreError> {
    let hashes = sqlx::query_scalar::<_, String>(
        "SELECT cursor.activation_hash
         FROM karma_schedule_cursor cursor
         JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
         WHERE cursor.lifecycle = 'armed' AND cursor.cadence_kind = 'calendar'
           AND frequency.status = 'active'
           AND frequency.active_activation_hash = cursor.activation_hash
         ORDER BY cursor.next_intended_at, cursor.required_resolution_ms,
                  cursor.activation_hash",
    )
    .fetch_all(pool)
    .await?;
    let mut deadlines = Vec::with_capacity(hashes.len());
    for encoded in hashes {
        let activation_hash = CanonicalHash::parse(encoded).map_err(boundary)?;
        let Some(cursor) = get_calendar_cursor(pool, &activation_hash, provider).await? else {
            continue;
        };
        deadlines.push(
            cursor
                .deadline
                .ok_or_else(|| protocol("armed Karma calendar cursor has no deadline"))?,
        );
    }
    Ok(deadlines)
}

pub async fn list_armed_demanded_calendar_deadlines(
    pool: &SqlitePool,
    provider: &dyn TimeZoneProvider,
) -> Result<Vec<DemandedDeadline>, StoreError> {
    let hashes = sqlx::query_scalar::<_, String>(
        "SELECT cursor.activation_hash
         FROM karma_schedule_cursor cursor
         JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
         WHERE cursor.lifecycle = 'armed' AND cursor.cadence_kind = 'calendar'
           AND frequency.status = 'active'
           AND frequency.active_activation_hash = cursor.activation_hash
         ORDER BY cursor.next_intended_at, cursor.required_resolution_ms,
                  cursor.activation_hash",
    )
    .fetch_all(pool)
    .await?;
    let mut deadlines = Vec::with_capacity(hashes.len());
    for encoded in hashes {
        let activation_hash = CanonicalHash::parse(encoded).map_err(boundary)?;
        let Some(cursor) = get_calendar_cursor(pool, &activation_hash, provider).await? else {
            continue;
        };
        let entry = cursor
            .deadline
            .ok_or_else(|| protocol("armed Karma calendar cursor has no deadline"))?;
        deadlines.push(DemandedDeadline::new(entry, cursor.demand).map_err(boundary)?);
    }
    Ok(deadlines)
}

pub async fn get_cursor(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
) -> Result<Option<ScheduleCursorRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_schedule_cursor WHERE activation_hash = ?")
        .bind(activation_hash.as_str())
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let cursor = map_cursor(row)?;
    let activation = frequencies::get_activation(pool, activation_hash)
        .await?
        .ok_or_else(|| protocol("Karma schedule cursor activation is missing"))?;
    let CompiledSchedule::Elapsed { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "calendar cursor decoding requires the pinned timezone provider",
        ));
    };
    validate_cursor_for_elapsed(&cursor, schedule)?;
    Ok(Some(cursor))
}

pub async fn get_calendar_cursor(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    provider: &dyn TimeZoneProvider,
) -> Result<Option<ScheduleCursorRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_schedule_cursor WHERE activation_hash = ?")
        .bind(activation_hash.as_str())
        .fetch_optional(pool)
        .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let cursor = map_cursor(row)?;
    let activation = frequencies::get_activation(pool, activation_hash)
        .await?
        .ok_or_else(|| protocol("Karma calendar cursor activation is missing"))?;
    let CompiledSchedule::Calendar { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "calendar cursor is attached to a non-calendar activation",
        ));
    };
    validate_cursor_for_calendar(&cursor, schedule, provider)?;
    Ok(Some(cursor))
}

pub async fn get_occurrence(
    pool: &SqlitePool,
    occurrence_hash: &CanonicalHash,
) -> Result<Option<StoredScheduleOccurrence>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_schedule_occurrence WHERE occurrence_hash = ?")
        .bind(occurrence_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_occurrence).transpose()
}

pub async fn list_cursors(pool: &SqlitePool) -> Result<Vec<ScheduleCursorRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_schedule_cursor
         ORDER BY frequency_uid, created_at, activation_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_cursor)
    .collect()
}

pub async fn list_occurrences(
    pool: &SqlitePool,
) -> Result<Vec<StoredScheduleOccurrence>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_schedule_occurrence
         ORDER BY activation_hash, sequence, occurrence_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_occurrence)
    .collect()
}

pub async fn claim_due(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    expected_cursor_revision: u64,
    worker_id: &str,
    observed_at: DateTime<Utc>,
    lease_duration: DurationMs,
) -> Result<ScheduleClaim, StoreError> {
    validate_worker_id(worker_id)?;
    if lease_duration.get() <= 0 {
        return Err(protocol("Karma schedule lease duration must be positive"));
    }
    let observed_at = timestamp(observed_at)?;
    let expires_at = observed_at
        .checked_add(lease_duration)
        .ok_or_else(|| protocol("Karma schedule lease expiry overflows"))?;
    let mut tx = pool.begin().await?;
    let current = get_cursor_tx(&mut tx, activation_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    if current.cursor_revision != expected_cursor_revision {
        tx.rollback().await?;
        return Ok(ScheduleClaim::Stale {
            current_cursor_revision: current.cursor_revision,
        });
    }
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_frequency
            WHERE record_uid = ? AND status = 'active' AND active_activation_hash = ?
         )",
    )
    .bind(&current.frequency_uid)
    .bind(activation_hash.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        tx.rollback().await?;
        return Ok(ScheduleClaim::Inactive);
    }
    if current.admitted_resolution_ms.is_none() {
        return Err(protocol(
            "Karma schedule cursor cannot be claimed before deadline admission",
        ));
    }
    let current_deadline = current
        .deadline
        .as_ref()
        .ok_or_else(|| protocol("armed Karma schedule cursor has no deadline"))?;
    if current_deadline.next_intended_at() > observed_at {
        return Err(protocol("Karma schedule cursor is not due yet"));
    }
    let reclaimable = current.lifecycle == ScheduleCursorLifecycle::Armed
        || (current.lifecycle == ScheduleCursorLifecycle::Leased
            && current
                .lease_expires_at
                .is_some_and(|expiry| expiry <= observed_at));
    if !reclaimable {
        let lease_expires_at = current
            .lease_expires_at
            .ok_or_else(|| protocol("leased Karma schedule cursor has no expiry"))?;
        tx.rollback().await?;
        return Ok(ScheduleClaim::Contended { lease_expires_at });
    }
    let fencing_token = current
        .lease_fencing_token
        .checked_add(1)
        .ok_or_else(|| protocol("Karma schedule fencing token overflowed"))?;
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET lifecycle = 'leased', lease_fencing_token = ?, lease_owner = ?,
             lease_expires_at = ?, updated_at = ?
         WHERE activation_hash = ? AND cursor_revision = ?
           AND (lifecycle = 'armed' OR (lifecycle = 'leased' AND lease_expires_at <= ?))",
    )
    .bind(sqlite_u64(fencing_token, "fencing token")?)
    .bind(worker_id)
    .bind(expires_at.to_string())
    .bind(observed_at.to_string())
    .bind(activation_hash.as_str())
    .bind(sqlite_u64(expected_cursor_revision, "cursor revision")?)
    .bind(observed_at.to_string())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        let current = sqlx::query(
            "SELECT cursor_revision, lifecycle, lease_expires_at
             FROM karma_schedule_cursor WHERE activation_hash = ?",
        )
        .bind(activation_hash.as_str())
        .fetch_one(pool)
        .await?;
        let lifecycle: String = current.get("lifecycle");
        if lifecycle == ScheduleCursorLifecycle::Leased.as_str() {
            let encoded: Option<String> = current.get("lease_expires_at");
            let lease_expires_at = encoded
                .map(parse_timestamp)
                .transpose()?
                .ok_or_else(|| protocol("leased Karma schedule cursor has no expiry"))?;
            return Ok(ScheduleClaim::Contended { lease_expires_at });
        }
        return Ok(ScheduleClaim::Stale {
            current_cursor_revision: rust_u64(current.get("cursor_revision"), "cursor revision")?,
        });
    }
    let cursor = get_cursor_tx(&mut tx, activation_hash)
        .await?
        .expect("claimed cursor exists");
    tx.commit().await?;
    Ok(ScheduleClaim::Claimed(ScheduleLease {
        cursor,
        worker_id: worker_id.to_string(),
        fencing_token,
        expires_at,
    }))
}

/// Fence leases whose exact expiry has passed so a directory rebuild can arm
/// them again. This is called at boot/change or from a one-shot lease recovery
/// arm; it is never a polling loop.
pub async fn recover_expired_schedule_leases(
    pool: &SqlitePool,
    now: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let now = canonical_timestamp(now)?;
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET lifecycle = 'armed', lease_owner = NULL, lease_expires_at = NULL,
             updated_at = ?
         WHERE lifecycle = 'leased' AND lease_expires_at <= ?
           AND EXISTS (
               SELECT 1 FROM karma_frequency frequency
               WHERE frequency.record_uid = karma_schedule_cursor.frequency_uid
                 AND frequency.status = 'active'
                 AND frequency.active_activation_hash = karma_schedule_cursor.activation_hash
           )",
    )
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected())
}

pub async fn next_active_schedule_lease_expiry(
    pool: &SqlitePool,
) -> Result<Option<TimestampMs>, StoreError> {
    let encoded = sqlx::query_scalar::<_, Option<String>>(
        "SELECT MIN(cursor.lease_expires_at)
         FROM karma_schedule_cursor cursor
         JOIN karma_frequency frequency ON frequency.record_uid = cursor.frequency_uid
         WHERE cursor.lifecycle = 'leased'
           AND frequency.status = 'active'
           AND frequency.active_activation_hash = cursor.activation_hash",
    )
    .fetch_one(pool)
    .await?;
    encoded.map(parse_timestamp).transpose()
}

pub async fn complete_elapsed(
    pool: &SqlitePool,
    lease: &ScheduleLease,
    observed_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
) -> Result<CursorCompletion, StoreError> {
    let activation = frequencies::get_activation(pool, &lease.cursor.activation_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let CompiledSchedule::Elapsed { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "elapsed cursor completion requires an elapsed Frequency activation",
        ));
    };
    validate_cursor_for_elapsed(&lease.cursor, schedule)?;
    let observed_at = timestamp(observed_at)?;
    let completed_at = timestamp(completed_at)?;
    if completed_at < observed_at {
        return Err(protocol(
            "Karma schedule completion cannot precede observation",
        ));
    }
    let elapsed_cursor = lease
        .cursor
        .cursor
        .elapsed()
        .ok_or_else(|| protocol("elapsed completion received a calendar cursor"))?;
    let advance = schedule
        .advance(elapsed_cursor, observed_at)
        .map_err(boundary)?
        .ok_or_else(|| protocol("claimed Karma schedule cursor was not due"))?;
    let next_revision = lease
        .cursor
        .cursor_revision
        .checked_add(1)
        .ok_or_else(|| protocol("Karma schedule cursor revision overflowed"))?;
    let next_sequence = if advance.emission.is_some() {
        Some(
            lease
                .cursor
                .last_occurrence_sequence
                .checked_add(1)
                .ok_or_else(|| protocol("Karma schedule occurrence sequence overflowed"))?,
        )
    } else {
        None
    };
    let occurrence = next_sequence
        .map(|sequence| {
            let emission = advance
                .emission
                .as_ref()
                .expect("an occurrence sequence exists only for an emitted advance");
            let batch = OccurrenceBatch::new(
                lease.cursor.activation_hash.clone(),
                sequence,
                schedule,
                emission,
            )
            .map_err(boundary)?;
            ElapsedScheduleOccurrence::new(
                lease.cursor.activation_hash.clone(),
                sequence,
                lease.cursor.cursor_revision,
                lease.fencing_token,
                observed_at,
                batch,
                advance.clone(),
            )
            .map_err(boundary)
        })
        .transpose()?;
    let stored_occurrence = occurrence
        .map(|occurrence| {
            let occurrence_hash = occurrence.occurrence_hash().map_err(boundary)?;
            Ok::<_, StoreError>(StoredScheduleOccurrence {
                occurrence_hash,
                occurrence: ScheduleOccurrencePayload::Elapsed { occurrence },
                created_at: completed_at.to_string(),
            })
        })
        .transpose()?;

    let mut tx = pool.begin().await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_frequency
            WHERE record_uid = ? AND status = 'active' AND active_activation_hash = ?
         )",
    )
    .bind(&lease.cursor.frequency_uid)
    .bind(lease.cursor.activation_hash.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        tx.rollback().await?;
        return Ok(CursorCompletion::LostLease);
    }
    let lifecycle = if advance.paused {
        ScheduleCursorLifecycle::Paused
    } else {
        ScheduleCursorLifecycle::Armed
    };
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = ?, lifecycle = ?, cursor_json = ?,
             last_intended_at = ?, next_intended_at = ?,
             lease_owner = NULL, lease_expires_at = NULL,
             last_occurrence_sequence = ?, updated_at = ?
         WHERE activation_hash = ? AND cursor_revision = ? AND lifecycle = 'leased'
           AND lease_fencing_token = ? AND lease_owner = ? AND lease_expires_at >= ?",
    )
    .bind(sqlite_u64(next_revision, "cursor revision")?)
    .bind(lifecycle.as_str())
    .bind(canonical_string(&StoredScheduleCursor::Elapsed {
        cursor: advance.cursor.clone(),
    })?)
    .bind(
        advance
            .cursor
            .last_intended_at()
            .map(|value| value.to_string()),
    )
    .bind(advance.cursor.next_intended_at().to_string())
    .bind(sqlite_u64(
        next_sequence.unwrap_or(lease.cursor.last_occurrence_sequence),
        "occurrence sequence",
    )?)
    .bind(completed_at.to_string())
    .bind(lease.cursor.activation_hash.as_str())
    .bind(sqlite_u64(lease.cursor.cursor_revision, "cursor revision")?)
    .bind(sqlite_u64(lease.fencing_token, "fencing token")?)
    .bind(&lease.worker_id)
    .bind(completed_at.to_string())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(CursorCompletion::LostLease);
    }
    if let Some(stored) = &stored_occurrence {
        insert_occurrence(&mut tx, stored).await?;
    }
    let cursor = get_cursor_tx(&mut tx, &lease.cursor.activation_hash)
        .await?
        .expect("completed cursor exists");
    validate_cursor_for_elapsed(&cursor, schedule)?;
    tx.commit().await?;
    Ok(CursorCompletion::Completed {
        cursor,
        occurrence: stored_occurrence,
    })
}

pub async fn complete_calendar(
    pool: &SqlitePool,
    lease: &ScheduleLease,
    provider: &dyn TimeZoneProvider,
    observed_at: DateTime<Utc>,
    completed_at: DateTime<Utc>,
    catch_up_budget: NonZeroU32,
) -> Result<CursorCompletion, StoreError> {
    let activation = frequencies::get_activation(pool, &lease.cursor.activation_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let CompiledSchedule::Calendar { schedule } = &activation.epoch.compiled().schedule else {
        return Err(protocol(
            "calendar cursor completion requires a calendar Frequency activation",
        ));
    };
    validate_cursor_for_calendar(&lease.cursor, schedule, provider)?;
    let StoredScheduleCursor::Calendar {
        cursor: calendar_cursor,
    } = &lease.cursor.cursor
    else {
        return Err(protocol("leased calendar cursor is not armed"));
    };
    let observed_at = timestamp(observed_at)?;
    let completed_at = timestamp(completed_at)?;
    if completed_at < observed_at {
        return Err(protocol(
            "Karma calendar completion cannot precede observation",
        ));
    }
    let catch_up = advance_calendar_cursor(
        schedule,
        provider,
        calendar_cursor,
        observed_at,
        catch_up_budget,
    )
    .map_err(boundary)?
    .ok_or_else(|| protocol("claimed Karma calendar cursor was not due"))?;
    let next_revision = lease
        .cursor
        .cursor_revision
        .checked_add(1)
        .ok_or_else(|| protocol("Karma calendar cursor revision overflowed"))?;
    let next_sequence = if catch_up.emission.is_some() {
        Some(
            lease
                .cursor
                .last_occurrence_sequence
                .checked_add(1)
                .ok_or_else(|| protocol("Karma calendar occurrence sequence overflowed"))?,
        )
    } else {
        None
    };
    let stored_occurrence = next_sequence
        .map(|sequence| {
            let occurrence = CalendarScheduleOccurrence::new(
                lease.cursor.activation_hash.clone(),
                sequence,
                lease.cursor.cursor_revision,
                lease.fencing_token,
                observed_at,
                catch_up.clone(),
            )
            .map_err(boundary)?;
            let occurrence_hash = occurrence.occurrence_hash().map_err(boundary)?;
            Ok::<_, StoreError>(StoredScheduleOccurrence {
                occurrence_hash,
                occurrence: ScheduleOccurrencePayload::Calendar { occurrence },
                created_at: completed_at.to_string(),
            })
        })
        .transpose()?;
    let (stored_cursor, lifecycle) = match (&catch_up.pause, &catch_up.next_cursor) {
        (None, Some(cursor)) => (
            StoredScheduleCursor::Calendar {
                cursor: cursor.clone(),
            },
            ScheduleCursorLifecycle::Armed,
        ),
        (Some(CalendarCatchUpPause::Lag), Some(cursor)) => (
            StoredScheduleCursor::Calendar {
                cursor: cursor.clone(),
            },
            ScheduleCursorLifecycle::Paused,
        ),
        (Some(CalendarCatchUpPause::Discontinuity { reason }), None) => {
            let previous = catch_up.due.last().copied();
            let CalendarCursorResolution::Paused {
                reason: resolved_reason,
                skipped,
                ..
            } = resolve_calendar_cursor(schedule, provider, previous).map_err(boundary)?
            else {
                return Err(protocol(
                    "Karma calendar discontinuity pause no longer reproduces",
                ));
            };
            if resolved_reason != *reason {
                return Err(protocol(
                    "Karma calendar discontinuity reason changed during completion",
                ));
            }
            (
                StoredScheduleCursor::CalendarPaused {
                    previous,
                    reason: *reason,
                    skipped,
                },
                ScheduleCursorLifecycle::Paused,
            )
        }
        _ => {
            return Err(protocol(
                "Karma calendar catch-up produced incoherent next/pause state",
            ));
        }
    };

    let mut tx = pool.begin().await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM karma_frequency
            WHERE record_uid = ? AND status = 'active' AND active_activation_hash = ?
         )",
    )
    .bind(&lease.cursor.frequency_uid)
    .bind(lease.cursor.activation_hash.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if !active {
        tx.rollback().await?;
        return Ok(CursorCompletion::LostLease);
    }
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = ?, lifecycle = ?, cursor_json = ?,
             last_intended_at = ?, next_intended_at = ?,
             lease_owner = NULL, lease_expires_at = NULL,
             last_occurrence_sequence = ?, updated_at = ?
         WHERE activation_hash = ? AND cursor_revision = ? AND lifecycle = 'leased'
           AND lease_fencing_token = ? AND lease_owner = ? AND lease_expires_at >= ?",
    )
    .bind(sqlite_u64(next_revision, "calendar cursor revision")?)
    .bind(lifecycle.as_str())
    .bind(canonical_string(&stored_cursor)?)
    .bind(
        stored_cursor
            .last_intended_at()
            .map(|value| value.to_string()),
    )
    .bind(
        stored_cursor
            .next_intended_at()
            .map(|value| value.to_string()),
    )
    .bind(sqlite_u64(
        next_sequence.unwrap_or(lease.cursor.last_occurrence_sequence),
        "calendar occurrence sequence",
    )?)
    .bind(completed_at.to_string())
    .bind(lease.cursor.activation_hash.as_str())
    .bind(sqlite_u64(
        lease.cursor.cursor_revision,
        "calendar cursor revision",
    )?)
    .bind(sqlite_u64(lease.fencing_token, "calendar fencing token")?)
    .bind(&lease.worker_id)
    .bind(completed_at.to_string())
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(CursorCompletion::LostLease);
    }
    if let Some(stored) = &stored_occurrence {
        insert_occurrence(&mut tx, stored).await?;
    }
    let cursor = get_cursor_tx(&mut tx, &lease.cursor.activation_hash)
        .await?
        .expect("completed calendar cursor exists");
    validate_cursor_for_calendar(&cursor, schedule, provider)?;
    tx.commit().await?;
    Ok(CursorCompletion::Completed {
        cursor,
        occurrence: stored_occurrence,
    })
}

pub async fn supersede_inactive_frequency_cursors(
    pool: &SqlitePool,
    frequency_uid: &str,
    now: DateTime<Utc>,
) -> Result<u64, StoreError> {
    let at = canonical_timestamp(now)?;
    let changed = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = cursor_revision + 1, lifecycle = 'superseded',
             lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE frequency_uid = ? AND lifecycle != 'superseded'
           AND activation_hash NOT IN (
               SELECT active_activation_hash FROM karma_frequency
               WHERE record_uid = ? AND status = 'active'
                 AND active_activation_hash IS NOT NULL
           )",
    )
    .bind(&at)
    .bind(frequency_uid)
    .bind(frequency_uid)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(changed)
}

pub async fn park_deadline_admission(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    expected_cursor_revision: u64,
    reason: DeadlineRejectionReason,
    now: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let at = canonical_timestamp(now)?;
    let error_json = admission_error_json(reason)?;
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = cursor_revision + 1, lifecycle = 'paused',
             admitted_resolution_ms = NULL, admission_degraded = 0,
             admitted_at = NULL, last_error_json = ?, updated_at = ?
         WHERE activation_hash = ? AND cursor_revision = ? AND lifecycle = 'armed'",
    )
    .bind(error_json)
    .bind(&at)
    .bind(activation_hash.as_str())
    .bind(sqlite_u64(expected_cursor_revision, "cursor revision")?)
    .execute(pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Ok(false);
    }
    Ok(true)
}

pub async fn record_deadline_admission(
    pool: &SqlitePool,
    activation_hash: &CanonicalHash,
    expected_cursor_revision: u64,
    lane_resolution_ms: NonZeroU32,
    degraded: bool,
    now: DateTime<Utc>,
) -> Result<bool, StoreError> {
    let at = canonical_timestamp(now)?;
    let updated = sqlx::query(
        "UPDATE karma_schedule_cursor
         SET admitted_resolution_ms = ?, admission_degraded = ?, admitted_at = ?,
             last_error_json = NULL, updated_at = ?
         WHERE activation_hash = ? AND cursor_revision = ? AND lifecycle = 'armed'",
    )
    .bind(i64::from(lane_resolution_ms.get()))
    .bind(if degraded { 1_i64 } else { 0_i64 })
    .bind(&at)
    .bind(&at)
    .bind(activation_hash.as_str())
    .bind(sqlite_u64(expected_cursor_revision, "cursor revision")?)
    .execute(pool)
    .await?;
    Ok(updated.rows_affected() == 1)
}

fn admission_error_json(reason: DeadlineRejectionReason) -> Result<String, StoreError> {
    #[derive(Serialize)]
    struct AdmissionError {
        schema: &'static str,
        reason: DeadlineRejectionReason,
    }

    canonical_string(&AdmissionError {
        schema: "karma.deadline-admission-error.v1",
        reason,
    })
}

async fn supersede_other_cursors_tx(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
    active_activation_hash: &CanonicalHash,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = cursor_revision + 1, lifecycle = 'superseded',
             lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE frequency_uid = ? AND activation_hash != ?
           AND lifecycle != 'superseded'",
    )
    .bind(at)
    .bind(frequency_uid)
    .bind(active_activation_hash.as_str())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn supersede_frequency_cursors_tx(
    tx: &mut Transaction<'_, Sqlite>,
    frequency_uid: &str,
    at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE karma_schedule_cursor
         SET cursor_revision = cursor_revision + 1, lifecycle = 'superseded',
             lease_owner = NULL, lease_expires_at = NULL, updated_at = ?
         WHERE frequency_uid = ? AND lifecycle != 'superseded'",
    )
    .bind(at)
    .bind(frequency_uid)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_occurrence(
    tx: &mut Transaction<'_, Sqlite>,
    stored: &StoredScheduleOccurrence,
) -> Result<(), StoreError> {
    let projection = stored.occurrence.projection()?;
    sqlx::query(
        "INSERT INTO karma_schedule_occurrence
            (occurrence_hash, cadence_kind, activation_hash, sequence, claimed_cursor_revision,
             lease_fencing_token, observed_at, emission_kind, first_intended_at,
             last_intended_at, covered_boundary_count, semantic_occurrence_count,
             occurrence_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(stored.occurrence_hash.as_str())
    .bind(stored.occurrence.cadence_name())
    .bind(stored.occurrence.activation_hash().as_str())
    .bind(sqlite_u64(
        stored.occurrence.sequence(),
        "occurrence sequence",
    )?)
    .bind(sqlite_u64(
        stored.occurrence.claimed_cursor_revision(),
        "claimed cursor revision",
    )?)
    .bind(sqlite_u64(
        stored.occurrence.lease_fencing_token(),
        "fencing token",
    )?)
    .bind(stored.occurrence.observed_at().to_string())
    .bind(projection.emission_kind)
    .bind(projection.first_intended_at.to_string())
    .bind(projection.last_intended_at.to_string())
    .bind(sqlite_u64(
        projection.covered_boundary_count,
        "covered boundary count",
    )?)
    .bind(sqlite_u64(
        projection.semantic_occurrence_count,
        "semantic occurrence count",
    )?)
    .bind(stored.occurrence.occurrence_json()?)
    .bind(&stored.created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn get_cursor_tx(
    tx: &mut Transaction<'_, Sqlite>,
    activation_hash: &CanonicalHash,
) -> Result<Option<ScheduleCursorRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_schedule_cursor WHERE activation_hash = ?")
        .bind(activation_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_cursor).transpose()
}

fn map_cursor(row: sqlx::sqlite::SqliteRow) -> Result<ScheduleCursorRow, StoreError> {
    let activation_hash = parse_hash(row.get("activation_hash"))?;
    let cursor_revision = rust_u64(row.get("cursor_revision"), "cursor revision")?;
    let lifecycle_name: String = row.get("lifecycle");
    let lifecycle = ScheduleCursorLifecycle::parse(&lifecycle_name)
        .ok_or_else(|| protocol("stored Karma schedule cursor lifecycle is invalid"))?;
    let cadence_kind: String = row.get("cadence_kind");
    let cursor_json: String = row.get("cursor_json");
    let cursor: StoredScheduleCursor = serde_json::from_str(&cursor_json).map_err(json_protocol)?;
    if canonical_string(&cursor)? != cursor_json {
        return Err(protocol(
            "stored Karma schedule cursor is not canonical JSON",
        ));
    }
    let valid_cadence = matches!(
        (cadence_kind.as_str(), &cursor),
        ("elapsed", StoredScheduleCursor::Elapsed { .. })
            | ("calendar", StoredScheduleCursor::Calendar { .. })
            | ("calendar", StoredScheduleCursor::CalendarPaused { .. })
    );
    if !valid_cadence {
        return Err(protocol(
            "stored Karma schedule cursor cadence kind disagrees with its payload",
        ));
    }
    let next_intended_at = row
        .get::<Option<String>, _>("next_intended_at")
        .map(parse_timestamp)
        .transpose()?;
    let last_intended_at = row
        .get::<Option<String>, _>("last_intended_at")
        .map(parse_timestamp)
        .transpose()?;
    if cursor.next_intended_at() != next_intended_at
        || cursor.last_intended_at() != last_intended_at
    {
        return Err(protocol(
            "stored Karma schedule cursor projections disagree with cursor JSON",
        ));
    }
    let timer = TimerPolicy::new(
        rust_u32(row.get("required_resolution_ms"), "required resolution")?,
        rust_u32(row.get("max_lateness_ms"), "maximum lateness")?,
        rust_u32(row.get("coalesce_window_ms"), "coalescing window")?,
    )
    .map_err(boundary)?;
    let overload_name: String = row.get("overload_policy");
    let overload = parse_overload(&overload_name)?;
    let demand_json: String = row.get("demand_json");
    let demand: ScheduleDemand = serde_json::from_str(&demand_json).map_err(json_protocol)?;
    if canonical_string(&demand)? != demand_json {
        return Err(protocol(
            "stored Karma schedule demand is not canonical JSON",
        ));
    }
    let admitted_resolution_ms = row
        .get::<Option<i64>, _>("admitted_resolution_ms")
        .map(|value| {
            let value = rust_u32(value, "admitted resolution")?;
            NonZeroU32::new(value)
                .ok_or_else(|| protocol("stored admitted resolution must be positive"))
        })
        .transpose()?;
    let admission_degraded = match row.get::<i64, _>("admission_degraded") {
        0 => false,
        1 => true,
        _ => return Err(protocol("stored admission degradation flag is invalid")),
    };
    let admitted_at: Option<String> = row.get("admitted_at");
    if admitted_resolution_ms.is_none() != admitted_at.is_none()
        || (admitted_resolution_ms.is_none() && admission_degraded)
    {
        return Err(protocol(
            "stored Karma schedule admission diagnostics are inconsistent",
        ));
    }
    let deadline = next_intended_at
        .map(|next_intended_at| {
            DeadlineEntry::new(
                activation_hash.clone(),
                cursor_revision,
                next_intended_at,
                timer,
                overload,
            )
            .map_err(boundary)
        })
        .transpose()?;
    let lease_owner: Option<String> = row.get("lease_owner");
    if let Some(worker_id) = &lease_owner {
        validate_worker_id(worker_id)?;
    }
    let lease_expires_at = row
        .get::<Option<String>, _>("lease_expires_at")
        .map(parse_timestamp)
        .transpose()?;
    if matches!(
        lifecycle,
        ScheduleCursorLifecycle::Armed | ScheduleCursorLifecycle::Leased
    ) && deadline.is_none()
    {
        return Err(protocol(
            "armed or leased Karma schedule cursor has no deadline",
        ));
    }
    if (lifecycle == ScheduleCursorLifecycle::Leased)
        != (lease_owner.is_some() && lease_expires_at.is_some())
    {
        return Err(protocol(
            "stored Karma schedule cursor lease state is inconsistent",
        ));
    }
    let last_error_json: Option<String> = row.get("last_error_json");
    if let Some(value) = &last_error_json {
        let parsed: serde_json::Value = serde_json::from_str(value).map_err(json_protocol)?;
        if canonical_string(&parsed)? != *value {
            return Err(protocol(
                "stored Karma schedule cursor error is not canonical JSON",
            ));
        }
    }
    Ok(ScheduleCursorRow {
        activation_hash,
        frequency_uid: row.get("frequency_uid"),
        cursor_revision,
        lifecycle,
        cursor,
        deadline,
        timer,
        overload_policy: overload,
        demand,
        admitted_resolution_ms,
        admission_degraded,
        admitted_at,
        lease_fencing_token: rust_u64(row.get("lease_fencing_token"), "fencing token")?,
        lease_owner,
        lease_expires_at,
        last_occurrence_sequence: rust_u64(
            row.get("last_occurrence_sequence"),
            "occurrence sequence",
        )?,
        last_error_json,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn map_demanded_deadline(row: sqlx::sqlite::SqliteRow) -> Result<DemandedDeadline, StoreError> {
    let demand_json: String = row.get("demand_json");
    let entry = map_deadline(row)?;
    let demand: ScheduleDemand = serde_json::from_str(&demand_json).map_err(json_protocol)?;
    if canonical_string(&demand)? != demand_json {
        return Err(protocol(
            "stored Karma schedule demand is not canonical JSON",
        ));
    }
    DemandedDeadline::new(entry, demand).map_err(boundary)
}

fn map_occurrence(row: sqlx::sqlite::SqliteRow) -> Result<StoredScheduleOccurrence, StoreError> {
    let occurrence_hash = parse_hash(row.get("occurrence_hash"))?;
    let occurrence_json: String = row.get("occurrence_json");
    let cadence_kind: String = row.get("cadence_kind");
    let activation_hash = parse_hash(row.get("activation_hash"))?;
    let sequence = rust_u64(row.get("sequence"), "occurrence sequence")?;
    let claimed_cursor_revision = rust_u64(
        row.get("claimed_cursor_revision"),
        "claimed cursor revision",
    )?;
    let lease_fencing_token = rust_u64(row.get("lease_fencing_token"), "fencing token")?;
    let observed_at = parse_timestamp(row.get("observed_at"))?;
    let occurrence = match cadence_kind.as_str() {
        "elapsed" => {
            let occurrence: ElapsedScheduleOccurrence =
                serde_json::from_str(&occurrence_json).map_err(json_protocol)?;
            if canonical_string(&occurrence)? != occurrence_json {
                return Err(protocol(
                    "stored Karma elapsed occurrence is not canonical JSON",
                ));
            }
            ScheduleOccurrencePayload::Elapsed { occurrence }
        }
        "calendar" => {
            let occurrence: CalendarScheduleOccurrence =
                serde_json::from_str(&occurrence_json).map_err(json_protocol)?;
            if canonical_string(&occurrence)? != occurrence_json {
                return Err(protocol(
                    "stored Karma calendar occurrence is not canonical JSON",
                ));
            }
            ScheduleOccurrencePayload::Calendar { occurrence }
        }
        _ => return Err(protocol("stored Karma occurrence cadence is invalid")),
    };
    if occurrence.activation_hash() != &activation_hash
        || occurrence.sequence() != sequence
        || occurrence.claimed_cursor_revision() != claimed_cursor_revision
        || occurrence.lease_fencing_token() != lease_fencing_token
        || occurrence.observed_at() != observed_at
    {
        return Err(protocol(
            "stored Karma schedule occurrence projections disagree with its envelope",
        ));
    }
    let expected_projection = occurrence.projection()?;
    let stored_projection = OccurrenceProjection {
        emission_kind: match row.get::<String, _>("emission_kind").as_str() {
            "individual" => "individual",
            "coalesced" => "coalesced",
            _ => return Err(protocol("stored occurrence emission kind is invalid")),
        },
        first_intended_at: parse_timestamp(row.get("first_intended_at"))?,
        last_intended_at: parse_timestamp(row.get("last_intended_at"))?,
        covered_boundary_count: rust_u64(
            row.get("covered_boundary_count"),
            "covered boundary count",
        )?,
        semantic_occurrence_count: rust_u64(
            row.get("semantic_occurrence_count"),
            "semantic occurrence count",
        )?,
    };
    if stored_projection != expected_projection {
        return Err(protocol(
            "stored Karma schedule occurrence batch projections are invalid",
        ));
    }
    let computed_hash = match &occurrence {
        ScheduleOccurrencePayload::Elapsed { occurrence } => {
            occurrence.occurrence_hash().map_err(boundary)?
        }
        ScheduleOccurrencePayload::Calendar { occurrence } => {
            occurrence.occurrence_hash().map_err(boundary)?
        }
    };
    if computed_hash != occurrence_hash {
        return Err(protocol("stored Karma schedule occurrence hash is invalid"));
    }
    Ok(StoredScheduleOccurrence {
        occurrence_hash,
        occurrence,
        created_at: row.get("created_at"),
    })
}

fn map_deadline(row: sqlx::sqlite::SqliteRow) -> Result<DeadlineEntry, StoreError> {
    let activation_hash = parse_hash(row.get("activation_hash"))?;
    let timer = TimerPolicy::new(
        rust_u32(row.get("required_resolution_ms"), "required resolution")?,
        rust_u32(row.get("max_lateness_ms"), "maximum lateness")?,
        rust_u32(row.get("coalesce_window_ms"), "coalescing window")?,
    )
    .map_err(boundary)?;
    DeadlineEntry::new(
        activation_hash,
        rust_u64(row.get("cursor_revision"), "cursor revision")?,
        parse_timestamp(row.get("next_intended_at"))?,
        timer,
        parse_overload(&row.get::<String, _>("overload_policy"))?,
    )
    .map_err(boundary)
}

fn validate_cursor_for_elapsed(
    row: &ScheduleCursorRow,
    schedule: &nucleus::karma::ElapsedSchedule,
) -> Result<(), StoreError> {
    let cursor = row
        .cursor
        .elapsed()
        .ok_or_else(|| protocol("elapsed Frequency has a non-elapsed cursor"))?;
    let deadline = row
        .deadline
        .as_ref()
        .ok_or_else(|| protocol("elapsed Frequency cursor has no deadline"))?;
    let reconstructed = schedule
        .cursor(cursor.last_intended_at(), cursor.next_intended_at())
        .map_err(boundary)?;
    if &reconstructed != cursor
        || row.timer != schedule.timer()
        || row.overload_policy != schedule.overload_policy()
        || deadline.required_resolution_ms().get() != row.timer.required_resolution_ms()
        || deadline.max_lateness_ms() != row.timer.max_lateness_ms()
        || deadline.coalesce_window_ms() != row.timer.coalesce_window_ms()
        || deadline.overload_policy() != row.overload_policy
    {
        return Err(protocol(
            "stored Karma schedule cursor disagrees with its compiled Frequency",
        ));
    }
    Ok(())
}

fn validate_cursor_for_calendar(
    row: &ScheduleCursorRow,
    schedule: &nucleus::karma::CalendarSchedule,
    provider: &dyn TimeZoneProvider,
) -> Result<(), StoreError> {
    if row.timer != schedule.timer || row.overload_policy != schedule.overload {
        return Err(protocol(
            "stored Karma calendar cursor timer contract disagrees with its activation",
        ));
    }
    match &row.cursor {
        StoredScheduleCursor::Calendar { cursor } => {
            cursor.validate_for(schedule, provider).map_err(boundary)?;
            let deadline = row
                .deadline
                .as_ref()
                .ok_or_else(|| protocol("armed Karma calendar cursor has no deadline"))?;
            if deadline.next_intended_at() != cursor.next().intended_at
                || deadline.required_resolution_ms().get()
                    != schedule.timer.required_resolution_ms()
                || deadline.max_lateness_ms() != schedule.timer.max_lateness_ms()
                || deadline.coalesce_window_ms() != schedule.timer.coalesce_window_ms()
                || deadline.overload_policy() != schedule.overload
            {
                return Err(protocol(
                    "stored Karma calendar deadline disagrees with its cursor",
                ));
            }
        }
        StoredScheduleCursor::CalendarPaused {
            previous,
            reason,
            skipped,
        } => {
            if row.lifecycle != ScheduleCursorLifecycle::Paused || row.deadline.is_some() {
                return Err(protocol(
                    "paused Karma calendar cursor has an active deadline",
                ));
            }
            let expected =
                resolve_calendar_cursor(schedule, provider, *previous).map_err(boundary)?;
            if expected
                != (CalendarCursorResolution::Paused {
                    previous: *previous,
                    reason: *reason,
                    skipped: skipped.clone(),
                })
            {
                return Err(protocol(
                    "stored Karma calendar pause disagrees with its provider",
                ));
            }
        }
        StoredScheduleCursor::Elapsed { .. } => {
            return Err(protocol("calendar Frequency has an elapsed cursor"));
        }
    }
    Ok(())
}

fn validate_worker_id(worker_id: &str) -> Result<(), StoreError> {
    if worker_id.is_empty()
        || worker_id.len() > MAX_WORKER_ID_BYTES
        || worker_id.trim() != worker_id
        || worker_id.chars().any(char::is_control)
    {
        Err(protocol(
            "Karma schedule worker id must contain 1 to 200 trimmed non-control bytes",
        ))
    } else {
        Ok(())
    }
}

fn canonical_timestamp(value: DateTime<Utc>) -> Result<String, StoreError> {
    Ok(timestamp(value)?.to_string())
}

fn timestamp(value: DateTime<Utc>) -> Result<TimestampMs, StoreError> {
    TimestampMs::from_millis(value.timestamp_millis()).map_err(boundary)
}

fn parse_timestamp(value: String) -> Result<TimestampMs, StoreError> {
    TimestampMs::parse_canonical(&value).map_err(boundary)
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn overload_name(value: OverloadPolicy) -> &'static str {
    match value {
        OverloadPolicy::PauseAndAsk => "pause-and-ask",
        OverloadPolicy::RejectActivation => "reject-activation",
        OverloadPolicy::DegradeWithinGrant => "degrade-within-grant",
    }
}

fn parse_overload(value: &str) -> Result<OverloadPolicy, StoreError> {
    match value {
        "pause-and-ask" => Ok(OverloadPolicy::PauseAndAsk),
        "reject-activation" => Ok(OverloadPolicy::RejectActivation),
        "degrade-within-grant" => Ok(OverloadPolicy::DegradeWithinGrant),
        _ => Err(protocol("stored Karma schedule overload policy is invalid")),
    }
}

fn sqlite_u64(value: u64, label: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("Karma schedule {label} exceeds SQLite")))
}

fn rust_u64(value: i64, label: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol(format!("stored Karma schedule {label} is negative")))
}

fn rust_u32(value: i64, label: &str) -> Result<u32, StoreError> {
    u32::try_from(value)
        .map_err(|_| protocol(format!("stored Karma schedule {label} is outside u32")))
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn json_protocol(error: serde_json::Error) -> StoreError {
    protocol(error.to_string())
}
