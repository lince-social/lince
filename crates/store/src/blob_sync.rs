// non-lean-yet
// Scope: struct Transfer, fn decode, fn list, fn get.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::needless_pass_by_value] Borrow eligible inputs after checking caller and
//   framework contracts. Sites 21.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 32, 81.
// - [Clippy: clippy::cast_sign_loss] Prove the sign or preserve behavior with an explicit
//   conversion. Sites 32.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 38, 43, 53, 62, 74, 85, 96, 105, 111.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 7, 8,
//   9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 38, 43, 53, 62, 74, 85, 96, 105, 111.

use nucleus::blob_sync::Manifest;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

use crate::{StoreError, sqlx};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Transfer {
    pub id: String,
    pub direction: String,
    pub peer: String,
    pub peer_organ: Option<String>,
    pub label: String,
    pub manifest: Manifest,
    pub state: String,
    pub destination: Option<String>,
    pub progress: u64,
    pub error: String,
    pub settled: bool,
}

fn decode(row: sqlx::sqlite::SqliteRow) -> Result<Transfer, StoreError> {
    Ok(Transfer {
        id: row.get("id"),
        direction: row.get("direction"),
        peer: row.get("peer"),
        peer_organ: row.get("peer_organ"),
        label: row.get("label"),
        manifest: serde_json::from_str(row.get("manifest"))
            .map_err(|error| StoreError::Decode(Box::new(error)))?,
        state: row.get("state"),
        destination: row.get("destination"),
        progress: row.get::<i64, _>("progress") as u64,
        error: row.get("error"),
        settled: row.get("settled"),
    })
}

pub async fn list(pool: &SqlitePool, owner: &str) -> Result<Vec<Transfer>, StoreError> {
    sqlx::query("SELECT * FROM blob_sync WHERE owner = ? ORDER BY state IN ('offered', 'accepted') DESC, settled, created_at DESC, id LIMIT 384")
        .bind(owner).fetch_all(pool).await?.into_iter().map(decode).collect()
}

pub async fn get(pool: &SqlitePool, owner: &str, id: &str) -> Result<Option<Transfer>, StoreError> {
    sqlx::query("SELECT * FROM blob_sync WHERE owner = ? AND id = ?")
        .bind(owner)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(decode)
        .transpose()
}

pub async fn insert(pool: &SqlitePool, owner: &str, transfer: &Transfer) -> Result<(), StoreError> {
    let manifest = serde_json::to_string(&transfer.manifest)
        .map_err(|error| StoreError::Decode(Box::new(error)))?;
    sqlx::query("INSERT INTO blob_sync (id, owner, direction, peer, label, manifest, state, peer_organ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&transfer.id).bind(owner).bind(&transfer.direction).bind(&transfer.peer)
        .bind(&transfer.label).bind(manifest).bind(&transfer.state).bind(&transfer.peer_organ).execute(pool).await?;
    Ok(())
}

pub async fn transition(
    pool: &SqlitePool,
    owner: &str,
    id: &str,
    from: &str,
    to: &str,
    destination: Option<&str>,
) -> Result<bool, StoreError> {
    Ok(sqlx::query("UPDATE blob_sync SET state = ?, destination = COALESCE(?, destination), error = '', settled = 0 WHERE owner = ? AND id = ? AND state = ?")
        .bind(to).bind(destination).bind(owner).bind(id).bind(from).execute(pool).await?.rows_affected() == 1)
}

pub async fn progress(
    pool: &SqlitePool,
    owner: &str,
    id: &str,
    bytes: u64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE blob_sync SET progress = ?, error = '' WHERE owner = ? AND id = ? AND state = 'accepted'")
        .bind(bytes as i64).bind(owner).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn error(
    pool: &SqlitePool,
    owner: &str,
    id: &str,
    message: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE blob_sync SET error = ? WHERE owner = ? AND id = ? AND state IN ('offered', 'accepted')")
        .bind(message.chars().take(512).collect::<String>()).bind(owner).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn settle(pool: &SqlitePool, owner: &str, id: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE blob_sync SET settled = 1 WHERE owner = ? AND id = ?")
        .bind(owner)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn offered(pool: &SqlitePool, owner: &str, id: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE blob_sync SET settled = 1, error = '' WHERE owner = ? AND id = ? AND state = 'offered' AND direction = 'outgoing'")
        .bind(owner).bind(id).execute(pool).await?;
    Ok(())
}

pub async fn authorized(
    pool: &SqlitePool,
    owner: &str,
    peer: &str,
    hash: &str,
) -> Result<Vec<Option<String>>, StoreError> {
    sqlx::query_scalar("SELECT DISTINCT t.peer_organ FROM blob_sync AS t, json_each(t.manifest, '$.entries') AS e WHERE t.owner = ? AND t.peer = ? AND t.direction = 'outgoing' AND t.state = 'accepted' AND json_extract(e.value, '$.hash') = ?")
        .bind(owner).bind(peer).bind(hash).fetch_all(pool).await
}
