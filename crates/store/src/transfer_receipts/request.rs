//! Serialized request identities and replay for receipt commits.

use nucleus::transfer::receipt::Command;
use sqlx::SqliteConnection;

use super::invalid;
use crate::StoreError;

pub(super) fn request_json(person: &str, command: &Command) -> Result<String, StoreError> {
    let value = serde_json::to_string(&(person, command)).map_err(invalid)?;
    if value.len() > 850_000 {
        return Err(invalid("Receipt request exceeds its byte budget"));
    }
    Ok(value)
}

pub(super) async fn lookup(
    connection: &mut SqliteConnection,
    id: &str,
) -> Result<Option<(String, String, i64)>, StoreError> {
    sqlx::query_as(
        "SELECT request_json,record_uid,version FROM transfer_receipt_request WHERE request_id=?",
    )
    .bind(id)
    .fetch_optional(connection)
    .await
}

pub(super) async fn record_request(
    connection: &mut SqliteConnection,
    id: &str,
    person: &str,
    request: &str,
    uid: &str,
    version: u64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO transfer_receipt_request
         (request_id,person_uid,request_json,record_uid,version) VALUES(?,?,?,?,?)",
    )
    .bind(id)
    .bind(person)
    .bind(request)
    .bind(uid)
    .bind(i64::try_from(version).map_err(invalid)?)
    .execute(connection)
    .await?;
    Ok(())
}
