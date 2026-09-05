use std::num::NonZeroU32;

use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{
    CalendarCoalescedBatch, CalendarEmission, CanonicalHash, KarmaOccurrenceEnvelope,
    MAX_OCCURRENCE_BATCH_PAGE_TICKS, OccurrenceBatchEmission, SemanticCalendarTick,
};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use super::occurrences::{self, KarmaOccurrenceCommit, KarmaOccurrenceRow};
use super::schedules::{self, ScheduleOccurrencePayload, StoredScheduleOccurrence};
use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleCadenceKind {
    Elapsed,
    Calendar,
}

impl ScheduleCadenceKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Elapsed => "elapsed",
            Self::Calendar => "calendar",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "elapsed" => Ok(Self::Elapsed),
            "calendar" => Ok(Self::Calendar),
            _ => Err(protocol("stored schedule expansion cadence is invalid")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleEmissionKind {
    Individual,
    Coalesced,
}

impl ScheduleEmissionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Individual => "individual",
            Self::Coalesced => "coalesced",
        }
    }

    fn parse(value: &str) -> Result<Self, StoreError> {
        match value {
            "individual" => Ok(Self::Individual),
            "coalesced" => Ok(Self::Coalesced),
            _ => Err(protocol("stored schedule expansion emission is invalid")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOccurrenceExpansionCursor {
    pub schedule_occurrence_hash: CanonicalHash,
    pub cadence: ScheduleCadenceKind,
    pub emission: ScheduleEmissionKind,
    pub next_ordinal: u64,
    pub total_items: u64,
    pub completed: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleOccurrenceExpansionPage {
    pub cursor: ScheduleOccurrenceExpansionCursor,
    pub occurrences: Vec<KarmaOccurrenceRow>,
}

#[derive(Debug, Clone, Copy)]
struct ExpansionSpec {
    cadence: ScheduleCadenceKind,
    emission: ScheduleEmissionKind,
    total_items: u64,
}

pub async fn get_cursor(
    pool: &SqlitePool,
    schedule_occurrence_hash: &CanonicalHash,
) -> Result<Option<ScheduleOccurrenceExpansionCursor>, StoreError> {
    let row = sqlx::query(
        "SELECT * FROM karma_schedule_occurrence_expansion
         WHERE schedule_occurrence_hash = ?",
    )
    .bind(schedule_occurrence_hash.as_str())
    .fetch_optional(pool)
    .await?;
    row.map(map_cursor).transpose()
}

pub async fn list_cursors(
    pool: &SqlitePool,
) -> Result<Vec<ScheduleOccurrenceExpansionCursor>, StoreError> {
    sqlx::query(
        "SELECT * FROM karma_schedule_occurrence_expansion
         ORDER BY created_at, schedule_occurrence_hash",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_cursor)
    .collect()
}

pub async fn has_pending_schedule_occurrences(pool: &SqlitePool) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1
            FROM karma_schedule_occurrence occurrence
            LEFT JOIN karma_schedule_occurrence_expansion expansion
              ON expansion.schedule_occurrence_hash = occurrence.occurrence_hash
            WHERE expansion.completed IS NULL OR expansion.completed = 0
         )",
    )
    .fetch_one(pool)
    .await
}

pub async fn expand_schedule_occurrence(
    pool: &SqlitePool,
    schedule_occurrence_hash: &CanonicalHash,
    page_limit: NonZeroU32,
    received_at: DateTime<Utc>,
) -> Result<ScheduleOccurrenceExpansionPage, StoreError> {
    validate_page_limit(page_limit)?;
    let source = schedules::get_occurrence(pool, schedule_occurrence_hash)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let spec = expansion_spec(&source)?;
    let at = canonical_time(received_at)?.to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO karma_schedule_occurrence_expansion
            (schedule_occurrence_hash, cadence_kind, emission_kind,
             next_ordinal, total_items, completed, created_at, updated_at)
         VALUES (?, ?, ?, 0, ?, 0, ?, ?)",
    )
    .bind(schedule_occurrence_hash.as_str())
    .bind(spec.cadence.as_str())
    .bind(spec.emission.as_str())
    .bind(sql_i64(spec.total_items, "schedule expansion item count")?)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let cursor = get_cursor_tx(&mut tx, schedule_occurrence_hash)
        .await?
        .expect("schedule expansion cursor exists after materialization");
    validate_cursor(&cursor, spec)?;
    if cursor.completed {
        tx.commit().await?;
        return Ok(ScheduleOccurrenceExpansionPage {
            cursor,
            occurrences: Vec::new(),
        });
    }

    let envelopes = expansion_page(&source, cursor.next_ordinal, page_limit)?;
    if envelopes.is_empty() {
        return Err(protocol(
            "incomplete schedule expansion produced no semantic occurrences",
        ));
    }
    let produced = u64::try_from(envelopes.len())
        .map_err(|_| protocol("schedule expansion page length overflowed"))?;
    let next_ordinal = cursor
        .next_ordinal
        .checked_add(produced)
        .ok_or_else(|| protocol("schedule expansion ordinal overflowed"))?;
    if next_ordinal > cursor.total_items {
        return Err(protocol(
            "schedule expansion page crossed its persisted item count",
        ));
    }

    let mut rows = Vec::with_capacity(envelopes.len());
    for envelope in &envelopes {
        let commit = occurrences::ingest_tx(&mut tx, envelope, received_at).await?;
        rows.push(match commit {
            KarmaOccurrenceCommit::Inserted(row) | KarmaOccurrenceCommit::Existing(row) => row,
        });
    }
    let completed = next_ordinal == cursor.total_items;
    let updated = sqlx::query(
        "UPDATE karma_schedule_occurrence_expansion
         SET next_ordinal = ?, completed = ?, updated_at = ?
         WHERE schedule_occurrence_hash = ? AND next_ordinal = ? AND completed = 0",
    )
    .bind(sql_i64(next_ordinal, "schedule expansion ordinal")?)
    .bind(completed)
    .bind(&at)
    .bind(schedule_occurrence_hash.as_str())
    .bind(sql_i64(cursor.next_ordinal, "schedule expansion ordinal")?)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() != 1 {
        return Err(protocol(
            "schedule occurrence expansion lost cursor serialization",
        ));
    }
    let cursor = get_cursor_tx(&mut tx, schedule_occurrence_hash)
        .await?
        .expect("advanced schedule expansion cursor exists");
    tx.commit().await?;
    Ok(ScheduleOccurrenceExpansionPage {
        cursor,
        occurrences: rows,
    })
}

pub async fn expand_pending_schedule_occurrences(
    pool: &SqlitePool,
    source_limit: NonZeroU32,
    page_limit: NonZeroU32,
    received_at: DateTime<Utc>,
) -> Result<Vec<ScheduleOccurrenceExpansionPage>, StoreError> {
    validate_page_limit(page_limit)?;
    let hashes = sqlx::query_scalar::<_, String>(
        "SELECT occurrence.occurrence_hash
         FROM karma_schedule_occurrence occurrence
         LEFT JOIN karma_schedule_occurrence_expansion expansion
           ON expansion.schedule_occurrence_hash = occurrence.occurrence_hash
         WHERE expansion.completed IS NULL OR expansion.completed = 0
         ORDER BY occurrence.activation_hash, occurrence.sequence, occurrence.occurrence_hash
         LIMIT ?",
    )
    .bind(i64::from(source_limit.get()))
    .fetch_all(pool)
    .await?;
    let mut pages = Vec::with_capacity(hashes.len());
    for hash in hashes {
        pages.push(
            expand_schedule_occurrence(
                pool,
                &CanonicalHash::parse(hash).map_err(boundary)?,
                page_limit,
                received_at,
            )
            .await?,
        );
    }
    Ok(pages)
}

fn expansion_spec(source: &StoredScheduleOccurrence) -> Result<ExpansionSpec, StoreError> {
    match &source.occurrence {
        ScheduleOccurrencePayload::Elapsed { occurrence } => Ok(ExpansionSpec {
            cadence: ScheduleCadenceKind::Elapsed,
            emission: match occurrence.batch.emission {
                OccurrenceBatchEmission::Individual => ScheduleEmissionKind::Individual,
                OccurrenceBatchEmission::Coalesced => ScheduleEmissionKind::Coalesced,
            },
            total_items: occurrence.batch.semantic_occurrence_count(),
        }),
        ScheduleOccurrencePayload::Calendar { occurrence } => {
            let emission =
                occurrence.catch_up.emission.as_ref().ok_or_else(|| {
                    protocol("calendar schedule occurrence has no semantic emission")
                })?;
            let (emission, total_items) = match emission {
                CalendarEmission::Individual(boundaries) => (
                    ScheduleEmissionKind::Individual,
                    u64::try_from(boundaries.len())
                        .map_err(|_| protocol("calendar expansion item count overflowed"))?,
                ),
                CalendarEmission::Coalesced(_) => (ScheduleEmissionKind::Coalesced, 1),
            };
            Ok(ExpansionSpec {
                cadence: ScheduleCadenceKind::Calendar,
                emission,
                total_items,
            })
        }
    }
}

fn expansion_page(
    source: &StoredScheduleOccurrence,
    start_ordinal: u64,
    page_limit: NonZeroU32,
) -> Result<Vec<KarmaOccurrenceEnvelope>, StoreError> {
    match &source.occurrence {
        ScheduleOccurrencePayload::Elapsed { occurrence }
            if occurrence.batch.emission == OccurrenceBatchEmission::Individual =>
        {
            occurrence
                .batch
                .individual_page(start_ordinal, page_limit)
                .map_err(boundary)?
                .into_iter()
                .map(|tick| {
                    KarmaOccurrenceEnvelope::schedule_tick(
                        source.occurrence_hash.clone(),
                        tick,
                        None,
                    )
                    .map_err(boundary)
                })
                .collect()
        }
        ScheduleOccurrencePayload::Elapsed { occurrence } => {
            if start_ordinal != 0 {
                return Ok(Vec::new());
            }
            Ok(vec![
                KarmaOccurrenceEnvelope::schedule_coalesced(
                    source.occurrence_hash.clone(),
                    occurrence.batch.clone(),
                    None,
                )
                .map_err(boundary)?,
            ])
        }
        ScheduleOccurrencePayload::Calendar { occurrence } => {
            let emission =
                occurrence.catch_up.emission.as_ref().ok_or_else(|| {
                    protocol("calendar schedule occurrence has no semantic emission")
                })?;
            match emission {
                CalendarEmission::Individual(boundaries) => {
                    let start = usize::try_from(start_ordinal)
                        .map_err(|_| protocol("calendar expansion ordinal overflowed"))?;
                    if start >= boundaries.len() {
                        return Ok(Vec::new());
                    }
                    let end = start
                        .saturating_add(page_limit.get() as usize)
                        .min(boundaries.len());
                    boundaries[start..end]
                        .iter()
                        .cloned()
                        .map(|boundary_value| {
                            KarmaOccurrenceEnvelope::calendar_tick(
                                source.occurrence_hash.clone(),
                                SemanticCalendarTick::new(
                                    occurrence.activation_hash.clone(),
                                    boundary_value,
                                ),
                                None,
                            )
                            .map_err(boundary)
                        })
                        .collect()
                }
                CalendarEmission::Coalesced(boundaries) => {
                    if start_ordinal != 0 {
                        return Ok(Vec::new());
                    }
                    let batch = CalendarCoalescedBatch::new(
                        occurrence.activation_hash.clone(),
                        occurrence.sequence,
                        boundaries.clone(),
                    )
                    .map_err(boundary)?;
                    Ok(vec![
                        KarmaOccurrenceEnvelope::calendar_coalesced(
                            source.occurrence_hash.clone(),
                            batch,
                            None,
                        )
                        .map_err(boundary)?,
                    ])
                }
            }
        }
    }
}

async fn get_cursor_tx(
    tx: &mut Transaction<'_, Sqlite>,
    schedule_occurrence_hash: &CanonicalHash,
) -> Result<Option<ScheduleOccurrenceExpansionCursor>, StoreError> {
    let row = sqlx::query(
        "SELECT * FROM karma_schedule_occurrence_expansion
         WHERE schedule_occurrence_hash = ?",
    )
    .bind(schedule_occurrence_hash.as_str())
    .fetch_optional(&mut **tx)
    .await?;
    row.map(map_cursor).transpose()
}

fn map_cursor(
    row: sqlx::sqlite::SqliteRow,
) -> Result<ScheduleOccurrenceExpansionCursor, StoreError> {
    let next_ordinal = rust_u64(row.get("next_ordinal"), "schedule expansion ordinal")?;
    let total_items = rust_u64(row.get("total_items"), "schedule expansion item count")?;
    let completed: bool = row.get("completed");
    if total_items == 0 || next_ordinal > total_items || completed != (next_ordinal == total_items)
    {
        return Err(protocol("stored schedule expansion cursor is invalid"));
    }
    Ok(ScheduleOccurrenceExpansionCursor {
        schedule_occurrence_hash: CanonicalHash::parse(
            row.get::<String, _>("schedule_occurrence_hash"),
        )
        .map_err(boundary)?,
        cadence: ScheduleCadenceKind::parse(&row.get::<String, _>("cadence_kind"))?,
        emission: ScheduleEmissionKind::parse(&row.get::<String, _>("emission_kind"))?,
        next_ordinal,
        total_items,
        completed,
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn validate_cursor(
    cursor: &ScheduleOccurrenceExpansionCursor,
    spec: ExpansionSpec,
) -> Result<(), StoreError> {
    if cursor.cadence != spec.cadence
        || cursor.emission != spec.emission
        || cursor.total_items != spec.total_items
    {
        return Err(protocol(
            "stored schedule expansion cursor disagrees with its immutable source batch",
        ));
    }
    Ok(())
}

fn validate_page_limit(limit: NonZeroU32) -> Result<(), StoreError> {
    if limit.get() > MAX_OCCURRENCE_BATCH_PAGE_TICKS {
        return Err(protocol(format!(
            "schedule expansion page limit exceeds {MAX_OCCURRENCE_BATCH_PAGE_TICKS}"
        )));
    }
    Ok(())
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("schedule expansion time is outside the supported range"))
}

fn sql_i64(value: u64, name: &str) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| protocol(format!("{name} exceeds SQLite integer range")))
}

fn rust_u64(value: i64, name: &str) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| protocol(format!("stored {name} is invalid")))
}

fn boundary(error: nucleus::karma::KarmaBoundaryError) -> StoreError {
    protocol(error.to_string())
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}
