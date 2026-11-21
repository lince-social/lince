use super::protocol;
use crate::{Store, StoreError, write_tx};
use nucleus::projection::{Context, Incomplete, MAX_CACHE_BYTES, MAX_SPANS, Scheduled, Span};
use serde::Serialize;
use serde_json::to_string;
use sqlx::{query, query_scalar};

/// Publishes a calendar for the given source revision.
/// Returns `false` if the source changed before the transaction acquired its lock.
///
/// # Errors
///
/// Returns an error for invalid identity, forecast data, or a failed transaction.
pub async fn publish(
    store: &Store,
    context: &Context,
    source: i64,
    base_ms: i64,
    expires_ms: i64,
    incomplete: Option<&Incomplete>,
    spans: &[Span],
) -> Result<bool, StoreError> {
    publish_schedule(
        store,
        context,
        source,
        base_ms,
        expires_ms,
        incomplete,
        spans,
        &[],
    )
    .await
}

/// Atomically publishes a calendar and schedule if the source revision still matches.
///
/// # Errors
///
/// Returns an error for invalid identity, forecast data, or a failed transaction.
#[expect(
    clippy::too_many_arguments,
    reason = "preserves the public Store projection API"
)]
pub async fn publish_schedule(
    store: &Store,
    context: &Context,
    source: i64,
    base_ms: i64,
    expires_ms: i64,
    incomplete: Option<&Incomplete>,
    spans: &[Span],
    schedule: &[Scheduled],
) -> Result<bool, StoreError> {
    if spans.len() > MAX_SPANS || schedule.len() > MAX_SPANS {
        return Err(protocol("projection span budget exceeded"));
    }
    let encoded = encode(spans)?;
    let encoded_schedule = encode(schedule)?;
    if encoded
        .iter()
        .chain(&encoded_schedule)
        .map(String::len)
        .sum::<usize>()
        > MAX_CACHE_BYTES
    {
        return Err(protocol("projection byte budget exceeded"));
    }
    let Some(identity) = store.execution_identity() else {
        return Err(protocol(
            "projection publication requires execution identity",
        ));
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let mut tx = write_tx(pool).await?;
    let current: i64 = query_scalar("SELECT revision FROM projection_source WHERE id = 1")
        .fetch_one(&mut *tx)
        .await?;
    if current != source {
        tx.rollback().await?;
        return Ok(false);
    }
    query("DELETE FROM projection_span")
        .execute(&mut *tx)
        .await?;
    query("DELETE FROM projection_schedule")
        .execute(&mut *tx)
        .await?;
    for (span, payload) in spans.iter().zip(encoded) {
        query("INSERT INTO projection_span VALUES (?, ?, ?, ?, ?)")
            .bind(&span.id)
            .bind(span.record.as_str())
            .bind(span.from_ms)
            .bind(span.until_ms)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
    }
    for (entry, payload) in schedule.iter().zip(encoded_schedule) {
        query("INSERT INTO projection_schedule VALUES (?, ?, ?, ?, ?)")
            .bind(&entry.id)
            .bind(entry.record.as_str())
            .bind(entry.time.from_ms)
            .bind(entry.time.until_ms)
            .bind(payload)
            .execute(&mut *tx)
            .await?;
    }
    query("INSERT INTO projection_window VALUES (1, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET cache_key = excluded.cache_key, source_revision = excluded.source_revision, base_ms = excluded.base_ms, expires_ms = excluded.expires_ms, from_ms = excluded.from_ms, until_ms = excluded.until_ms, incomplete = excluded.incomplete")
        .bind(key.as_str()).bind(source).bind(base_ms).bind(expires_ms).bind(context.window.from_ms).bind(context.window.until_ms).bind(incomplete.map(to_string).transpose().map_err(protocol)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

fn encode<T: Serialize>(entries: &[T]) -> Result<Vec<String>, StoreError> {
    entries
        .iter()
        .map(|entry| to_string(entry).map_err(protocol))
        .collect()
}
