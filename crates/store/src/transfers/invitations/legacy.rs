use chrono::{DateTime, Utc};
use sqlx::SqlitePool;

use super::{
    TransferInvitationRow, TransferInvitationStatus, invitation, is_party, map_invitation,
    pending_on, require_people,
};
use crate::StoreError;

/// The People and deadline for a legacy invitation.
#[derive(Debug)]
pub struct NewTransferInvitation<'a> {
    pub transfer_uid: &'a str,
    pub addressed_person_uid: &'a str,
    pub invited_by_person_uid: &'a str,
    pub expires_at: Option<DateTime<Utc>>,
}

/// The accepted invitation and its newly assigned party.
#[derive(Debug, Clone)]
pub struct AcceptedTransferInvitation {
    pub invitation: TransferInvitationRow,
    pub party_uid: String,
}

/// Creates a pending invitation without event or revision evidence.
///
/// # Errors
///
/// Returns an error for an invalid Person, an existing party, or a storage failure.
pub async fn create_invitation(
    pool: &SqlitePool,
    new: NewTransferInvitation<'_>,
    now: DateTime<Utc>,
) -> Result<TransferInvitationRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    require_people(&mut tx, new.addressed_person_uid, new.invited_by_person_uid).await?;
    if is_party(&mut tx, new.transfer_uid, new.addressed_person_uid).await? {
        return Err(sqlx::Error::Protocol(format!(
            "Person `{}` is already a party of transfer `{}`",
            new.addressed_person_uid, new.transfer_uid
        )));
    }

    let uid = nucleus::new_uid("ti");
    let at = now.to_rfc3339();
    let expires_at = new.expires_at.map(|value| value.to_rfc3339());
    sqlx::query(
        "INSERT INTO transfer_invitation
            (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
             status, expires_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(&uid)
    .bind(new.transfer_uid)
    .bind(new.addressed_person_uid)
    .bind(new.invited_by_person_uid)
    .bind(expires_at)
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;

    let row = sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(&uid)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;
    map_invitation(row)
}

/// Accepts a pending invitation through the legacy row-only API.
///
/// # Errors
///
/// Returns an error if storage fails or the invitation has an invalid status.
pub async fn accept_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<AcceptedTransferInvitation>, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let Some(pending) = pending_on(&mut tx, uid).await? else {
        return Ok(None);
    };

    let party_uid = nucleus::new_uid("y");
    sqlx::query("INSERT INTO transfer_party (uid, transfer_uid, actor_uid) VALUES (?, ?, ?)")
        .bind(&party_uid)
        .bind(&pending.transfer_uid)
        .bind(&pending.addressed_person_uid)
        .execute(&mut *tx)
        .await?;

    let transition = sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'accepted', party_uid = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&party_uid)
    .bind(now.to_rfc3339())
    .bind(uid)
    .execute(&mut *tx)
    .await?;
    if transition.rows_affected() != 1 {
        tx.rollback().await?;
        return Ok(None);
    }
    let invitation = sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(uid)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    Ok(Some(AcceptedTransferInvitation {
        invitation: map_invitation(invitation)?,
        party_uid,
    }))
}

async fn close_pending_invitation(
    pool: &SqlitePool,
    uid: &str,
    status: TransferInvitationStatus,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    debug_assert!(matches!(
        status,
        TransferInvitationStatus::Rejected
            | TransferInvitationStatus::Withdrawn
            | TransferInvitationStatus::Expired
    ));
    let result = sqlx::query(
        "UPDATE transfer_invitation SET status = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(status.as_str())
    .bind(now.to_rfc3339())
    .bind(uid)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Ok(None);
    }
    invitation(pool, uid).await
}

/// Closes a pending invitation as rejected through the legacy API.
///
/// # Errors
///
/// Returns an error if storage fails or the resulting status is invalid.
pub async fn reject_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Rejected, now).await
}

/// Closes a pending invitation as withdrawn through the legacy API.
///
/// # Errors
///
/// Returns an error if storage fails or the resulting status is invalid.
pub async fn withdraw_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Withdrawn, now).await
}

/// Closes a pending invitation as expired through the legacy API.
///
/// # Errors
///
/// Returns an error if storage fails or the resulting status is invalid.
pub async fn expire_invitation(
    pool: &SqlitePool,
    uid: &str,
    now: DateTime<Utc>,
) -> Result<Option<TransferInvitationRow>, StoreError> {
    close_pending_invitation(pool, uid, TransferInvitationStatus::Expired, now).await
}
