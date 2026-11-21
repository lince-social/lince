//! Local occurrence mappings for reviewed receipt scopes.

use nucleus::{
    performance::Span,
    transfer::receipt::{Document, Scope, ScopeRequest, Source},
};
use sqlx::{SqliteConnection, SqlitePool};

use super::invalid;
use crate::{StoreError, organs::LOCAL_ORGAN_SLUG};

pub async fn local_source(
    pool: &SqlitePool,
    uid: &str,
) -> Result<Option<ScopeRequest>, StoreError> {
    let value: Option<String> = sqlx::query_scalar(
        "SELECT scope_request_json FROM transfer_receipt_local_source WHERE record_uid=?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(invalid))
        .transpose()
}

pub async fn source_for_scope(
    pool: &SqlitePool,
    document: &Document,
) -> Result<Option<ScopeRequest>, StoreError> {
    let scope = document
        .scope()
        .ok_or_else(|| invalid("Receipt scope is unavailable"))?;
    source_for_reviewed_scope(pool, scope).await
}

pub async fn source_for_reviewed_scope(
    pool: &SqlitePool,
    scope: &Scope,
) -> Result<Option<ScopeRequest>, StoreError> {
    let identity = scope.id().map_err(invalid)?;
    let _span = Span::new("receipt-identity-read");
    let value: Option<String> = sqlx::query_scalar(
        "SELECT s.scope_request_json FROM transfer_receipt_local_source s
         JOIN transfer_receipt_record r ON r.record_uid=s.record_uid
         WHERE r.kind='review' AND r.identity_hash=? ORDER BY r.rowid LIMIT 1",
    )
    .bind(identity.as_str())
    .fetch_optional(pool)
    .await?;
    value
        .map(|value| serde_json::from_str(&value).map_err(invalid))
        .transpose()
}

pub(super) async fn record_on(
    connection: &mut SqliteConnection,
    uid: &str,
    document: &Document,
    source: &ScopeRequest,
) -> Result<(), StoreError> {
    let scope = document
        .scope()
        .ok_or_else(|| invalid("Authority certificates have no private occurrence mapping"))?;
    let origin: Option<String> = sqlx::query_scalar(
        "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
    )
    .bind(LOCAL_ORGAN_SLUG)
    .fetch_optional(&mut *connection)
    .await?;
    if origin.as_deref() != Some(scope.source.origin.as_str())
        || Source::occurrence_id(&scope.source.origin, &source.occurrence).map_err(invalid)?
            != scope.source.occurrence
    {
        return Err(invalid("Private receipt mapping does not match its source"));
    }
    let current: Option<i64> = sqlx::query_scalar(
        "SELECT t.revision FROM transfer_occurrence o
         JOIN transfer t ON t.record_uid=o.transfer_uid WHERE o.uid=?",
    )
    .bind(&source.occurrence)
    .fetch_optional(&mut *connection)
    .await?;
    if current.and_then(|value| u64::try_from(value).ok()) != Some(source.expected_revision) {
        return Err(invalid("Source Transfer changed after receipt review"));
    }
    let value = serde_json::to_string(source).map_err(invalid)?;
    if value.len() > 32768 {
        return Err(invalid("Private receipt mapping exceeds its budget"));
    }
    sqlx::query(
        "INSERT INTO transfer_receipt_local_source(record_uid,occurrence_uid,scope_request_json)
         VALUES(?,?,?) ON CONFLICT(record_uid) DO NOTHING",
    )
    .bind(uid)
    .bind(&source.occurrence)
    .bind(value)
    .execute(connection)
    .await?;
    Ok(())
}
