use super::protocol;
use crate::StoreError;
use sqlx::{Sqlite, SqlitePool, Transaction, query, query_as};

const STATE_LIMIT: i64 = 4096;
const STORAGE_LIMIT: i64 = 64 * 1024 * 1024;

/// Reads private device state and its version, if present.
///
/// # Errors
///
/// Returns storage errors or an error if the row cannot be decoded.
pub async fn device_state(
    pool: &SqlitePool,
    id: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    query_as("SELECT body,version FROM social_device_state WHERE id=?")
        .bind(id)
        .fetch_optional(pool)
        .await
}

/// Creates private device state or updates an existing matching version.
///
/// # Errors
///
/// Returns an error if the device budget is exceeded, the version is stale or
/// exhausted, or SQLite cannot write the state.
pub async fn put_device_state_on(
    tx: &mut Transaction<'_, Sqlite>,
    id: &str,
    kind: &str,
    context: &str,
    body: &str,
    previous: Option<i64>,
    now: i64,
) -> Result<(), StoreError> {
    let (count, bytes): (i64, i64) = query_as(
        "SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_device_state WHERE id<>?",
    ).bind(id).fetch_one(&mut **tx).await?;
    let body_bytes = i64::try_from(body.len()).unwrap_or(i64::MAX);
    if count >= STATE_LIMIT || bytes.saturating_add(body_bytes) > STORAGE_LIMIT {
        return Err(protocol("The device's private transport storage is full"));
    }
    if let Some(version) = previous {
        let next = version
            .checked_add(1)
            .ok_or_else(|| protocol("Private transport version exhausted"))?;
        let changed = query("UPDATE social_device_state SET body=?,version=?,updated_at=? WHERE id=? AND version=? AND kind=? AND context=?")
            .bind(body).bind(next).bind(now).bind(id).bind(version).bind(kind).bind(context).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(protocol(
                "Private transport state changed; retry without advancing the saved session",
            ));
        }
    } else {
        query("INSERT INTO social_device_state(id,kind,context,body,updated_at) VALUES(?,?,?,?,?)")
            .bind(id)
            .bind(kind)
            .bind(context)
            .bind(body)
            .bind(now)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}
