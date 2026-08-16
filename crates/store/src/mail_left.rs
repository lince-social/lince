//! What this Cell left with a carrier, and what came back about it.
//!
//! The counterpart of `mailbox`: that module is what a box holds FOR other
//! people, this one is what we handed to somebody else's box. They never meet
//! in one process except by coincidence — a Cell can be both — and they share
//! nothing but the bundle uid, which is the carrier's name for the thing.
//!
//! Its whole reason for existing is belief. A carrier reporting an expiry is
//! reporting a failure about mail it was holding, and nothing in that report
//! is signed; matching the uid against a row here is what separates "your
//! message was never picked up" from a stranger's assertion.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftMail {
    pub uid: String,
    pub carrier_organ: String,
    pub carrier_node: String,
    pub to_organ: String,
    pub left_at: String,
    pub expired_at: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> LeftMail {
    LeftMail {
        uid: row.get("uid"),
        carrier_organ: row.get("carrier_organ"),
        carrier_node: row.get("carrier_node"),
        to_organ: row.get("to_organ"),
        left_at: row.get("left_at"),
        expired_at: row.get("expired_at"),
    }
}

/// Record a deposit the carrier accepted.
pub async fn record(
    pool: &SqlitePool,
    uid: &str,
    carrier_organ: &str,
    carrier_node: &str,
    to_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO mail_left
           (uid, carrier_organ, carrier_node, to_organ, left_at, expired_at)
         VALUES (?, ?, ?, ?, ?, NULL)",
    )
    .bind(uid)
    .bind(carrier_organ)
    .bind(carrier_node)
    .bind(to_organ)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// The carriers worth asking: every box holding something of ours that has
/// not already been reported expired.
///
/// One ask per node, not per bundle. A Cell that left forty batches with the
/// same box asks it once.
pub async fn carriers_to_ask(pool: &SqlitePool) -> Result<Vec<String>, StoreError> {
    Ok(
        sqlx::query("SELECT DISTINCT carrier_node FROM mail_left WHERE expired_at IS NULL")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| row.get("carrier_node"))
            .collect(),
    )
}

/// Mark one of our deposits expired, on a carrier's report.
///
/// Scoped to the carrier that is reporting, so a box can only ever speak about
/// mail it was actually given. Returns whether a row matched — `false` means
/// the report named something we never left there, and the caller drops it in
/// silence rather than alarming anybody.
pub async fn mark_expired(
    pool: &SqlitePool,
    carrier_node: &str,
    uid: &str,
    expired_at: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query(
        "UPDATE mail_left SET expired_at = ?
          WHERE uid = ? AND carrier_node = ? AND expired_at IS NULL",
    )
    .bind(expired_at)
    .bind(uid)
    .bind(carrier_node)
    .execute(pool)
    .await?
    .rows_affected()
        > 0)
}

/// Everything reported expired, newest first — what a person is shown.
pub async fn expired(pool: &SqlitePool, limit: i64) -> Result<Vec<LeftMail>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM mail_left WHERE expired_at IS NOT NULL
          ORDER BY expired_at DESC, uid LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

/// How much is still out with carriers, unreported either way.
pub async fn outstanding(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COUNT(*) AS n FROM mail_left WHERE expired_at IS NULL")
            .fetch_one(pool)
            .await?
            .get("n"),
    )
}

/// Forget rows older than the carrier could possibly still hold, and expiry
/// reports the person has had ample time to see.
///
/// Bounded by age rather than by an acknowledgement, because there is no
/// acknowledgement to wait for: a bundle that was collected normally produces
/// no message at all, so an un-expired row past the retention window means it
/// arrived — the ordinary case — and keeping it would make this table grow
/// forever with the record of every batch that worked.
pub async fn prune(pool: &SqlitePool, keep_days: i64) -> Result<u64, StoreError> {
    let cutoff = (Utc::now() - chrono::Duration::days(keep_days)).to_rfc3339();
    Ok(sqlx::query("DELETE FROM mail_left WHERE left_at < ?")
        .bind(&cutoff)
        .execute(pool)
        .await?
        .rows_affected())
}
