use chrono::{DateTime, Utc};
use nucleus::{Cause, Fact, NewFact, transfer::TransferInvitationLifecycleEvidence};
use sqlx::{Row, Sqlite, SqlitePool, Transaction, sqlite::SqliteRow};

use super::{TransferInvitationStatus, invitation, lifecycle::InvitationCommitOutcome};
use crate::{StoreError, transfers::revision_fact};

/// The immutable event recorded for one invitation attempt.
#[derive(Debug, Clone)]
pub struct TransferInvitationEventRow {
    pub uid: String,
    pub invitation_uid: String,
    pub transfer_uid: String,
    pub attempt: u64,
    pub kind: String,
    pub actor_uid: Option<String>,
    pub revision: Option<u64>,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

fn map_invitation_event(row: SqliteRow) -> TransferInvitationEventRow {
    TransferInvitationEventRow {
        uid: row.get("uid"),
        invitation_uid: row.get("invitation_uid"),
        transfer_uid: row.get("transfer_uid"),
        attempt: row.get::<i64, _>("attempt").cast_unsigned(),
        kind: row.get("kind"),
        actor_uid: row.get("actor_uid"),
        revision: row
            .get::<Option<i64>, _>("revision")
            .map(i64::cast_unsigned),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Reads an invitation's history by attempt, time, and UID.
///
/// # Errors
///
/// Returns a storage error if the history cannot be read.
pub async fn invitation_events(
    pool: &SqlitePool,
    invitation_uid: &str,
) -> Result<Vec<TransferInvitationEventRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM transfer_invitation_event
         WHERE invitation_uid = ? ORDER BY attempt, created_at, uid",
    )
    .bind(invitation_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_invitation_event)
    .collect())
}

/// Finds an event by request key, ignoring surrounding whitespace.
///
/// # Errors
///
/// Returns a storage error if the event cannot be read.
pub async fn invitation_event_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<TransferInvitationEventRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM transfer_invitation_event WHERE idempotency_key = ?")
            .bind(request_id.trim())
            .fetch_optional(pool)
            .await?
            .map(map_invitation_event),
    )
}

fn invitation_status_for_event(kind: &str) -> Result<TransferInvitationStatus, StoreError> {
    match kind {
        "addressed" | "reopened" => Ok(TransferInvitationStatus::Pending),
        "accepted" => Ok(TransferInvitationStatus::Accepted),
        "rejected" => Ok(TransferInvitationStatus::Rejected),
        "withdrawn" => Ok(TransferInvitationStatus::Withdrawn),
        "expired" => Ok(TransferInvitationStatus::Expired),
        value => Err(sqlx::Error::Protocol(format!(
            "unknown transfer invitation event kind `{value}`"
        ))),
    }
}

pub(super) async fn invitation_commit_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<InvitationCommitOutcome>, StoreError> {
    let Some(event) = invitation_event_for_request(pool, request_id).await? else {
        return Ok(None);
    };
    let mut invitation = invitation(pool, &event.invitation_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    invitation.status = invitation_status_for_event(&event.kind)?;
    invitation.attempt = event.attempt;
    if invitation.status != TransferInvitationStatus::Accepted {
        invitation.party_uid = None;
    }
    let event_fact = crate::facts::get(pool, &event.fact_uid)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let revision_fact = if let Some(revision) = event.revision {
        revision_fact(pool, &event.transfer_uid, revision).await?
    } else {
        None
    };
    Ok(Some(InvitationCommitOutcome {
        transfer_uid: event.transfer_uid,
        party_uid: invitation.party_uid.clone(),
        invitation,
        revision: event.revision,
        revision_fact,
        event_fact,
    }))
}

pub(in super::super) struct NewInvitationEvent<'a> {
    pub(in super::super) invitation_uid: &'a str,
    pub(in super::super) attempt: u64,
    pub(in super::super) kind: &'a str,
    pub(in super::super) from_status: Option<&'a str>,
    pub(in super::super) actor_uid: Option<&'a str>,
    pub(in super::super) revision: Option<u64>,
    pub(in super::super) request_id: &'a str,
}

pub(in super::super) async fn insert_invitation_event<F>(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    event: &NewInvitationEvent<'_>,
    now: DateTime<Utc>,
    sign: &F,
) -> Result<Fact, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let evidence = TransferInvitationLifecycleEvidence {
        action: format!("{}-transfer-invitation", event.kind),
        idempotency_key: event.request_id.into(),
        invitation_uid: event.invitation_uid.into(),
        transfer_uid: transfer_uid.into(),
        attempt: event.attempt,
        from_status: event.from_status.map(str::to_owned),
        to_status: invitation_status_for_event(event.kind)?.as_str().into(),
        actor_person_uid: event.actor_uid.map(str::to_owned),
        revision: event.revision,
    };
    let payload = serde_json::to_string(&evidence)
        .map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    let previous_hash = crate::facts::last_hash(tx).await?;
    let mut fact = nucleus::fact::seal(
        NewFact {
            uid: None,
            record_uid: transfer_uid.into(),
            delta: crate::exact::zero(),
            at: None,
            actor_uid: event.actor_uid.map(str::to_owned),
            cause: Cause::user_edit(),
            payload: Some(payload),
        },
        &previous_hash,
        now,
    );
    fact.signature = sign(&fact.hash);
    crate::facts::insert(tx, &fact).await?;
    sqlx::query(
        "INSERT INTO transfer_invitation_event
            (uid, invitation_uid, transfer_uid, attempt, kind, actor_uid,
             revision, fact_uid, idempotency_key, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("tie"))
    .bind(event.invitation_uid)
    .bind(transfer_uid)
    .bind(
        i64::try_from(event.attempt)
            .map_err(|_| sqlx::Error::Protocol("invitation attempt exceeds SQLite range".into()))?,
    )
    .bind(event.kind)
    .bind(event.actor_uid)
    .bind(event.revision.map(u64::cast_signed))
    .bind(&fact.uid)
    .bind(event.request_id)
    .bind(now.to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(fact)
}
