use chrono::{DateTime, Duration, Utc};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateKind {
    Refusal,
    FullLogServe,
}

impl RateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            RateKind::Refusal => "refusal",
            RateKind::FullLogServe => "full-log-serve",
        }
    }

    fn allowance(self) -> i64 {
        match self {
            RateKind::Refusal => 200,
            RateKind::FullLogServe => 4,
        }
    }

    fn window(self) -> Duration {
        Duration::hours(1)
    }

    fn backoff(self) -> Duration {
        match self {
            RateKind::Refusal => Duration::hours(1),
            RateKind::FullLogServe => Duration::hours(6),
        }
    }

    fn reason(self) -> &'static str {
        match self {
            RateKind::Refusal => {
                "too many refused ops in one hour; answering this contact less often"
            }
            RateKind::FullLogServe => {
                "asked for the whole log too many times in one hour; \
                 answering this contact less often"
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RateState {
    pub kind: String,
    pub count: i64,
    pub allowance: i64,
    pub window_start: String,
    pub backoff_until: Option<String>,
    pub reason: Option<String>,
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|stamp| stamp.with_timezone(&Utc))
}

pub async fn backing_off(
    pool: &SqlitePool,
    from_organ: &str,
    kind: RateKind,
) -> Result<Option<String>, StoreError> {
    let row = sqlx::query(
        "SELECT backoff_until, reason FROM contact_rate WHERE from_organ = ? AND kind = ?",
    )
    .bind(from_organ)
    .bind(kind.as_str())
    .fetch_optional(pool)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let until: Option<String> = row.get("backoff_until");
    let Some(until) = until.as_deref().and_then(parse_time) else {
        return Ok(None);
    };
    if until <= Utc::now() {
        return Ok(None);
    }
    Ok(Some(
        row.get::<Option<String>, _>("reason")
            .unwrap_or_else(|| kind.reason().to_string()),
    ))
}

pub async fn spend(
    pool: &SqlitePool,
    from_organ: &str,
    kind: RateKind,
) -> Result<Option<String>, StoreError> {
    let now = Utc::now();
    let mut tx = crate::write_tx(pool).await?;
    let row = sqlx::query(
        "SELECT window_start, count, backoff_until FROM contact_rate
          WHERE from_organ = ? AND kind = ?",
    )
    .bind(from_organ)
    .bind(kind.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let mut window_start = now;
    let mut count = 0_i64;
    if let Some(row) = row {
        let stored: String = row.get("window_start");
        if let Some(stamp) = parse_time(&stored) {
            if now.signed_duration_since(stamp) < kind.window() {
                window_start = stamp;
                count = row.get("count");
            }
        }
    }
    count += 1;

    let exhausted = count > kind.allowance();
    let backoff_until = exhausted.then(|| (now + kind.backoff()).to_rfc3339());
    let reason = exhausted.then(|| kind.reason().to_string());

    sqlx::query(
        "INSERT INTO contact_rate (from_organ, kind, window_start, count, backoff_until, reason)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(from_organ, kind) DO UPDATE SET
             window_start  = excluded.window_start,
             count         = excluded.count,
             backoff_until = COALESCE(excluded.backoff_until, contact_rate.backoff_until),
             reason        = COALESCE(excluded.reason, contact_rate.reason)",
    )
    .bind(from_organ)
    .bind(kind.as_str())
    .bind(window_start.to_rfc3339())
    .bind(count)
    .bind(backoff_until)
    .bind(&reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(reason)
}

pub async fn states(pool: &SqlitePool, from_organ: &str) -> Result<Vec<RateState>, StoreError> {
    Ok(sqlx::query(
        "SELECT kind, window_start, count, backoff_until, reason FROM contact_rate
          WHERE from_organ = ? ORDER BY kind",
    )
    .bind(from_organ)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        let kind: String = row.get("kind");
        let allowance = match kind.as_str() {
            "refusal" => RateKind::Refusal.allowance(),
            _ => RateKind::FullLogServe.allowance(),
        };
        RateState {
            kind,
            count: row.get("count"),
            allowance,
            window_start: row.get("window_start"),
            backoff_until: row.get("backoff_until"),
            reason: row.get("reason"),
        }
    })
    .collect())
}

pub async fn clear(pool: &SqlitePool, from_organ: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM contact_rate WHERE from_organ = ?")
        .bind(from_organ)
        .execute(pool)
        .await?;
    Ok(())
}
