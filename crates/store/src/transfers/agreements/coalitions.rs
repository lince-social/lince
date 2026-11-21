//! Membership frozen by a percentage agreement.

use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::StoreError;

/// The threshold, members, and triggering event of a frozen coalition.
#[derive(Debug, Clone)]
pub struct AgreementCoalitionRow {
    pub transfer_uid: String,
    pub revision: u64,
    pub threshold_pct: u8,
    pub eligible_count: usize,
    pub frozen_by_event_uid: String,
    pub frozen_at: String,
    pub party_uids: Vec<String>,
}

/// Reads the frozen coalition for a revision, if one was formed.
///
/// # Errors
///
/// Returns an error if the revision exceeds SQLite's range or the lookup fails.
pub async fn agreement_coalition(
    pool: &SqlitePool,
    transfer_uid: &str,
    revision: u64,
) -> Result<Option<AgreementCoalitionRow>, StoreError> {
    let mut connection = pool.acquire().await?;
    agreement_coalition_on(&mut connection, transfer_uid, revision).await
}

pub(super) async fn agreement_coalition_on(
    connection: &mut SqliteConnection,
    transfer_uid: &str,
    revision: u64,
) -> Result<Option<AgreementCoalitionRow>, StoreError> {
    let stored_revision = i64::try_from(revision)
        .map_err(|_| StoreError::Protocol("transfer revision exceeds SQLite range".into()))?;
    let Some(row) = sqlx::query(
        "SELECT * FROM transfer_agreement_coalition
         WHERE transfer_uid = ? AND revision = ?",
    )
    .bind(transfer_uid)
    .bind(stored_revision)
    .fetch_optional(&mut *connection)
    .await?
    else {
        return Ok(None);
    };
    let party_uids = sqlx::query_scalar(
        "SELECT party_uid FROM transfer_agreement_coalition_member
         WHERE transfer_uid = ? AND revision = ? ORDER BY party_uid",
    )
    .bind(transfer_uid)
    .bind(stored_revision)
    .fetch_all(&mut *connection)
    .await?;
    Ok(Some(AgreementCoalitionRow {
        transfer_uid: transfer_uid.into(),
        revision,
        threshold_pct: row.get("threshold_pct"),
        eligible_count: row.get::<i64, _>("eligible_count") as usize,
        frozen_by_event_uid: row.get("frozen_by_event_uid"),
        frozen_at: row.get("frozen_at"),
        party_uids,
    }))
}

pub(super) async fn excluded(
    connection: &mut SqliteConnection,
    transfer: &str,
    revision: i64,
    party: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition
             WHERE transfer_uid = ?1 AND revision = ?2)
         AND NOT EXISTS(SELECT 1 FROM transfer_agreement_coalition_member
             WHERE transfer_uid = ?1 AND revision = ?2 AND party_uid = ?3)",
    )
    .bind(transfer)
    .bind(revision)
    .bind(party)
    .fetch_one(connection)
    .await
}

pub(super) async fn freeze_if_ready(
    connection: &mut SqliteConnection,
    transfer: &str,
    revision: i64,
    threshold: i64,
    event: &str,
    at: &str,
    cancellation_pending: bool,
) -> Result<(), StoreError> {
    if !(1..=100).contains(&threshold) {
        return Err(StoreError::Protocol(
            "percentage agreement requires a threshold between 1 and 100".into(),
        ));
    }
    let (frozen, eligible, committed): (bool, i64, i64) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM transfer_agreement_coalition
                        WHERE transfer_uid = ?1 AND revision = ?2),
                (SELECT count(*) FROM transfer_party WHERE transfer_uid = ?1),
                (SELECT count(*) FROM transfer_agreement
                 WHERE transfer_uid = ?1 AND revision = ?2 AND level = 2)",
    )
    .bind(transfer)
    .bind(revision)
    .fetch_one(&mut *connection)
    .await?;
    if frozen
        || eligible == 0
        || i128::from(committed) * 100 < i128::from(eligible) * i128::from(threshold)
        || cancellation_pending && committed != eligible
    {
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO transfer_agreement_coalition
            (transfer_uid, revision, threshold_pct, eligible_count, frozen_by_event_uid, frozen_at)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(transfer)
    .bind(revision)
    .bind(threshold)
    .bind(eligible)
    .bind(event)
    .bind(at)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO transfer_agreement_coalition_member (transfer_uid, revision, party_uid)
         SELECT transfer_uid, revision, party_uid FROM transfer_agreement
         WHERE transfer_uid = ? AND revision = ? AND level = 2",
    )
    .bind(transfer)
    .bind(revision)
    .execute(connection)
    .await?;
    Ok(())
}
