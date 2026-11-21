use sqlx::{SqliteConnection, SqliteExecutor, SqlitePool};

use super::{
    agreement_event_for_request, correction_link_for_request, invitation_event_for_request,
    open_claim_pair_for_request, phase6_bulk_request_for_request, promise_successor_for_request,
    revision_for_request,
};
use crate::StoreError;

pub(super) fn validate_request_key(value: &str) -> Result<&str, StoreError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        return Err(sqlx::Error::Protocol(
            "transfer request id must contain 1 to 200 characters".into(),
        ));
    }
    Ok(value)
}

/// Reads the operation kind, target UID, and fact UID for a phase-four request key.
///
/// # Errors
///
/// Returns a storage error if the request cannot be read.
pub async fn phase4_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    sqlx::query_as(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase4_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await
}

pub(super) async fn ensure_phase4_request_unused(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    // Use the existing lookups so malformed revision and open-claim evidence still surfaces.
    if phase4_request_for_request(pool, request_id)
        .await?
        .is_some()
        || revision_for_request(pool, request_id).await?.is_some()
        || invitation_event_for_request(pool, request_id)
            .await?
            .is_some()
        || agreement_event_for_request(pool, request_id)
            .await?
            .is_some()
        || phase5_request_for_request(pool, request_id)
            .await?
            .is_some()
        || phase5_correction_request_for_request(pool, request_id)
            .await?
            .is_some()
        || phase6_bulk_request_for_request(pool, request_id)
            .await?
            .is_some()
        || open_claim_pair_for_request(pool, request_id)
            .await?
            .is_some()
        || correction_link_for_request(pool, request_id)
            .await?
            .is_some()
        || promise_successor_for_request(pool, request_id)
            .await?
            .is_some()
    {
        return Err(sqlx::Error::Protocol(
            "transfer request id was already used by another action".into(),
        ));
    }
    Ok(())
}

/// Reads the operation kind, target UID, and fact UID for a settlement request key.
///
/// # Errors
///
/// Returns a storage error if the request cannot be read.
pub async fn phase5_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    sqlx::query_as(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase5_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await
}

/// Reads the operation kind, target UID, and fact UID for a correction request key.
///
/// # Errors
///
/// Returns a storage error if the request cannot be read.
pub async fn phase5_correction_request_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<(String, String, String)>, StoreError> {
    sqlx::query_as(
        "SELECT kind, target_uid, fact_uid FROM transfer_phase5_correction_request
         WHERE idempotency_key = ?",
    )
    .bind(request_id.trim())
    .fetch_optional(pool)
    .await
}

enum CompanionRequest {
    Revision,
    Invitation,
}

async fn ensure_request_unused<'e>(
    executor: impl SqliteExecutor<'e>,
    request_id: &str,
    companion: CompanionRequest,
) -> Result<(), StoreError> {
    let used: bool = sqlx::query_scalar(
        "SELECT CASE ?2
                 WHEN 'revision' THEN EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = ?1)
                 ELSE EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = ?1)
               END
             OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = ?1)
             OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = ?1)",
    )
    .bind(request_id)
    .bind(match companion {
        CompanionRequest::Revision => "revision",
        CompanionRequest::Invitation => "invitation",
    })
    .fetch_one(executor)
    .await?;
    if used {
        return Err(sqlx::Error::Protocol(
            "transfer request id was already used by another action".into(),
        ));
    }
    Ok(())
}

pub(crate) async fn ensure_request_not_used_by_transfer_revision(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    ensure_request_unused(pool, request_id, CompanionRequest::Revision).await
}

pub(crate) async fn ensure_request_not_used_by_invitation_event(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<(), StoreError> {
    ensure_request_unused(pool, request_id, CompanionRequest::Invitation).await
}

pub(super) async fn ensure_request_not_used_by_invitation_event_on(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<(), StoreError> {
    ensure_request_unused(connection, request_id, CompanionRequest::Invitation).await
}
