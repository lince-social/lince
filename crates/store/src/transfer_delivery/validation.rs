//! Validation shared by delivery policies and application handoffs.

use sqlx::SqliteConnection;

use crate::StoreError;

pub(super) fn protocol(message: impl Into<String>) -> StoreError {
    StoreError::Protocol(message.into())
}

pub(super) fn required(label: &str, value: &str) -> Result<(), StoreError> {
    if value.trim().is_empty() || value.len() > 1_024 {
        return Err(protocol(format!("{label} has an invalid length")));
    }
    Ok(())
}

pub(super) async fn ensure_request_unused(
    connection: &mut SqliteConnection,
    request_id: &str,
) -> Result<(), StoreError> {
    let used: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = ?1)
          OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = ?1)",
    )
    .bind(request_id)
    .fetch_one(connection)
    .await?;
    if used {
        return Err(protocol("Transfer request id belongs to another workflow"));
    }
    Ok(())
}
