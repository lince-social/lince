use chrono::{DateTime, Duration, Utc};
use nucleus::execution::now;
use sqlx::{SqlitePool, query, query_scalar};

use crate::StoreError;

/// Records when an Organ first began waiting for its roster.
pub async fn mark_awaiting_roster(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query(
        "UPDATE organ_contact SET awaiting_roster_since = ?
          WHERE record_uid = ? AND awaiting_roster_since IS NULL",
    )
    .bind(now().to_rfc3339())
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Checks whether a valid roster wait timestamp is older than the grace period.
pub async fn awaiting_roster_longer_than(
    pool: &SqlitePool,
    organ_uid: &str,
    grace: Duration,
) -> Result<bool, StoreError> {
    let since: Option<String> =
        query_scalar("SELECT awaiting_roster_since FROM organ_contact WHERE record_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .flatten();
    let Some(since) = since else {
        return Ok(false);
    };
    let Ok(since) = DateTime::parse_from_rfc3339(&since) else {
        return Ok(false);
    };
    Ok(now().signed_duration_since(since.with_timezone(&Utc)) > grace)
}

/// Clears the saved roster wait timestamp.
pub async fn clear_awaiting_roster(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query(
        "UPDATE organ_contact SET awaiting_roster_since = NULL
          WHERE record_uid = ? AND awaiting_roster_since IS NOT NULL",
    )
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records when a contact first became unreachable.
pub async fn mark_unreachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query(
        "UPDATE organ_contact SET unreachable_since = ?
          WHERE record_uid = ? AND unreachable_since IS NULL",
    )
    .bind(now().to_rfc3339())
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Clears the unreachable and mailed timestamps when direct delivery resumes.
pub async fn mark_reachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query(
        "UPDATE organ_contact SET unreachable_since = NULL, mailed_at = NULL
          WHERE record_uid = ? AND (unreachable_since IS NOT NULL OR mailed_at IS NOT NULL)",
    )
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Records the latest mailbox delivery time.
pub async fn mark_mailed(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET mailed_at = ? WHERE record_uid = ?")
        .bind(now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Clears the mailbox delivery timestamp.
pub async fn mark_mailed_clear(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET mailed_at = NULL WHERE record_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Sets the saved start of the unreachable period.
pub async fn backdate_unreachable(
    pool: &SqlitePool,
    organ_uid: &str,
    when: &str,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET unreachable_since = ? WHERE record_uid = ?")
        .bind(when)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Stores the last synchronized sequence for a contact.
pub async fn set_last_synced_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET last_synced_seq = ? WHERE record_uid = ?")
        .bind(seq)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Advances a contact's acknowledgment without lowering its current value.
pub async fn advance_peer_acked_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET peer_acked_seq = MAX(peer_acked_seq, ?) WHERE record_uid = ?")
        .bind(seq)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// The stored preference for direct or mailbox delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Direct,
    Mailbox,
    Auto,
}

impl Delivery {
    pub(super) fn from_mode(mode: &str) -> Self {
        match mode {
            "direct" => Self::Direct,
            "mailbox" => Self::Mailbox,
            _ => Self::Auto,
        }
    }

    /// Returns the saved spelling of the delivery preference.
    pub fn as_str(self) -> &'static str {
        match self {
            Delivery::Direct => "direct",
            Delivery::Mailbox => "mailbox",
            Delivery::Auto => "auto",
        }
    }
}

/// Stores the contact's delivery mode.
pub async fn set_mode(pool: &SqlitePool, organ_uid: &str, mode: &str) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET mode = ? WHERE record_uid = ?")
        .bind(mode)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Stores the contact's catch-up interval in seconds.
pub async fn set_catchup_interval(
    pool: &SqlitePool,
    organ_uid: &str,
    secs: i64,
) -> Result<(), StoreError> {
    query("UPDATE organ_contact SET catchup_interval_secs = ? WHERE record_uid = ?")
        .bind(secs)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Returns the shortest active incoming catch-up interval.
pub async fn minimum_catchup_interval(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    query_scalar(
        "SELECT MIN(c.catchup_interval_secs) FROM organ_contact c
         JOIN record r ON r.uid = c.record_uid
         WHERE c.sync_in <> 0 AND c.catchup_interval_secs > 0 AND c.trust <> 'blocked'",
    )
    .fetch_one(pool)
    .await
}
