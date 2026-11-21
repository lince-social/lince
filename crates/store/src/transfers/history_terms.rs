use nucleus::transfer::public_history::{NAMESPACE, Terms};

use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::StoreError;

/// Reads the public-history terms, when present.
///
/// # Errors
///
/// Returns an error if storage fails or the terms are malformed.
pub async fn read(pool: &SqlitePool, transfer: &str) -> Result<Option<Terms>, StoreError> {
    let mut tx = pool.begin().await?;
    read_on(&mut tx, transfer).await
}

pub(super) async fn read_on(
    tx: &mut Transaction<'_, Sqlite>,
    transfer: &str,
) -> Result<Option<Terms>, StoreError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?")
            .bind(transfer)
            .bind(NAMESPACE)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(raw) = raw else { return Ok(None) };
    let mut value: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let terms: Option<Terms> = serde_json::from_value(
        value
            .get_mut("terms")
            .map(std::mem::take)
            .unwrap_or_default(),
    )
    .map_err(|error| StoreError::Protocol(error.to_string()))?;
    if let Some(terms) = &terms {
        terms.validate().map_err(StoreError::Protocol)?;
    }
    Ok(terms)
}
