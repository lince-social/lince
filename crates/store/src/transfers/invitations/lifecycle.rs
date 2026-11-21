use chrono::{DateTime, Utc};
use nucleus::Fact;
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use super::{
    TransferInvitationRow, TransferInvitationStatus,
    events::{NewInvitationEvent, insert_invitation_event, invitation_commit_for_request},
    is_party, map_invitation, pending_on, require_people,
};
use crate::{
    StoreError,
    transfers::{
        RevisionChange, advance_transfer_revision, ensure_request_not_used_by_transfer_revision,
        insert_revision_fact, reset_agreements_for_revision, validate_request_key,
    },
};

/// A request to invite a Person at a specific Transfer revision.
#[derive(Debug, Clone)]
pub struct AddressTransferInvitationInput {
    pub transfer_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub addressed_person_uid: String,
    pub invited_by_person_uid: String,
    pub expires_at: Option<DateTime<Utc>>,
}

/// A transition for an existing invitation, with its actor and optional deadline.
#[derive(Debug, Clone)]
pub struct InvitationTransitionInput {
    pub invitation_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub actor_person_uid: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

/// The event and any revision and party created by the transition.
#[derive(Debug, Clone)]
pub struct InvitationCommitOutcome {
    pub transfer_uid: String,
    pub invitation: TransferInvitationRow,
    pub party_uid: Option<String>,
    pub revision: Option<u64>,
    pub revision_fact: Option<Fact>,
    pub event_fact: Fact,
}

/// Whether the transition committed, replayed, or found a stale revision.
#[derive(Debug, Clone)]
pub enum InvitationCommit {
    Committed(InvitationCommitOutcome),
    Replayed(InvitationCommitOutcome),
    Stale {
        transfer_uid: String,
        current_revision: u64,
    },
}

/// Creates an invitation and advances the Transfer revision.
///
/// # Errors
///
/// Returns an error for an invalid Person, conflicting request, existing
/// invitation, current party, or storage failure.
pub async fn address_transfer_invitation<F>(
    pool: &SqlitePool,
    input: AddressTransferInvitationInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        if outcome.transfer_uid != input.transfer_uid {
            return Err(sqlx::Error::Protocol(
                "transfer request id was already used for another transfer".into(),
            ));
        }
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    require_people(
        &mut tx,
        &input.addressed_person_uid,
        &input.invited_by_person_uid,
    )
    .await?;
    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT uid, status FROM transfer_invitation
         WHERE transfer_uid = ? AND addressed_person_uid = ? LIMIT 1",
    )
    .bind(&input.transfer_uid)
    .bind(&input.addressed_person_uid)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((uid, status)) = existing {
        return Err(sqlx::Error::Protocol(format!(
            "invitation `{uid}` already represents this Person with status `{status}`; reopen it instead"
        )));
    }
    if is_party(&mut tx, &input.transfer_uid, &input.addressed_person_uid).await? {
        return Err(sqlx::Error::Protocol(
            "a current transfer party cannot be invited".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &input.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: input.transfer_uid,
                    current_revision,
                });
            }
        };
    let invitation_uid = nucleus::new_uid("ti");
    let at = now.to_rfc3339();
    sqlx::query(
        "INSERT INTO transfer_invitation
            (uid, transfer_uid, addressed_person_uid, invited_by_person_uid,
             status, party_uid, expires_at, attempt, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'pending', NULL, ?, 1, ?, ?)",
    )
    .bind(&invitation_uid)
    .bind(&input.transfer_uid)
    .bind(&input.addressed_person_uid)
    .bind(&input.invited_by_person_uid)
    .bind(input.expires_at.map(|value| value.to_rfc3339()))
    .bind(&at)
    .bind(&at)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &input.transfer_uid,
        RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id,
            action: "address-transfer-invitation",
        },
        Some(input.invited_by_person_uid.clone()),
        now,
        &sign,
    )
    .await?;
    finish(
        tx,
        input.transfer_uid,
        NewInvitationEvent {
            invitation_uid: &invitation_uid,
            attempt: 1,
            kind: "addressed",
            from_status: None,
            actor_uid: Some(&input.invited_by_person_uid),
            revision: Some(revision.cast_unsigned()),
            request_id,
        },
        Some(revision_fact),
        None,
        (now, &at),
        &sign,
    )
    .await
}

/// Adds the addressed Person as a party and advances the revision.
///
/// # Errors
///
/// Returns an error for an expired or non-pending invitation, a different
/// actor, a conflicting request, or a storage failure.
pub async fn accept_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("accepting an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let pending = pending_on(&mut tx, &input.invitation_uid)
        .await?
        .ok_or_else(|| StoreError::Protocol("invitation is not pending".into()))?;
    if pending.addressed_person_uid != actor_uid {
        return Err(sqlx::Error::Protocol(
            "only the addressed Person may accept an invitation".into(),
        ));
    }
    if pending.expires_by(now) {
        return Err(sqlx::Error::Protocol(
            "expired invitation must be expired and reopened before acceptance".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &pending.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: pending.transfer_uid,
                    current_revision,
                });
            }
        };
    let party_uid = nucleus::new_uid("y");
    sqlx::query(
        "INSERT INTO transfer_party (uid, transfer_uid, actor_uid, kind)
         VALUES (?, ?, ?, 'participant')",
    )
    .bind(&party_uid)
    .bind(&pending.transfer_uid)
    .bind(actor_uid)
    .execute(&mut *tx)
    .await?;
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation SET status = 'accepted', party_uid = ?, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&party_uid)
    .bind(&at)
    .bind(&input.invitation_uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &pending.transfer_uid, revision, now).await?;
    sqlx::query(
        "INSERT INTO transfer_agreement (uid, transfer_uid, party_uid, level, at, revision)
         VALUES (?, ?, ?, 0, ?, ?)",
    )
    .bind(nucleus::new_uid("g"))
    .bind(&pending.transfer_uid)
    .bind(&party_uid)
    .bind(&at)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &pending.transfer_uid,
        RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id,
            action: "accept-transfer-invitation",
        },
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    finish(
        tx,
        pending.transfer_uid,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            attempt: pending.attempt,
            kind: "accepted",
            from_status: Some("pending"),
            actor_uid: Some(actor_uid),
            revision: Some(revision.cast_unsigned()),
            request_id,
        },
        Some(revision_fact),
        Some(party_uid),
        (now, &at),
        &sign,
    )
    .await
}

/// Records the creator's withdrawal and advances the revision.
///
/// # Errors
///
/// Returns an error for an unauthorized actor, a non-pending invitation,
/// a conflicting request, or a storage failure.
pub async fn withdraw_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("withdrawing an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let pending = pending_on(&mut tx, &input.invitation_uid)
        .await?
        .ok_or_else(|| StoreError::Protocol("invitation is not pending".into()))?;
    require_creator(&mut tx, &pending.transfer_uid, actor_uid, "withdraw").await?;
    let revision =
        match advance_transfer_revision(&mut tx, &pending.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: pending.transfer_uid,
                    current_revision,
                });
            }
        };
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'withdrawn', party_uid = NULL, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(&at)
    .bind(&pending.uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &pending.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &pending.transfer_uid,
        RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id,
            action: "withdraw-transfer-invitation",
        },
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    finish(
        tx,
        pending.transfer_uid,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            attempt: pending.attempt,
            kind: "withdrawn",
            from_status: Some("pending"),
            actor_uid: Some(actor_uid),
            revision: Some(revision.cast_unsigned()),
            request_id,
        },
        Some(revision_fact),
        None,
        (now, &at),
        &sign,
    )
    .await
}

/// Starts a new attempt on the same invitation and advances the revision.
///
/// # Errors
///
/// Returns an error for an unauthorized actor, an ineligible invitation,
/// a conflicting request, an exhausted attempt counter, or a storage failure.
pub async fn reopen_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let actor_uid = input.actor_person_uid.as_deref().ok_or_else(|| {
        sqlx::Error::Protocol("reopening an invitation requires a Person actor".into())
    })?;
    let mut tx = crate::write_tx(pool).await?;
    let closed = sqlx::query(
        "SELECT * FROM transfer_invitation
         WHERE uid = ? AND status IN ('rejected', 'withdrawn', 'expired')",
    )
    .bind(&input.invitation_uid)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| sqlx::Error::Protocol("invitation is not reopenable".into()))?;
    let closed = map_invitation(closed)?;
    require_creator(&mut tx, &closed.transfer_uid, actor_uid, "reopen").await?;
    if is_party(&mut tx, &closed.transfer_uid, &closed.addressed_person_uid).await? {
        return Err(sqlx::Error::Protocol(
            "a current transfer party cannot have an invitation reopened".into(),
        ));
    }
    let revision =
        match advance_transfer_revision(&mut tx, &closed.transfer_uid, input.expected_revision)
            .await?
        {
            Ok(revision) => revision,
            Err(current_revision) => {
                tx.rollback().await?;
                return Ok(InvitationCommit::Stale {
                    transfer_uid: closed.transfer_uid,
                    current_revision,
                });
            }
        };
    let attempt = closed
        .attempt
        .checked_add(1)
        .ok_or_else(|| sqlx::Error::Protocol("invitation attempt overflow".into()))?;
    let at = now.to_rfc3339();
    sqlx::query(
        "UPDATE transfer_invitation
         SET status = 'pending', party_uid = NULL, expires_at = ?, attempt = ?,
             invited_by_person_uid = ?, updated_at = ?
         WHERE uid = ? AND status IN ('rejected', 'withdrawn', 'expired')",
    )
    .bind(input.expires_at.map(|value| value.to_rfc3339()))
    .bind(
        i64::try_from(attempt)
            .map_err(|_| sqlx::Error::Protocol("invitation attempt exceeds SQLite range".into()))?,
    )
    .bind(actor_uid)
    .bind(&at)
    .bind(&closed.uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &closed.transfer_uid, revision, now).await?;
    let revision_fact = insert_revision_fact(
        &mut tx,
        &closed.transfer_uid,
        RevisionChange {
            revision,
            previous_revision: input.expected_revision,
            request_id,
            action: "reopen-transfer-invitation",
        },
        Some(actor_uid.into()),
        now,
        &sign,
    )
    .await?;
    finish(
        tx,
        closed.transfer_uid,
        NewInvitationEvent {
            invitation_uid: &closed.uid,
            attempt,
            kind: "reopened",
            from_status: Some(closed.status.as_str()),
            actor_uid: Some(actor_uid),
            revision: Some(revision.cast_unsigned()),
            request_id,
        },
        Some(revision_fact),
        None,
        (now, &at),
        &sign,
    )
    .await
}

enum Closure {
    Rejected,
    Expired,
}

async fn close_transfer_invitation_signed<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    closure: Closure,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_id = validate_request_key(&input.idempotency_key)?;
    if let Some(outcome) = invitation_commit_for_request(pool, request_id).await? {
        return Ok(InvitationCommit::Replayed(outcome));
    }
    ensure_request_not_used_by_transfer_revision(pool, request_id).await?;
    let mut tx = crate::write_tx(pool).await?;
    let pending = pending_on(&mut tx, &input.invitation_uid)
        .await?
        .ok_or_else(|| StoreError::Protocol("invitation is not pending".into()))?;
    let status = match closure {
        Closure::Rejected => {
            if input.actor_person_uid.as_deref() != Some(&pending.addressed_person_uid) {
                return Err(sqlx::Error::Protocol(
                    "only the addressed Person may reject an invitation".into(),
                ));
            }
            TransferInvitationStatus::Rejected
        }
        Closure::Expired => {
            if input.actor_person_uid.is_some() {
                return Err(sqlx::Error::Protocol(
                    "automatic invitation expiry must not attribute a Person actor".into(),
                ));
            }
            if !pending.expires_by(now) {
                return Err(sqlx::Error::Protocol(
                    "invitation is not due to expire".into(),
                ));
            }
            TransferInvitationStatus::Expired
        }
    };
    let at = now.to_rfc3339();
    let transition = sqlx::query(
        "UPDATE transfer_invitation SET status = ?, party_uid = NULL, updated_at = ?
         WHERE uid = ? AND status = 'pending'",
    )
    .bind(status.as_str())
    .bind(&at)
    .bind(&pending.uid)
    .execute(&mut *tx)
    .await?;
    if transition.rows_affected() != 1 {
        return Err(sqlx::Error::Protocol("invitation is not pending".into()));
    }
    finish(
        tx,
        pending.transfer_uid,
        NewInvitationEvent {
            invitation_uid: &pending.uid,
            attempt: pending.attempt,
            kind: status.as_str(),
            from_status: Some("pending"),
            actor_uid: input.actor_person_uid.as_deref(),
            revision: None,
            request_id,
        },
        None,
        None,
        (now, &at),
        &sign,
    )
    .await
}

/// Records a rejection by the addressed Person at the current revision.
///
/// # Errors
///
/// Returns an error for an unauthorized actor, a non-pending invitation,
/// a conflicting request, or a storage failure.
pub async fn reject_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    close_transfer_invitation_signed(pool, input, Closure::Rejected, now, sign).await
}

/// Records a due invitation's expiry without a Person actor.
///
/// # Errors
///
/// Returns an error if expiry is premature, the request has an actor, the
/// invitation is closed, the key conflicts, or storage fails.
pub async fn expire_transfer_invitation<F>(
    pool: &SqlitePool,
    input: InvitationTransitionInput,
    now: DateTime<Utc>,
    sign: F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    close_transfer_invitation_signed(pool, input, Closure::Expired, now, sign).await
}

async fn require_creator(
    connection: &mut SqliteConnection,
    transfer: &str,
    actor: &str,
    operation: &str,
) -> Result<(), StoreError> {
    let creator: Option<String> = sqlx::query_scalar(
        "SELECT actor_uid FROM transfer_party WHERE transfer_uid = ? AND kind = 'creator' LIMIT 1",
    )
    .bind(transfer)
    .fetch_optional(connection)
    .await?;
    if creator.as_deref() != Some(actor) {
        return Err(StoreError::Protocol(format!(
            "only the Transfer creator may {operation} an invitation"
        )));
    }
    Ok(())
}

async fn finish<F>(
    mut tx: Transaction<'_, Sqlite>,
    transfer_uid: String,
    event: NewInvitationEvent<'_>,
    revision_fact: Option<Fact>,
    party_uid: Option<String>,
    (now, at): (DateTime<Utc>, &str),
    sign: &F,
) -> Result<InvitationCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let event_fact = insert_invitation_event(&mut tx, &transfer_uid, &event, now, sign).await?;
    crate::records::bump_quantity(&mut tx, &transfer_uid, crate::exact::zero(), at).await?;
    let row = sqlx::query("SELECT * FROM transfer_invitation WHERE uid = ?")
        .bind(event.invitation_uid)
        .fetch_one(&mut *tx)
        .await?;
    let invitation = map_invitation(row)?;
    tx.commit().await?;
    Ok(InvitationCommit::Committed(InvitationCommitOutcome {
        transfer_uid,
        invitation,
        party_uid,
        revision: event.revision,
        revision_fact,
        event_fact,
    }))
}
