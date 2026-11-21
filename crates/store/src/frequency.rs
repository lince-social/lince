//! Simple Frequency declarations and lookups for recurrence and query callers.

use std::{
    fmt::{self, Debug, Formatter},
    iter::once,
};

use chrono::{DateTime, Utc};
use nucleus::{
    execution::now,
    karma::{
        Cadence, CadenceStep, CompiledSchedule, KarmaBoundaryError, NodeOperation::Trigger, Slug,
        TimestampMs, TriggerSource, simple_frequency::frequency_from_cadence,
    },
};
use sqlx::{SqlitePool, query, query_scalar};

use crate::{
    StoreError,
    karma::{
        frequencies::{self, CreateFrequencyInput, FrequencyMutationCommit},
        programs,
    },
    recurrence, write_tx,
};

const MILLIS_PER_DAY: u64 = 86_400_000;

/// The simple cadence view of a stored Frequency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frequency {
    /// Record identity.
    pub uid: String,
    /// Record name used by simple recurrence references.
    pub slug: String,
    /// Purpose from the selected definition revision.
    pub head: String,
    /// Repeating step in the selected cadence.
    pub every: CadenceStep,
    /// Selected cadence and its calendar constraints.
    pub cadence: Cadence,
    /// Anchor instant in RFC 3339 form.
    pub anchor_at: String,
    /// Person who owns the Frequency, when recorded.
    pub actor_uid: Option<String>,
    /// Creation time of the Frequency.
    pub created_at: String,
    /// Last update time of the Frequency.
    pub updated_at: String,
}

impl Frequency {
    /// Returns an owned copy of the cadence.
    pub fn cadence(&self) -> Cadence {
        self.cadence.clone()
    }

    /// Returns the stored anchor in UTC.
    /// # Errors
    /// Returns an error if the anchor is not valid RFC 3339.
    pub fn anchor(&self) -> Result<DateTime<Utc>, StoreError> {
        DateTime::parse_from_rfc3339(&self.anchor_at)
            .map(|at| at.with_timezone(&Utc))
            .map_err(|error| protocol(error.to_string()))
    }
}

/// Inputs for authoring a simple repeating Frequency.
pub struct NewFrequency<'a> {
    /// Name as entered, optionally prefixed with an at sign.
    pub slug: &'a str,
    /// Purpose, with the input name used when this is blank.
    pub head: &'a str,
    /// Repeating time step.
    pub every: CadenceStep,
    /// Instant from which the cadence is derived.
    pub anchor_at: DateTime<Utc>,
    /// Identity used to recognize a retry.
    pub request_id: &'a str,
    /// Optional author and owner.
    pub actor_uid: Option<&'a str>,
}

impl Debug for NewFrequency<'_> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewFrequency")
            .field("every", &self.every)
            .finish_non_exhaustive()
    }
}

/// Creates a simple Frequency or returns its matching replay.
/// # Errors
/// Returns an error for invalid input, a conflicting declaration, or a failed write or reread.
pub async fn create(
    pool: &SqlitePool,
    new: NewFrequency<'_>,
    now: DateTime<Utc>,
) -> Result<Frequency, StoreError> {
    let slug = Slug::new(new.slug.trim().trim_start_matches('@')).map_err(boundary)?;
    let purpose = if new.head.trim().is_empty() {
        new.slug
    } else {
        new.head
    };
    let frequency = frequency_from_cadence(
        slug,
        purpose.to_owned(),
        &Cadence::every(new.every),
        TimestampMs::from_millis(new.anchor_at.timestamp_millis()).map_err(boundary)?,
    )
    .map_err(boundary)?;
    let commit = frequencies::create(
        pool,
        CreateFrequencyInput {
            request_id: new.request_id.to_owned(),
            frequency,
            owner_person_uid: new.actor_uid.map(str::to_owned),
            actor_person_uid: new.actor_uid.map(str::to_owned),
        },
        now,
        |_| None,
    )
    .await?;
    let handle = match commit {
        FrequencyMutationCommit::Committed { handle, .. }
        | FrequencyMutationCommit::Replayed { handle, .. } => handle,
        FrequencyMutationCommit::Stale { .. } => return Err(protocol("frequency changed")),
    };
    get(pool, &handle.record_uid)
        .await?
        .ok_or(StoreError::RowNotFound)
}

/// Reads a live Frequency, preferring its active definition and activation.
/// # Errors
/// Returns an error if the selected schedule or referenced evidence cannot be read or represented.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Frequency>, StoreError> {
    let Some(handle) = frequencies::get_handle(pool, uid).await? else {
        return Ok(None);
    };
    let revision = frequencies::get_revision(
        pool,
        handle
            .active_revision_hash
            .as_ref()
            .unwrap_or(&handle.head_revision_hash),
    )
    .await?
    .ok_or(StoreError::RowNotFound)?;
    let activation = match &handle.active_activation_hash {
        Some(hash) => Some(
            frequencies::get_activation(pool, hash)
                .await?
                .ok_or(StoreError::RowNotFound)?,
        ),
        None => None,
    };
    let compiled = activation
        .as_ref()
        .map_or(&revision.default_compiled, |activation| {
            activation.epoch.compiled()
        });
    let (cadence, anchor_at) = match &compiled.schedule {
        CompiledSchedule::Calendar { schedule } => (
            schedule.cadence.clone(),
            schedule.anchor.as_naive().and_utc().to_rfc3339(),
        ),
        CompiledSchedule::Elapsed { schedule } => {
            let interval = schedule.interval_ms();
            let (Ok(days), Ok(milliseconds)) = (
                u32::try_from(interval / MILLIS_PER_DAY),
                u32::try_from(interval % MILLIS_PER_DAY),
            ) else {
                return Err(protocol("frequency interval exceeds simple form"));
            };
            let anchor = DateTime::from_timestamp_millis(schedule.anchor().as_millis())
                .ok_or_else(|| protocol("invalid frequency anchor"))?;
            (
                Cadence::every(CadenceStep {
                    days,
                    milliseconds,
                    ..Default::default()
                }),
                anchor.to_rfc3339(),
            )
        }
    };
    Ok(Some(Frequency {
        uid: handle.record_uid,
        slug: handle.slug,
        head: revision.frequency.purpose,
        every: cadence.every,
        cadence,
        anchor_at,
        actor_uid: handle.owner_person_uid,
        created_at: handle.created_at,
        updated_at: handle.updated_at,
    }))
}

/// Resolves a live Frequency by trimmed name or Record identity.
/// # Errors
/// Returns an error if resolving the name or reading the selected Frequency fails.
pub async fn resolve(pool: &SqlitePool, name: &str) -> Result<Option<Frequency>, StoreError> {
    let name = name.trim().trim_start_matches('@');
    let uid: Option<String> = query_scalar("SELECT k.record_uid FROM karma_frequency k JOIN record r ON r.uid = k.record_uid WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL")
        .bind(name).bind(name).fetch_optional(pool).await?;
    match uid {
        Some(uid) => get(pool, &uid).await,
        None => Ok(None),
    }
}

/// Lists Frequencies in Record name and identity order.
/// # Errors
/// Returns an error if the listing or any still-live Frequency cannot be read.
pub async fn all(pool: &SqlitePool) -> Result<Vec<Frequency>, StoreError> {
    let handles = frequencies::list_handles(pool).await?;
    let mut result = Vec::with_capacity(handles.len());
    for handle in handles {
        if let Some(frequency) = get(pool, &handle.record_uid).await? {
            result.push(frequency);
        }
    }
    Ok(result)
}

/// Soft-deletes an unreferenced Frequency and clears its active schedule.
/// # Errors
/// Returns an error if the Frequency is missing or in use, or if checking or writing fails.
pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    let frequency = get(pool, uid).await?.ok_or(StoreError::RowNotFound)?;
    let bound: bool = query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_frequency WHERE frequency_uid = ? UNION ALL SELECT 1 FROM karma_signal_frequency WHERE frequency_uid = ?)")
        .bind(uid).bind(uid).fetch_one(pool).await?;
    if bound {
        return Err(protocol("a rule or Signal still uses that frequency"));
    }
    for rule in recurrence::all(pool).await? {
        if let Some(condition) = rule.condition {
            let parsed = condition
                .parsed()
                .map_err(|error| protocol(error.to_string()))?;
            if parsed.reads_ref().iter().any(|token| {
                token.func == "freq" && (token.slug == frequency.slug || token.slug == uid)
            }) {
                return Err(protocol("a rule still reads that frequency"));
            }
        }
    }
    for handle in programs::list_handles(pool).await? {
        let active = handle
            .active_revision_hash
            .as_ref()
            .filter(|&hash| hash != &handle.head_revision_hash);
        for hash in once(&handle.head_revision_hash).chain(active) {
            let Some(revision) = programs::get_revision(pool, hash).await? else {
                continue;
            };
            if revision.program.nodes.values().any(|node| {
                matches!(
                    &node.operation,
                    Trigger { source: TriggerSource::Frequency { frequency }, .. }
                    if frequency.target.as_str() == uid
                )
            }) {
                return Err(protocol("a Program still reads that frequency"));
            }
        }
    }
    let mut tx = write_tx(pool).await?;
    query("UPDATE record SET deleted_at = ? WHERE uid = ?")
        .bind(now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    query("UPDATE karma_frequency SET status = 'paused', active_revision_hash = NULL, active_activation_hash = NULL WHERE record_uid = ?")
        .bind(uid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

fn boundary(error: KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}
