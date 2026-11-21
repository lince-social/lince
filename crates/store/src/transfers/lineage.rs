//! Lineage from corrections and replacement Promises.

use sqlx::{Row, SqlitePool, sqlite::SqliteRow};

use crate::StoreError;

/// Source and quantity for a correction Transfer.
#[derive(Debug, Clone)]
pub struct NewTransferCorrectionLink {
    pub kind: String,
    pub source_transfer_uid: String,
    pub source_occurrence_uid: String,
    pub source_revision: u64,
    pub canonical_quantity: f64,
}

/// A signed link between an occurrence and its correction Transfer.
#[derive(Debug, Clone)]
pub struct TransferCorrectionLinkRow {
    pub uid: String,
    pub kind: String,
    pub source_transfer_uid: String,
    pub source_occurrence_uid: String,
    pub created_transfer_uid: String,
    pub source_revision: u64,
    pub canonical_quantity: f64,
    pub actor_person_uid: String,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

/// The Promise to replace and its intended successor.
#[derive(Debug, Clone)]
pub struct PromiseSuccessorInput {
    pub predecessor_promise_uid: String,
    pub successor_promise_uid: String,
}

/// The stored replacement link and the evidence that established it.
#[derive(Debug, Clone)]
pub struct PromiseSuccessorRow {
    pub uid: String,
    pub transfer_uid: String,
    pub predecessor_promise_uid: String,
    pub successor_promise_uid: String,
    pub revision: u64,
    pub actor_person_uid: String,
    pub fact_uid: String,
    pub idempotency_key: String,
    pub created_at: String,
}

fn map_transfer_correction_link(row: SqliteRow) -> TransferCorrectionLinkRow {
    TransferCorrectionLinkRow {
        uid: row.get("uid"),
        kind: row.get("kind"),
        source_transfer_uid: row.get("source_transfer_uid"),
        source_occurrence_uid: row.get("source_occurrence_uid"),
        created_transfer_uid: row.get("created_transfer_uid"),
        source_revision: row.get::<i64, _>("source_revision").cast_unsigned(),
        canonical_quantity: row.get("canonical_quantity"),
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Looks up the correction link created by a request.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn correction_link_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<TransferCorrectionLinkRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_correction_link WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .map(map_transfer_correction_link)
        .fetch_optional(pool)
        .await
}

/// Lists correction links where the Transfer is either the source or the correction.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn correction_links_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferCorrectionLinkRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_correction_link
         WHERE source_transfer_uid = ?1 OR created_transfer_uid = ?1
         ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .map(map_transfer_correction_link)
    .fetch_all(pool)
    .await
}

fn map_promise_successor(row: SqliteRow) -> PromiseSuccessorRow {
    PromiseSuccessorRow {
        uid: row.get("uid"),
        transfer_uid: row.get("transfer_uid"),
        predecessor_promise_uid: row.get("predecessor_promise_uid"),
        successor_promise_uid: row.get("successor_promise_uid"),
        revision: row.get::<i64, _>("revision").cast_unsigned(),
        actor_person_uid: row.get("actor_person_uid"),
        fact_uid: row.get("fact_uid"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

/// Looks up the Promise successor created by a request.
///
/// # Errors
///
/// Returns an error if the lookup fails.
pub async fn promise_successor_for_request(
    pool: &SqlitePool,
    request_id: &str,
) -> Result<Option<PromiseSuccessorRow>, StoreError> {
    sqlx::query("SELECT * FROM transfer_promise_successor WHERE idempotency_key = ?")
        .bind(request_id.trim())
        .map(map_promise_successor)
        .fetch_optional(pool)
        .await
}

/// Lists the Transfer's Promise successors in creation order.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn promise_successors_for_transfer(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<PromiseSuccessorRow>, StoreError> {
    sqlx::query(
        "SELECT * FROM transfer_promise_successor
         WHERE transfer_uid = ? ORDER BY created_at, uid",
    )
    .bind(transfer_uid)
    .map(map_promise_successor)
    .fetch_all(pool)
    .await
}
