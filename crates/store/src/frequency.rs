use chrono::{DateTime, Utc};
use nucleus::karma::{Cadence, CadenceStep, CompiledSchedule, Slug, TimestampMs};
use sqlx::SqlitePool;

use crate::StoreError;
use crate::karma::frequencies::{self, CreateFrequencyInput, FrequencyMutationCommit};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frequency {
    pub uid: String,
    pub slug: String,
    pub head: String,
    pub every: CadenceStep,
    pub cadence: Cadence,
    pub anchor_at: String,
    pub actor_uid: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Frequency {
    pub fn cadence(&self) -> Cadence {
        self.cadence.clone()
    }

    pub fn anchor(&self) -> Result<DateTime<Utc>, StoreError> {
        DateTime::parse_from_rfc3339(&self.anchor_at)
            .map(|at| at.with_timezone(&Utc))
            .map_err(|error| protocol(error.to_string()))
    }
}

pub struct NewFrequency<'a> {
    pub slug: &'a str,
    pub head: &'a str,
    pub every: CadenceStep,
    pub anchor_at: DateTime<Utc>,
    pub request_id: &'a str,
    pub actor_uid: Option<&'a str>,
}

pub async fn create(
    pool: &SqlitePool,
    new: NewFrequency<'_>,
    now: DateTime<Utc>,
) -> Result<Frequency, StoreError> {
    let slug = Slug::new(new.slug.trim().trim_start_matches('@')).map_err(boundary)?;
    let frequency = nucleus::karma::simple_frequency::frequency_from_cadence(
        slug,
        if new.head.trim().is_empty() {
            new.slug.to_string()
        } else {
            new.head.to_string()
        },
        &Cadence::every(new.every),
        TimestampMs::from_millis(new.anchor_at.timestamp_millis()).map_err(boundary)?,
    )
    .map_err(boundary)?;
    let commit = frequencies::create(
        pool,
        CreateFrequencyInput {
            request_id: new.request_id.to_string(),
            frequency,
            owner_person_uid: new.actor_uid.map(str::to_string),
            actor_person_uid: new.actor_uid.map(str::to_string),
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
        .ok_or(sqlx::Error::RowNotFound)
}

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
    .ok_or(sqlx::Error::RowNotFound)?;
    let compiled = match &handle.active_activation_hash {
        Some(hash) => frequencies::get_activation(pool, hash)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?
            .epoch
            .compiled()
            .clone(),
        None => revision.default_compiled,
    };
    let (cadence, anchor_at) = match compiled.schedule {
        CompiledSchedule::Calendar { schedule } => (
            schedule.cadence,
            schedule.anchor.as_naive().and_utc().to_rfc3339(),
        ),
        CompiledSchedule::Elapsed { schedule } => (
            Cadence::every(CadenceStep {
                days: u32::try_from(schedule.interval_ms() / 86_400_000)
                    .map_err(|_| protocol("frequency interval exceeds simple form"))?,
                milliseconds: (schedule.interval_ms() % 86_400_000) as u32,
                ..Default::default()
            }),
            DateTime::from_timestamp_millis(schedule.anchor().as_millis())
                .ok_or_else(|| protocol("invalid frequency anchor"))?
                .to_rfc3339(),
        ),
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

pub async fn resolve(pool: &SqlitePool, name: &str) -> Result<Option<Frequency>, StoreError> {
    let name = name.trim().trim_start_matches('@');
    let uid: Option<String> = sqlx::query_scalar("SELECT k.record_uid FROM karma_frequency k JOIN record r ON r.uid = k.record_uid WHERE (r.slug = ? OR r.uid = ?) AND r.deleted_at IS NULL")
        .bind(name).bind(name).fetch_optional(pool).await?;
    match uid {
        Some(uid) => get(pool, &uid).await,
        None => Ok(None),
    }
}

pub async fn all(pool: &SqlitePool) -> Result<Vec<Frequency>, StoreError> {
    let mut result = Vec::new();
    for handle in frequencies::list_handles(pool).await? {
        if let Some(frequency) = get(pool, &handle.record_uid).await? {
            result.push(frequency);
        }
    }
    Ok(result)
}

pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<(), StoreError> {
    let frequency = get(pool, uid).await?.ok_or(sqlx::Error::RowNotFound)?;
    let bound: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM karma_rule_frequency WHERE frequency_uid = ? UNION ALL SELECT 1 FROM karma_signal_frequency WHERE frequency_uid = ?)")
        .bind(uid).bind(uid).fetch_one(pool).await?;
    if bound {
        return Err(protocol("a rule or Signal still uses that frequency"));
    }
    for rule in crate::recurrence::all(pool).await? {
        if let Some(condition) = rule.condition {
            let parsed = nucleus::karma::Condition::parse(&condition.source)
                .map_err(|e| protocol(e.to_string()))?;
            if parsed.reads().iter().any(|token| {
                token.func == "freq" && (token.slug == frequency.slug || token.slug == uid)
            }) {
                return Err(protocol("a rule still reads that frequency"));
            }
        }
    }
    for handle in crate::karma::programs::list_handles(pool).await? {
        for hash in
            std::iter::once(&handle.head_revision_hash).chain(handle.active_revision_hash.as_ref())
        {
            if let Some(revision) = crate::karma::programs::get_revision(pool, hash).await? {
                for node in revision.program.nodes.values() {
                    if let nucleus::karma::NodeOperation::Trigger {
                        source: nucleus::karma::TriggerSource::Frequency { frequency },
                        ..
                    } = &node.operation
                        && frequency.target.as_str() == uid
                    {
                        return Err(protocol("a Program still reads that frequency"));
                    }
                }
            }
        }
    }
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("UPDATE record SET deleted_at = ? WHERE uid = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(uid)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE karma_frequency SET status = 'paused', active_revision_hash = NULL, active_activation_hash = NULL WHERE record_uid = ?")
        .bind(uid).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}
fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}
