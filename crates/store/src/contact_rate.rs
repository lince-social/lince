//! Persisted per-contact limits for refusals and public evidence requests.

use chrono::{DateTime, Duration, Utc};
use nucleus::execution::now;
use sqlx::{Row, SqlitePool, query, sqlite::SqliteRow};

use crate::{StoreError, write_tx};

/// Independent rate buckets for a contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RateKind {
    /// Refused operations from the contact.
    Refusal,
    /// Requests for the entire operation log.
    FullLogServe,
    /// Requests to fetch evidence.
    EvidenceServe,
}

impl RateKind {
    /// Returns the stable key stored for this rate bucket.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Refusal => "refusal",
            Self::FullLogServe => "full-log-serve",
            Self::EvidenceServe => "evidence-serve",
        }
    }

    fn allowance(self) -> i64 {
        match self {
            Self::Refusal => 200,
            Self::FullLogServe => 4,
            Self::EvidenceServe => 128,
        }
    }

    fn window(self) -> Duration {
        match self {
            Self::EvidenceServe => Duration::minutes(1),
            _ => Duration::hours(1),
        }
    }

    fn backoff(self) -> Duration {
        match self {
            Self::Refusal => Duration::hours(1),
            Self::FullLogServe => Duration::hours(6),
            Self::EvidenceServe => Duration::minutes(1),
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Refusal => "too many refused ops in one hour; answering this contact less often",
            Self::FullLogServe => {
                "asked for the whole log too many times in one hour; \
                 answering this contact less often"
            }
            Self::EvidenceServe => {
                "too many evidence fetches in one minute; answering evidence queries less often"
            }
        }
    }
}

/// Stored counter and backoff details for one bucket.
#[derive(Debug, Clone)]
pub struct RateState {
    /// Stored rate-bucket key.
    pub kind: String,
    /// Operations counted in the stored window.
    pub count: i64,
    /// Operations allowed before backoff.
    pub allowance: i64,
    /// Start of the stored window.
    pub window_start: String,
    /// Last stored backoff deadline, including a past deadline.
    pub backoff_until: Option<String>,
    /// Last stored explanation for backoff.
    pub reason: Option<String>,
}

fn parse_time(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|stamp| stamp.to_utc())
}

/// Returns the reason for an active backoff.
/// # Errors
/// Returns an error if the contact’s rate data cannot be read or decoded.
pub async fn backing_off(
    pool: &SqlitePool,
    from_organ: &str,
    kind: RateKind,
) -> Result<Option<String>, StoreError> {
    let row =
        query("SELECT backoff_until, reason FROM contact_rate WHERE from_organ = ? AND kind = ?")
            .bind(from_organ)
            .bind(kind.as_str())
            .fetch_optional(pool)
            .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let Some(until) = row
        .try_get::<Option<&str>, _>("backoff_until")?
        .and_then(parse_time)
    else {
        return Ok(None);
    };
    if until <= now() {
        return Ok(None);
    }
    Ok(Some(
        row.try_get::<Option<String>, _>("reason")?
            .unwrap_or_else(|| kind.reason().to_owned()),
    ))
}

/// Counts one operation and returns a reason if the allowance is exceeded.
/// # Errors
/// Returns an error if the counter cannot be incremented or the transaction fails.
pub async fn spend(
    pool: &SqlitePool,
    from_organ: &str,
    kind: RateKind,
) -> Result<Option<String>, StoreError> {
    let now = now();
    let mut tx = write_tx(pool).await?;
    let row = query(
        "SELECT window_start, count FROM contact_rate
          WHERE from_organ = ? AND kind = ?",
    )
    .bind(from_organ)
    .bind(kind.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let (window_start, count) = if let Some(row) = row
        && let Some(stamp) = parse_time(row.try_get("window_start")?)
        && now.signed_duration_since(stamp) < kind.window()
    {
        (stamp, row.try_get::<i64, _>("count")?)
    } else {
        (now, 0)
    };
    let count = count.checked_add(1).ok_or_else(|| {
        StoreError::Protocol("Contact rate counter exceeds SQLite integer range".into())
    })?;

    let exhausted = count > kind.allowance();
    let backoff_until = exhausted.then(|| (now + kind.backoff()).to_rfc3339());
    let reason = exhausted.then_some(kind.reason());

    query(
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
    .bind(reason)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(reason.map(str::to_owned))
}

/// Lists the contact’s stored bucket state in key order.
/// # Errors
/// Returns an error if the counters or backoff details cannot be read or decoded.
pub async fn states(pool: &SqlitePool, from_organ: &str) -> Result<Vec<RateState>, StoreError> {
    query(
        "SELECT kind, window_start, count, backoff_until, reason FROM contact_rate
          WHERE from_organ = ? ORDER BY kind",
    )
    .bind(from_organ)
    .try_map(|row: SqliteRow| {
        let kind: String = row.try_get("kind")?;
        let allowance = match kind.as_str() {
            "refusal" => RateKind::Refusal.allowance(),
            "evidence-serve" => RateKind::EvidenceServe.allowance(),
            _ => RateKind::FullLogServe.allowance(),
        };
        Ok(RateState {
            kind,
            count: row.try_get("count")?,
            allowance,
            window_start: row.try_get("window_start")?,
            backoff_until: row.try_get("backoff_until")?,
            reason: row.try_get("reason")?,
        })
    })
    .fetch_all(pool)
    .await
}

/// Clears the contact’s stored rate buckets.
/// # Errors
/// Returns an error if the stored buckets cannot be deleted.
pub async fn clear(pool: &SqlitePool, from_organ: &str) -> Result<(), StoreError> {
    query("DELETE FROM contact_rate WHERE from_organ = ?")
        .bind(from_organ)
        .execute(pool)
        .await?;
    Ok(())
}
