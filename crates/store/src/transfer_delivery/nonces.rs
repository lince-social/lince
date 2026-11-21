//! Replay protection state for Organ requests.

use chrono::{DateTime, Utc};
use nucleus::transfer_delivery::SignedOrganRequestV1;
use sqlx::SqlitePool;

use super::validation::protocol;
use crate::StoreError;

/// Records a nonce once for the sending Organ.
///
/// # Errors
///
/// Returns an error for invalid request shape, a reused nonce, or storage failure.
pub async fn consume_organ_request_nonce(
    pool: &SqlitePool,
    request: &SignedOrganRequestV1,
    now: DateTime<Utc>,
) -> Result<(), StoreError> {
    request.validate_shape().map_err(protocol)?;
    let result = sqlx::query(
        "INSERT INTO organ_transfer_request_nonce
         (sender_organ_uid, nonce, request_hash, method, path, request_timestamp, received_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(sender_organ_uid, nonce) DO NOTHING",
    )
    .bind(&request.sender_organ_uid)
    .bind(&request.nonce)
    .bind(request.request_hash())
    .bind(&request.method)
    .bind(&request.path)
    .bind(&request.timestamp)
    .bind(now.to_rfc3339())
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(protocol("signed Organ request nonce was already consumed"));
    }
    Ok(())
}

/// Deletes nonces received strictly before the cutoff.
///
/// # Errors
///
/// Returns an error if the deletion fails.
pub async fn prune_organ_request_nonces(
    pool: &SqlitePool,
    received_before: DateTime<Utc>,
) -> Result<u64, StoreError> {
    Ok(
        sqlx::query("DELETE FROM organ_transfer_request_nonce WHERE received_at < ?")
            .bind(received_before.to_rfc3339())
            .execute(pool)
            .await?
            .rows_affected(),
    )
}
