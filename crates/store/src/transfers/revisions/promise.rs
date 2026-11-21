use chrono::{DateTime, Utc};
use nucleus::PromiseState;
use sqlx::{Row, SqlitePool};

use super::{
    RevisionAuthorization, RevisionCommit, RevisionWrite, append_revision,
    cancel_stale_item_delivery, fact_has_transfer_revision_action, reset_agreements_for_revision,
    snapshot::validated_snapshot,
};
use crate::{
    StoreError,
    transfers::{
        correction_link_for_request, item_terms::encode_item, open_claim_pair_for_request,
        promise_successor_for_request, request_ids::ensure_request_not_used_by_invitation_event_on,
    },
};

/// An edit to an existing promise and its expected Transfer revision.
#[derive(Debug, Clone)]
pub struct PromiseRevisionInput {
    pub transfer_uid: String,
    pub item: Option<nucleus::transfer::disclosure::TransferItem>,
    pub promise_uid: String,
    pub expected_revision: u64,
    pub idempotency_key: String,
    pub record_uid: String,
    pub person_uid: String,
    pub delta: f64,
    pub window_end: Option<String>,
    pub condition: Option<String>,
    pub reserve_from: String,
}

/// Revises a proposed or agreed promise and resets its Transfer's agreements.
///
/// # Errors
///
/// Returns an error for an invalid state, conflicting request, invalid terms,
/// or a storage failure.
pub async fn revise_promise<F>(
    pool: &SqlitePool,
    input: PromiseRevisionInput,
    now: DateTime<Utc>,
    fact_actor: Option<String>,
    sign: F,
) -> Result<RevisionCommit, StoreError>
where
    F: Fn(&str) -> Option<String> + Send + Sync,
{
    let request_key = input.idempotency_key.trim();
    if request_key.is_empty() || request_key.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer revision idempotency key must contain 1 to 200 characters".into(),
        ));
    }
    let expected_revision = i64::try_from(input.expected_revision)
        .map_err(|_| sqlx::Error::Protocol("transfer revision exceeds SQLite range".into()))?;
    let mut tx = crate::write_tx(pool).await?;

    if let Some(row) = sqlx::query(
        "SELECT transfer_uid, revision, fact_uid FROM transfer_revision
         WHERE idempotency_key = ?",
    )
    .bind(request_key)
    .fetch_optional(&mut *tx)
    .await?
    {
        let existing_transfer: String = row.get("transfer_uid");
        if existing_transfer != input.transfer_uid {
            return Err(sqlx::Error::Protocol(
                "transfer request id was already used for another transfer".into(),
            ));
        }
        let revision = row.get::<i64, _>("revision") as u64;
        let fact_uid: String = row.get("fact_uid");
        tx.rollback().await?;
        if correction_link_for_request(pool, request_key)
            .await?
            .is_some()
            || promise_successor_for_request(pool, request_key)
                .await?
                .is_some()
            || open_claim_pair_for_request(pool, request_key)
                .await?
                .is_some()
        {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different action".into(),
            ));
        }
        let fact = crate::facts::get(pool, &fact_uid)
            .await?
            .ok_or(sqlx::Error::RowNotFound)?;
        if !fact_has_transfer_revision_action(&fact, "revise-transfer-promise") {
            return Err(sqlx::Error::Protocol(
                "transfer request id was replayed as a different revision action".into(),
            ));
        }
        return Ok(RevisionCommit::Replayed { revision, fact });
    }

    ensure_request_not_used_by_invitation_event_on(&mut tx, request_key).await?;

    let promise = sqlx::query("SELECT state FROM promise WHERE uid = ? AND transfer_uid = ?")
        .bind(&input.promise_uid)
        .bind(&input.transfer_uid)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(sqlx::Error::RowNotFound)?;
    let state: String = promise.get("state");
    if !matches!(
        PromiseState::parse(&state),
        Some(PromiseState::Proposed | PromiseState::Agreed)
    ) {
        return Err(sqlx::Error::Protocol(format!(
            "promise {} cannot be revised from state {state}",
            input.promise_uid
        )));
    }

    let advanced = sqlx::query(
        "UPDATE transfer SET revision = revision + 1
         WHERE record_uid = ? AND revision = ?",
    )
    .bind(&input.transfer_uid)
    .bind(expected_revision)
    .execute(&mut *tx)
    .await?;
    if advanced.rows_affected() == 0 {
        let current_revision: i64 =
            sqlx::query_scalar("SELECT revision FROM transfer WHERE record_uid = ?")
                .bind(&input.transfer_uid)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        tx.rollback().await?;
        return Ok(RevisionCommit::Stale {
            current_revision: current_revision as u64,
        });
    }

    let revision = expected_revision
        .checked_add(1)
        .ok_or_else(|| StoreError::Protocol("transfer revision overflow".into()))?;
    let now_string = now.to_rfc3339();
    sqlx::query(
        "UPDATE promise
         SET record_uid = ?, concept_uid = NULL, party_uid = ?, delta = ?, window_end = ?,
             condition = ?, reserve_from = ?,
             state = CASE WHEN state = 'agreed' THEN 'proposed' ELSE state END,
             revision = ?, updated_at = ?, item_json = COALESCE(?, item_json)
         WHERE uid = ? AND transfer_uid = ?",
    )
    .bind(&input.record_uid)
    .bind(&input.person_uid)
    .bind(input.delta)
    .bind(&input.window_end)
    .bind(&input.condition)
    .bind(&input.reserve_from)
    .bind(revision)
    .bind(&now_string)
    .bind(encode_item(input.item.as_ref())?)
    .bind(&input.promise_uid)
    .bind(&input.transfer_uid)
    .execute(&mut *tx)
    .await?;
    reset_agreements_for_revision(&mut tx, &input.transfer_uid, revision, now).await?;

    let snapshot = validated_snapshot(&mut tx, &input.transfer_uid).await?;
    let fact = append_revision(
        &mut tx,
        snapshot,
        RevisionWrite {
            transfer_uid: &input.transfer_uid,
            revision,
            previous: Some(input.expected_revision),
            request_key,
            action: "revise-transfer-promise",
            actor: fact_actor,
            delta: crate::exact::zero(),
            authorization: RevisionAuthorization::Optional,
        },
        now,
        &now_string,
        &sign,
    )
    .await?;
    crate::records::bump_quantity(
        &mut tx,
        &input.transfer_uid,
        crate::exact::zero(),
        &now_string,
    )
    .await?;
    cancel_stale_item_delivery(&mut tx, &input.transfer_uid, revision).await?;
    tx.commit().await?;

    Ok(RevisionCommit::Committed {
        revision: revision as u64,
        fact,
        parent_facts: Vec::new(),
    })
}
