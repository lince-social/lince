use chrono::{DateTime, TimeZone, Utc};
use nucleus::karma::{CanonicalHash, KarmaOccurrenceEnvelope, TimestampMs, canonical_json_bytes};
use serde::Serialize;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KarmaOccurrenceRow {
    pub occurrence_hash: CanonicalHash,
    pub cell_sequence: u64,
    pub source_kind: String,
    pub source_identity: CanonicalHash,
    pub logical_at: TimestampMs,
    pub parent_occurrence_hash: Option<CanonicalHash>,
    pub envelope: KarmaOccurrenceEnvelope,
    pub received_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KarmaOccurrenceCommit {
    Inserted(KarmaOccurrenceRow),
    Existing(KarmaOccurrenceRow),
}

pub async fn ingest(
    pool: &SqlitePool,
    envelope: &KarmaOccurrenceEnvelope,
    received_at: DateTime<Utc>,
) -> Result<KarmaOccurrenceCommit, StoreError> {
    let mut tx = pool.begin().await?;
    let commit = ingest_tx(&mut tx, envelope, received_at).await?;
    tx.commit().await?;
    Ok(commit)
}

pub(crate) async fn ingest_tx(
    tx: &mut Transaction<'_, Sqlite>,
    envelope: &KarmaOccurrenceEnvelope,
    received_at: DateTime<Utc>,
) -> Result<KarmaOccurrenceCommit, StoreError> {
    let occurrence_hash = envelope.occurrence_hash().map_err(boundary)?;
    let source_identity = envelope.source_identity().map_err(boundary)?;
    let source_kind = envelope.source.kind_name();
    let envelope_json = canonical_string(envelope)?;
    let received_at = canonical_time(received_at)?.to_rfc3339();
    let inserted_sequence = sqlx::query_scalar::<_, i64>(
        "INSERT INTO karma_occurrence
            (occurrence_hash, cell_sequence, source_kind, source_identity,
             logical_at, parent_occurrence_hash, envelope_json, received_at)
         SELECT ?, sequence.next_sequence, ?, ?, ?, ?, ?, ?
         FROM karma_occurrence_sequence sequence WHERE sequence.singleton = 1
         ON CONFLICT DO NOTHING
         RETURNING cell_sequence",
    )
    .bind(occurrence_hash.as_str())
    .bind(source_kind)
    .bind(source_identity.as_str())
    .bind(envelope.logical_at.to_string())
    .bind(
        envelope
            .parent_occurrence_hash
            .as_ref()
            .map(CanonicalHash::as_str),
    )
    .bind(&envelope_json)
    .bind(&received_at)
    .fetch_optional(&mut **tx)
    .await?;

    let commit = if let Some(inserted_sequence) = inserted_sequence {
        let next_sequence = inserted_sequence
            .checked_add(1)
            .ok_or_else(|| protocol("Karma Cell occurrence sequence overflowed"))?;
        let advanced = sqlx::query(
            "UPDATE karma_occurrence_sequence SET next_sequence = ?
             WHERE singleton = 1 AND next_sequence = ?",
        )
        .bind(next_sequence)
        .bind(inserted_sequence)
        .execute(&mut **tx)
        .await?;
        if advanced.rows_affected() != 1 {
            return Err(protocol(
                "Karma Cell occurrence sequence lost serialization",
            ));
        }
        let row = get_tx(tx, &occurrence_hash)
            .await?
            .expect("inserted Karma occurrence exists");
        KarmaOccurrenceCommit::Inserted(row)
    } else {
        let existing = get_by_source_tx(tx, source_kind, &source_identity).await?;
        let Some(existing) = existing else {
            return Err(protocol(
                "Karma occurrence hash collided outside its source identity",
            ));
        };
        if existing.occurrence_hash != occurrence_hash || existing.envelope != *envelope {
            return Err(protocol(
                "Karma occurrence source identity was reused with different content",
            ));
        }
        KarmaOccurrenceCommit::Existing(existing)
    };
    Ok(commit)
}

pub async fn get(
    pool: &SqlitePool,
    occurrence_hash: &CanonicalHash,
) -> Result<Option<KarmaOccurrenceRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence WHERE occurrence_hash = ?")
        .bind(occurrence_hash.as_str())
        .fetch_optional(pool)
        .await?;
    row.map(map_row).transpose()
}

pub async fn get_by_cell_sequence(
    pool: &SqlitePool,
    cell_sequence: u64,
) -> Result<Option<KarmaOccurrenceRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence WHERE cell_sequence = ?")
        .bind(sql_i64(cell_sequence)?)
        .fetch_optional(pool)
        .await?;
    row.map(map_row).transpose()
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<KarmaOccurrenceRow>, StoreError> {
    sqlx::query("SELECT * FROM karma_occurrence ORDER BY cell_sequence, occurrence_hash")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map_row)
        .collect()
}

async fn get_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    occurrence_hash: &CanonicalHash,
) -> Result<Option<KarmaOccurrenceRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence WHERE occurrence_hash = ?")
        .bind(occurrence_hash.as_str())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_row).transpose()
}

pub(crate) async fn get_by_cell_sequence_tx(
    tx: &mut Transaction<'_, Sqlite>,
    cell_sequence: u64,
) -> Result<Option<KarmaOccurrenceRow>, StoreError> {
    let row = sqlx::query("SELECT * FROM karma_occurrence WHERE cell_sequence = ?")
        .bind(sql_i64(cell_sequence)?)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_row).transpose()
}

async fn get_by_source_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    source_kind: &str,
    source_identity: &CanonicalHash,
) -> Result<Option<KarmaOccurrenceRow>, StoreError> {
    let row =
        sqlx::query("SELECT * FROM karma_occurrence WHERE source_kind = ? AND source_identity = ?")
            .bind(source_kind)
            .bind(source_identity.as_str())
            .fetch_optional(&mut **tx)
            .await?;
    row.map(map_row).transpose()
}

fn map_row(row: sqlx::sqlite::SqliteRow) -> Result<KarmaOccurrenceRow, StoreError> {
    let occurrence_hash = parse_hash(row.get("occurrence_hash"))?;
    let source_kind: String = row.get("source_kind");
    let source_identity = parse_hash(row.get("source_identity"))?;
    let logical_at = TimestampMs::parse_canonical(row.get("logical_at")).map_err(boundary)?;
    let parent_occurrence_hash = row
        .get::<Option<String>, _>("parent_occurrence_hash")
        .map(CanonicalHash::parse)
        .transpose()
        .map_err(boundary)?;
    let envelope_json: String = row.get("envelope_json");
    let envelope: KarmaOccurrenceEnvelope =
        serde_json::from_str(&envelope_json).map_err(json_protocol)?;
    if canonical_string(&envelope)? != envelope_json
        || envelope.occurrence_hash().map_err(boundary)? != occurrence_hash
        || envelope.source.kind_name() != source_kind
        || envelope.source_identity().map_err(boundary)? != source_identity
        || envelope.logical_at != logical_at
        || envelope.parent_occurrence_hash != parent_occurrence_hash
    {
        return Err(protocol(
            "stored Karma occurrence projections or content hash are invalid",
        ));
    }
    Ok(KarmaOccurrenceRow {
        occurrence_hash,
        cell_sequence: u64::try_from(row.get::<i64, _>("cell_sequence"))
            .map_err(|_| protocol("stored Karma occurrence sequence is invalid"))?,
        source_kind,
        source_identity,
        logical_at,
        parent_occurrence_hash,
        envelope,
        received_at: row.get("received_at"),
    })
}

fn canonical_time(value: DateTime<Utc>) -> Result<DateTime<Utc>, StoreError> {
    Utc.timestamp_millis_opt(value.timestamp_millis())
        .single()
        .ok_or_else(|| protocol("Karma occurrence receipt time is outside the supported range"))
}

fn canonical_string<T: Serialize>(value: &T) -> Result<String, StoreError> {
    String::from_utf8(canonical_json_bytes(value).map_err(boundary)?)
        .map_err(|error| protocol(error.to_string()))
}

fn parse_hash(value: String) -> Result<CanonicalHash, StoreError> {
    CanonicalHash::parse(value).map_err(boundary)
}

fn sql_i64(value: u64) -> Result<i64, StoreError> {
    i64::try_from(value)
        .map_err(|_| protocol("Karma occurrence sequence exceeds SQLite integer range"))
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
