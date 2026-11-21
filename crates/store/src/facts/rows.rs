use std::{collections::HashMap, fmt};

use chrono::{DateTime, Utc};
use nucleus::{Cause, CauseKind, DecimalValue, Fact, execution::now};
use serde_json::{from_str, to_string};
use sqlx::{
    Row, Sqlite, SqliteConnection, SqlitePool, Transaction, query, query_as, query_scalar,
    sqlite::SqliteRow,
};

use super::instant;
use crate::{
    StoreError,
    exact::{decimal_columns, parse_decimal},
    sync_ops::{OpKind, log_local_tx},
};

const POSITION_PAGE: &str =
    "SELECT rowid AS position, * FROM fact WHERE rowid > ? ORDER BY rowid LIMIT ?";

/// Checks for a fact in the caller's transaction.
///
/// # Errors
/// Returns an error if the query fails.
pub async fn exists(tx: &mut Transaction<'_, Sqlite>, uid: &str) -> Result<bool, StoreError> {
    Ok(query("SELECT 1 FROM fact WHERE uid = ?")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
        .is_some())
}

/// Returns the latest chain hash, or `genesis` for an empty chain.
///
/// # Errors
/// Returns an error if the chain head cannot be read.
pub async fn last_hash(tx: &mut Transaction<'_, Sqlite>) -> Result<String, StoreError> {
    Ok(
        query_scalar("SELECT hash FROM fact ORDER BY rowid DESC LIMIT 1")
            .fetch_optional(&mut **tx)
            .await?
            .unwrap_or_else(|| "genesis".into()),
    )
}

/// Inserts a sealed fact and logs locally produced changes in the same transaction.
///
/// # Errors
/// Returns an error if the fact or its local change cannot be written.
pub async fn insert(tx: &mut Transaction<'_, Sqlite>, fact: &Fact) -> Result<(), StoreError> {
    let (mantissa, scale) = decimal_columns(fact.delta);
    query(
        "INSERT INTO fact (uid, record_uid, delta_mantissa, delta_scale, at, actor_uid,
                           cause_kind, cause_uid, payload, prev_hash, hash, signature, commit_sequence)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, (SELECT value FROM commit_sequence WHERE id = 1))",
    )
    .bind(&fact.uid)
    .bind(&fact.record_uid)
    .bind(mantissa)
    .bind(scale)
    .bind(instant(fact.at))
    .bind(&fact.actor_uid)
    .bind(fact.cause.kind.as_str())
    .bind(&fact.cause.uid)
    .bind(&fact.payload)
    .bind(&fact.prev_hash)
    .bind(&fact.hash)
    .bind(&fact.signature)
    .execute(&mut **tx)
    .await?;
    if fact.cause.kind != CauseKind::Sync {
        log_local_tx(tx, "fact", &fact.uid, "", OpKind::Fact, None).await?;
    }
    Ok(())
}

/// The value, time, and attribution needed to display a fact.
pub struct Summary {
    /// Exact signed change in quantity.
    pub delta: DecimalValue,
    /// Fact timestamp.
    pub at: DateTime<Utc>,
    /// Actor identity, when recorded.
    pub actor_uid: Option<String>,
    /// Type and optional identity of the event that produced the fact.
    pub cause: Cause,
}

impl fmt::Debug for Summary {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Summary")
            .field("delta", &self.delta)
            .field("at", &self.at)
            .field("cause_kind", &self.cause.kind)
            .finish_non_exhaustive()
    }
}

pub(super) fn read_delta(row: &SqliteRow) -> Result<DecimalValue, StoreError> {
    parse_decimal(row.try_get("delta_mantissa")?, row.try_get("delta_scale")?)
}

pub(super) fn summary(row: &SqliteRow) -> Result<Summary, StoreError> {
    let at: &str = row.try_get("at")?;
    let cause_kind: &str = row.try_get("cause_kind")?;
    Ok(Summary {
        delta: read_delta(row)?,
        at: DateTime::parse_from_rfc3339(at).map_or_else(|_| now(), |at| at.with_timezone(&Utc)),
        actor_uid: row.try_get("actor_uid")?,
        cause: Cause {
            kind: CauseKind::parse(cause_kind).unwrap_or(CauseKind::UserEdit),
            uid: row.try_get("cause_uid")?,
        },
    })
}

pub(super) fn map_fact(row: SqliteRow) -> Result<Fact, StoreError> {
    let Summary {
        delta,
        at,
        actor_uid,
        cause,
    } = summary(&row)?;
    Ok(Fact {
        uid: row.try_get("uid")?,
        record_uid: row.try_get("record_uid")?,
        delta,
        at,
        actor_uid,
        cause,
        payload: row.try_get("payload")?,
        prev_hash: row.try_get("prev_hash")?,
        hash: row.try_get("hash")?,
        signature: row.try_get("signature")?,
    })
}

pub(crate) fn map_fact_with_origin(row: SqliteRow) -> Result<Fact, StoreError> {
    if let Some(payload) = row.try_get::<Option<&str>, _>("origin_payload")? {
        from_str(payload).map_err(json_error)
    } else {
        map_fact(row)
    }
}

fn json_error(error: serde_json::Error) -> StoreError {
    StoreError::Protocol(error.to_string())
}

/// Pages local facts by increasing database position.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn after_position(
    pool: &SqlitePool,
    after: i64,
    limit: i64,
) -> Result<Vec<(i64, Fact)>, StoreError> {
    query(POSITION_PAGE)
        .bind(after)
        .bind(limit)
        .try_map(|row| Ok((row.try_get("position")?, map_fact(row)?)))
        .fetch_all(pool)
        .await
}

/// Pages local facts with the commit sequence, when known, for each fact.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn after_position_with_commits(
    pool: &SqlitePool,
    after: i64,
    limit: i64,
) -> Result<Vec<(i64, Option<i64>, Fact)>, StoreError> {
    query(POSITION_PAGE)
        .bind(after)
        .bind(limit)
        .try_map(|row| {
            Ok((
                row.try_get("position")?,
                row.try_get("commit_sequence")?,
                map_fact(row)?,
            ))
        })
        .fetch_all(pool)
        .await
}

/// Reads a fact, preferring the retained original over the local copy.
///
/// # Errors
/// Returns an error if either representation cannot be queried or decoded.
pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<Fact>, StoreError> {
    get_on(&mut *pool.acquire().await?, uid).await
}

/// Reads the original or local fact on the caller's connection.
///
/// # Errors
/// Returns an error if either representation cannot be queried or decoded.
pub async fn get_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<Option<Fact>, StoreError> {
    if let Some(payload) =
        query_scalar::<_, String>("SELECT payload FROM fact_origin WHERE fact_uid = ?")
            .bind(uid)
            .fetch_optional(&mut *connection)
            .await?
    {
        return from_str(&payload).map(Some).map_err(json_error);
    }
    local_fact_on(connection, uid).await
}

/// Attaches the original sealed evidence and its source to a local fact identity.
///
/// # Errors
/// Returns an error for a conflicting original or failed encoding or storage.
pub async fn retain_origin(
    tx: &mut Transaction<'_, Sqlite>,
    fact: &Fact,
    organ: &str,
    cell: &str,
) -> Result<(), StoreError> {
    let payload = to_string(fact).map_err(json_error)?;
    let saved: Option<(String, String, String)> =
        query_as("SELECT payload, organ_uid, cell_uid FROM fact_origin WHERE fact_uid = ?")
            .bind(&fact.uid)
            .fetch_optional(&mut **tx)
            .await?;
    if saved.is_some_and(|(saved_payload, saved_organ, saved_cell)| {
        saved_payload != payload || saved_organ != organ || saved_cell != cell
    }) {
        return Err(StoreError::Protocol(
            "Fact identity was reused with different original evidence".into(),
        ));
    }
    query("INSERT OR IGNORE INTO fact_origin (fact_uid, payload, organ_uid, cell_uid) VALUES (?, ?, ?, ?)")
        .bind(&fact.uid)
        .bind(payload)
        .bind(organ)
        .bind(cell)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Reads the local copy in the caller's transaction, without using retained origin data.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn get_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
) -> Result<Option<Fact>, StoreError> {
    local_fact_on(tx, uid).await
}

async fn local_fact_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<Option<Fact>, StoreError> {
    query("SELECT * FROM fact WHERE uid = ?")
        .bind(uid)
        .try_map(map_fact)
        .fetch_optional(connection)
        .await
}

/// Reads the actor on the record's earliest local fact, when present.
///
/// # Errors
/// Returns an error if the actor query fails.
pub async fn creator_uid(
    pool: &SqlitePool,
    record_uid: &str,
) -> Result<Option<String>, StoreError> {
    query_scalar("SELECT actor_uid FROM fact WHERE record_uid = ? ORDER BY rowid ASC LIMIT 1")
        .bind(record_uid)
        .fetch_optional(pool)
        .await
        .map(Option::flatten)
}

/// Lists a record's latest local facts in reverse database order.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn for_record(
    pool: &SqlitePool,
    record_uid: &str,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    query("SELECT * FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT ?")
        .bind(record_uid)
        .bind(limit)
        .try_map(map_fact)
        .fetch_all(pool)
        .await
}

/// Groups latest fact summaries by record, applying the same limit to each record.
///
/// # Errors
/// Returns an error if encoding, querying, or summary decoding fails.
pub async fn summaries_for_records(
    pool: &SqlitePool,
    records: &[&str],
    limit: i64,
) -> Result<HashMap<String, Vec<Summary>>, StoreError> {
    let mut output = HashMap::<String, Vec<Summary>>::new();
    if records.is_empty() || limit == 0 {
        return Ok(output);
    }
    let selected = to_string(records).map_err(json_error)?;
    let rows = query(
        "SELECT f.record_uid, f.delta_mantissa, f.delta_scale, f.at,
                f.actor_uid, f.cause_kind, f.cause_uid
         FROM (SELECT DISTINCT value FROM json_each(?1)) selected
         JOIN fact f ON f.rowid IN (
             SELECT rowid FROM fact WHERE record_uid = selected.value
             ORDER BY rowid DESC LIMIT ?2
         ) ORDER BY f.record_uid, f.rowid DESC",
    )
    .bind(selected)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    for row in rows {
        let fact = summary(&row)?;
        output
            .entry(row.try_get("record_uid")?)
            .or_default()
            .push(fact);
    }
    Ok(output)
}
