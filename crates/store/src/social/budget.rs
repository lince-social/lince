use super::protocol;
use crate::{StoreError, write_tx};
use BudgetClass::Ordinary;
use sqlx::{Sqlite, SqlitePool, Transaction, query, query_as, query_scalar};

const WINDOW_SECONDS: i64 = 60;
const MAX_SCOPES: i64 = 8192;
const TOTAL_WORK_LIMIT: u64 = 2048;
const SOURCE_WORK_LIMIT: u64 = 256;

/// Accounts for an ordinary request in its own write transaction.
///
/// # Errors
///
/// Returns an error for an invalid scope, exhausted budget, or failed write.
pub async fn spend(
    pool: &SqlitePool,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
) -> Result<(), StoreError> {
    spend_class(pool, source, direction, bytes, limit, now, Ordinary).await
}

/// Accounts for an ordinary request in the caller's write transaction.
///
/// # Errors
///
/// Returns an error for an invalid scope, exhausted budget, or failed write.
pub async fn spend_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
) -> Result<(), StoreError> {
    spend_class_on(tx, source, direction, bytes, limit, now, Ordinary).await
}

/// Determines which share of a service budget the request can use.
#[derive(Debug, Clone, Copy)]
pub enum BudgetClass {
    /// Uses the ordinary share and reserves the rest for control requests.
    Ordinary,
    /// Can use the full budget.
    Control,
}

impl BudgetClass {
    fn ceiling(self, limit: u64) -> u64 {
        match self {
            Self::Ordinary => limit - limit.div_ceil(4),
            Self::Control => limit,
        }
    }
}

/// Accounts for a request with an explicit budget class in its own write transaction.
///
/// # Errors
///
/// Returns an error for an invalid scope, exhausted budget, or failed write.
pub async fn spend_class(
    pool: &SqlitePool,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
    class: BudgetClass,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    spend_class_on(&mut tx, source, direction, bytes, limit, now, class).await?;
    tx.commit().await
}

/// Accounts for a request with an explicit budget class in the caller's transaction.
///
/// # Errors
///
/// Returns an error for an invalid scope, exhausted budget, or failed write.
pub async fn spend_class_on(
    tx: &mut Transaction<'_, Sqlite>,
    source: &str,
    direction: &str,
    bytes: usize,
    limit: u64,
    now: i64,
    class: BudgetClass,
) -> Result<(), StoreError> {
    if source.len() > 128 || !matches!(direction, "in" | "out") {
        return Err(protocol("Invalid service accounting scope"));
    }
    let bytes = u64::try_from(bytes).map_err(|_| limited())?;
    let window = now.div_euclid(WINDOW_SECONDS);
    query("DELETE FROM social_service_budget WHERE window<?")
        .bind(window - 1)
        .execute(&mut **tx)
        .await?;
    let count: i64 = query_scalar("SELECT COUNT(*) FROM social_service_budget")
        .fetch_one(&mut **tx)
        .await?;
    let mut updates = [("", 0, 0); 2];
    let mut additions = 0;
    for (update, (key, ceiling, work_limit)) in updates.iter_mut().zip([
        ("*", limit, TOTAL_WORK_LIMIT),
        (source, limit / 4, SOURCE_WORK_LIMIT),
    ]) {
        let row: Option<(i64, i64, i64)> = query_as(
            "SELECT window,bytes,work FROM social_service_budget WHERE source=? AND direction=?",
        )
        .bind(key)
        .bind(direction)
        .fetch_optional(&mut **tx)
        .await?;
        additions += i64::from(row.is_none());
        let (_, old_bytes, old_work) = row
            .filter(|(held, _, _)| *held == window)
            .unwrap_or_default();
        let old_bytes = u64::try_from(old_bytes).map_err(|_| limited())?;
        let next = old_bytes.checked_add(bytes).ok_or_else(limited)?;
        let work = u64::try_from(old_work).map_err(|_| limited())?;
        if next > class.ceiling(ceiling) || work >= class.ceiling(work_limit) {
            return Err(limited());
        }
        *update = (
            key,
            i64::try_from(next).map_err(|_| limited())?,
            old_work + 1,
        );
    }
    if count + additions > MAX_SCOPES {
        return Err(protocol("The service source budget is full"));
    }
    for (key, next, work) in updates {
        query("INSERT INTO social_service_budget(source,direction,window,bytes,work) VALUES (?,?,?,?,?) ON CONFLICT(source,direction) DO UPDATE SET window=excluded.window,bytes=excluded.bytes,work=excluded.work")
            .bind(key).bind(direction).bind(window).bind(next).bind(work).execute(&mut **tx).await?;
    }
    Ok(())
}

fn limited() -> StoreError {
    protocol("The service limited requests; try later")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Store;

    #[tokio::test]
    async fn ordinary_requests_support_the_full_unsigned_limit() {
        let store = Store::open_memory().await.unwrap();
        spend(&store.pool, "host", "in", 1, u64::MAX, 0)
            .await
            .unwrap();
        let bytes: i64 = query_scalar(
            "SELECT bytes FROM social_service_budget WHERE source='host' AND direction='in'",
        )
        .fetch_one(&store.pool)
        .await
        .unwrap();
        assert_eq!(bytes, 1);
    }
}
