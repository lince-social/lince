use std::fmt;

use chrono::{DateTime, Utc};
use nucleus::{
    Cause, DecimalValue, Fact,
    performance::{Span, add},
};
use sqlx::{QueryBuilder, Row, SqlitePool, query, query_scalar};

use super::{
    instant,
    rows::{Summary, map_fact, summary},
};
use crate::StoreError;

fn instant_str(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map_or_else(|_| value.to_owned(), |at| instant(at.with_timezone(&Utc)))
}

/// Lists local facts in database order, optionally starting at the supplied timestamp.
///
/// # Errors
/// Returns an error if the query or fact decoding fails.
pub async fn list_since(
    pool: &SqlitePool,
    since_rfc3339: Option<&str>,
    limit: i64,
) -> Result<Vec<Fact>, StoreError> {
    let query = match since_rfc3339 {
        Some(since) => query("SELECT * FROM fact WHERE at >= ? ORDER BY rowid LIMIT ?")
            .bind(instant_str(since)),
        None => query("SELECT * FROM fact ORDER BY rowid LIMIT ?"),
    };
    query.bind(limit).try_map(map_fact).fetch_all(pool).await
}

/// A local fact projected for ledger display or aggregation.
pub struct LedgerRow {
    /// Fact identity.
    pub uid: String,
    /// Record affected by the fact.
    pub record_uid: String,
    /// Exact signed quantity change.
    pub delta: DecimalValue,
    /// Fact timestamp.
    pub at: DateTime<Utc>,
    /// Actor identity, omitted in the aggregate projection.
    pub actor_uid: Option<String>,
    /// Event type, with identity included only in the detailed projection.
    pub cause: Cause,
    /// Stored payload, omitted in the aggregate projection.
    pub payload: Option<String>,
}

impl fmt::Debug for LedgerRow {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LedgerRow")
            .field("delta", &self.delta)
            .field("at", &self.at)
            .field("cause_kind", &self.cause.kind)
            .finish_non_exhaustive()
    }
}

/// Reads ledger rows, selecting actor, cause identity, and payload only when requested.
///
/// # Errors
/// Returns an error if the query or ledger decoding fails.
pub async fn ledger_since(
    pool: &SqlitePool,
    since_rfc3339: Option<&str>,
    limit: i64,
    include_details: bool,
) -> Result<Vec<LedgerRow>, StoreError> {
    let _read = Span::new("fact-ledger-read-await");
    let mut query =
        QueryBuilder::new("SELECT uid, record_uid, delta_mantissa, delta_scale, at, cause_kind, ");
    let (projection, counter) = if include_details {
        (
            "actor_uid, cause_uid, payload FROM fact",
            "fact-ledger-detail-rows",
        )
    } else {
        (
            "NULL AS actor_uid, NULL AS cause_uid, NULL AS payload FROM fact",
            "fact-ledger-aggregate-rows",
        )
    };
    query.push(projection);
    if let Some(since) = since_rfc3339 {
        query.push(" WHERE at >= ").push_bind(instant_str(since));
    }
    query.push(" ORDER BY rowid LIMIT ").push_bind(limit);
    let rows = query.build().fetch_all(pool).await?;
    add(counter, u64::try_from(rows.len()).unwrap_or(u64::MAX));
    rows.into_iter()
        .map(|row| {
            let Summary {
                delta,
                at,
                actor_uid,
                cause,
            } = summary(&row)?;
            Ok(LedgerRow {
                uid: row.try_get("uid")?,
                record_uid: row.try_get("record_uid")?,
                delta,
                at,
                actor_uid,
                cause,
                payload: row.try_get("payload")?,
            })
        })
        .collect()
}

/// Reports hours since a record's last fact, using whole elapsed seconds.
///
/// # Errors
/// Returns an error if the timestamp query fails.
pub async fn hours_since_last(
    pool: &SqlitePool,
    record_uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<f64>, StoreError> {
    let last: Option<String> =
        query_scalar("SELECT at FROM fact WHERE record_uid = ? ORDER BY rowid DESC LIMIT 1")
            .bind(record_uid)
            .fetch_optional(pool)
            .await?;
    #[expect(
        clippy::cast_precision_loss,
        reason = "The supported calendar range in whole seconds fits exactly in f64"
    )]
    Ok(last.and_then(|at| {
        DateTime::parse_from_rfc3339(&at)
            .ok()
            .map(|at| (now - at.with_timezone(&Utc)).num_seconds() as f64 / 3600.0)
    }))
}
