use super::protocol;
use crate::{Store, StoreError};
use nucleus::{
    performance::{Span as Measure, add},
    projection::{Context, MAX_SPANS, Scheduled, Span, Status},
};
use serde_json::from_str;
use sqlx::{Row, SqliteConnection, query, query_as, sqlite::SqliteRow};

/// Cached forecast data for an exact context and execution identity.
#[derive(Debug)]
pub struct Cached {
    /// Whether this result covers the complete calculation.
    pub status: Status,
    /// Calendar spans clipped to the requested window.
    pub spans: Vec<Span>,
    /// Scheduled events overlapping the requested window.
    pub schedule: Vec<Scheduled>,
    /// Exclusive expiry timestamp in milliseconds.
    pub expires_ms: i64,
}

async fn current_window(
    connection: &mut SqliteConnection,
    key: &str,
    context: &Context,
    now_ms: i64,
) -> Result<Option<SqliteRow>, StoreError> {
    query("SELECT base_ms, expires_ms, incomplete FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ? AND from_ms <= MAX(?, base_ms) AND until_ms >= ?")
        .bind(key).bind(now_ms).bind(now_ms).bind(context.window.from_ms).bind(context.window.until_ms)
        .fetch_optional(connection).await
}

/// Returns the expiry of a matching cache window, if present.
///
/// # Errors
///
/// Returns an error if the context or cache metadata cannot be read.
pub async fn cached_until(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<i64>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let key = context.key(identity).map_err(protocol)?;
    let mut connection = store.pool.acquire().await?;
    current_window(&mut connection, key.as_str(), context, now_ms)
        .await?
        .map(|row| row.try_get("expires_ms"))
        .transpose()
}

/// Reads current calendar and schedule forecasts for the context.
///
/// # Errors
///
/// Returns an error if the context or cache rows cannot be decoded.
pub async fn read(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<Cached>, StoreError> {
    read_content(store, context, now_ms, Content::All).await
}

/// Reads the calendar without fetching scheduled event rows.
///
/// # Errors
///
/// Returns an error if the context or cache rows cannot be decoded.
pub async fn read_calendar(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(Status, Vec<Span>)>, StoreError> {
    Ok(read_content(store, context, now_ms, Content::Calendar)
        .await?
        .map(|cached| (cached.status, cached.spans)))
}

/// Reads scheduled events without fetching calendar span rows.
///
/// # Errors
///
/// Returns an error if the context or cache rows cannot be decoded.
pub async fn read_schedule(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(Status, Vec<Scheduled>)>, StoreError> {
    Ok(read_content(store, context, now_ms, Content::Schedule)
        .await?
        .map(|cached| (cached.status, cached.schedule)))
}

#[derive(Clone, Copy)]
enum Content {
    All,
    Calendar,
    Schedule,
}

async fn read_content(
    store: &Store,
    context: &Context,
    now_ms: i64,
    content: Content,
) -> Result<Option<Cached>, StoreError> {
    let _read = Measure::new("projection-cache-read-await");
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let mut tx = pool.begin().await?;
    let row = current_window(&mut tx, key.as_str(), context, now_ms).await?;
    let Some(row) = row else {
        tx.rollback().await?;
        return Ok(None);
    };
    let span_rows = if matches!(content, Content::All | Content::Calendar) {
        add("projection-span-reads", 1);
        query("SELECT payload FROM projection_span WHERE from_ms < ? AND until_ms > ? ORDER BY from_ms, id LIMIT ?")
            .bind(context.window.until_ms).bind(context.window.from_ms).bind(MAX_SPANS as i64).fetch_all(&mut *tx).await?
    } else {
        Vec::new()
    };
    let schedule_rows = if matches!(content, Content::All | Content::Schedule) {
        add("projection-schedule-reads", 1);
        query("SELECT payload FROM projection_schedule WHERE from_ms < ? AND ((until_ms IS NULL AND from_ms >= ?) OR until_ms > ?) ORDER BY from_ms, id LIMIT ?")
            .bind(context.window.until_ms).bind(context.window.from_ms).bind(context.window.from_ms).bind(MAX_SPANS as i64).fetch_all(&mut *tx).await?
    } else {
        Vec::new()
    };
    tx.rollback().await?;
    add("projection-span-rows", span_rows.len() as u64);
    add("projection-schedule-rows", schedule_rows.len() as u64);
    let incomplete: Option<&str> = row.try_get("incomplete")?;
    let status = match incomplete {
        Some(incomplete) => Status::Incomplete {
            base_ms: row.try_get("base_ms")?,
            reason: from_str(incomplete).map_err(protocol)?,
        },
        None => Status::Ready {
            base_ms: row.try_get("base_ms")?,
            expires_ms: row.try_get("expires_ms")?,
        },
    };
    let spans = span_rows
        .iter()
        .map(|row| {
            let mut span: Span = from_str(row.try_get::<&str, _>("payload")?).map_err(protocol)?;
            span.from_ms = span.from_ms.max(context.window.from_ms);
            span.until_ms = span.until_ms.min(context.window.until_ms);
            Ok::<Span, StoreError>(span)
        })
        .collect::<Result<Vec<Span>, _>>()?;
    let schedule = schedule_rows
        .iter()
        .map(|row| from_str(row.try_get::<&str, _>("payload")?).map_err(protocol))
        .collect::<Result<Vec<Scheduled>, StoreError>>()?;
    Ok(Some(Cached {
        status,
        spans,
        schedule,
        expires_ms: row.try_get("expires_ms")?,
    }))
}

/// Returns the stored bounds of a matching, current cache window.
///
/// # Errors
///
/// Returns an error if the context or cache bounds cannot be read.
pub async fn covered_window(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Option<(i64, i64)>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(None);
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    query_as("SELECT from_ms, until_ms FROM projection_window WHERE id = 1 AND cache_key = ? AND source_revision = (SELECT revision FROM projection_source WHERE id = 1) AND expires_ms > ? AND base_ms <= ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms).fetch_optional(pool).await
}

/// Reads the recent schedule for the same execution context, including stale revisions.
///
/// # Errors
///
/// Returns an error if the context or stored events cannot be decoded.
pub async fn previous_schedule(
    store: &Store,
    context: &Context,
    now_ms: i64,
) -> Result<Vec<Scheduled>, StoreError> {
    let Some(identity) = store.execution_identity() else {
        return Ok(Vec::new());
    };
    let pool = &store.pool;
    let key = context.key(identity).map_err(protocol)?;
    let rows = query("SELECT s.payload FROM projection_schedule s JOIN projection_window w ON w.id = 1 WHERE w.cache_key = ? AND w.base_ms <= ? AND w.expires_ms > ? AND s.from_ms >= ? AND s.from_ms < ? ORDER BY s.from_ms, s.id LIMIT ?")
        .bind(key.as_str()).bind(now_ms).bind(now_ms.saturating_sub(60_000)).bind(now_ms.max(context.window.from_ms)).bind(context.window.until_ms).bind(MAX_SPANS as i64).fetch_all(pool).await?;
    rows.iter()
        .map(|row| from_str(row.try_get::<&str, _>("payload")?).map_err(protocol))
        .collect()
}
