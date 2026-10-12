use serde::{Deserialize, Serialize};
use sqlx::{Sqlite, SqliteConnection, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Endpoint {
    pub uid: String,
    pub name: String,
    pub public_key: String,
    pub signal_uid: String,
    pub enabled: bool,
    pub revision: i64,
    pub credential_epoch: i64,
    pub cooldown_seconds: i64,
    pub minimum: String,
    pub maximum: String,
    pub expires_at: i64,
    pub last_sequence: i64,
    pub last_fingerprint: Option<String>,
    pub last_fact_uid: Option<String>,
    pub last_received_at: Option<i64>,
    pub created_at: i64,
}

pub(crate) async fn ensure_authority(pool: &SqlitePool) -> Result<(), StoreError> {
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM signal_endpoint_authority WHERE id = 1)")
            .fetch_one(pool)
            .await?;
    if !exists {
        let audience = format!(
            "{}{}",
            nucleus::execution::uuid().simple(),
            nucleus::execution::uuid().simple()
        );
        sqlx::query("INSERT OR IGNORE INTO signal_endpoint_authority (id, audience) VALUES (1, ?)")
            .bind(audience)
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn token_endpoint(
    pool: &SqlitePool,
    digest: &str,
) -> Result<Option<Endpoint>, StoreError> {
    sqlx::query_as("SELECT e.* FROM signal_endpoint e JOIN signal_endpoint_token t ON t.endpoint_uid = e.uid WHERE t.digest = ?")
        .bind(digest).fetch_optional(pool).await
}

pub async fn token_matches_on(
    connection: &mut SqliteConnection,
    endpoint: &str,
    digest: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM signal_endpoint_token WHERE endpoint_uid = ? AND digest = ?)",
    )
    .bind(endpoint)
    .bind(digest)
    .fetch_one(connection)
    .await
}

pub async fn save_token_on(
    connection: &mut SqliteConnection,
    endpoint: &str,
    digest: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO signal_endpoint_token (endpoint_uid, digest) VALUES (?, ?) ON CONFLICT(endpoint_uid) DO UPDATE SET digest = excluded.digest")
        .bind(endpoint).bind(digest).execute(connection).await?;
    Ok(())
}

pub async fn revoke_token_on(
    connection: &mut SqliteConnection,
    endpoint: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM signal_endpoint_token WHERE endpoint_uid = ?")
        .bind(endpoint)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn audience_on(connection: &mut SqliteConnection) -> Result<String, StoreError> {
    sqlx::query_scalar("SELECT audience FROM signal_endpoint_authority WHERE id = 1")
        .fetch_one(connection)
        .await
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<Option<Endpoint>, StoreError> {
    sqlx::query_as("SELECT * FROM signal_endpoint WHERE uid = ?")
        .bind(uid)
        .fetch_optional(connection)
        .await
}

pub async fn list_on(connection: &mut SqliteConnection) -> Result<Vec<Endpoint>, StoreError> {
    sqlx::query_as("SELECT * FROM signal_endpoint ORDER BY name, uid LIMIT 257")
        .fetch_all(connection)
        .await
}

pub async fn save_on(
    tx: &mut Transaction<'_, Sqlite>,
    endpoint: &Endpoint,
    actor: Option<&str>,
    now: i64,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO signal_endpoint (uid, name, public_key, signal_uid, enabled, revision, credential_epoch, cooldown_seconds, minimum, maximum, expires_at, last_sequence, last_fingerprint, last_fact_uid, last_received_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(uid) DO UPDATE SET name=excluded.name, public_key=excluded.public_key, enabled=excluded.enabled, revision=excluded.revision, credential_epoch=excluded.credential_epoch, cooldown_seconds=excluded.cooldown_seconds, minimum=excluded.minimum, maximum=excluded.maximum, expires_at=excluded.expires_at, last_sequence=excluded.last_sequence, last_fingerprint=excluded.last_fingerprint, last_fact_uid=excluded.last_fact_uid, last_received_at=excluded.last_received_at")
        .bind(&endpoint.uid).bind(&endpoint.name).bind(&endpoint.public_key).bind(&endpoint.signal_uid)
        .bind(endpoint.enabled).bind(endpoint.revision).bind(endpoint.credential_epoch).bind(endpoint.cooldown_seconds)
        .bind(&endpoint.minimum).bind(&endpoint.maximum).bind(endpoint.expires_at).bind(endpoint.last_sequence)
        .bind(&endpoint.last_fingerprint).bind(&endpoint.last_fact_uid).bind(endpoint.last_received_at)
        .bind(endpoint.created_at).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO signal_endpoint_audit (endpoint_uid, revision, actor_uid, at, configuration) VALUES (?, ?, ?, ?, ?)")
        .bind(&endpoint.uid).bind(endpoint.revision).bind(actor).bind(now)
        .bind(serde_json::to_string(endpoint).map_err(|error| StoreError::Protocol(error.to_string()))?)
        .execute(&mut **tx).await?;
    Ok(())
}

pub async fn accept_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    sequence: i64,
    fingerprint: &str,
    fact: &str,
    now: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE signal_endpoint SET last_sequence = ?, last_fingerprint = ?, last_fact_uid = ?, last_received_at = ? WHERE uid = ?")
        .bind(sequence).bind(fingerprint).bind(fact).bind(now).bind(uid)
        .execute(&mut **tx).await?;
    Ok(())
}

pub async fn signal_available_on(
    connection: &mut SqliteConnection,
    signal: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record r JOIN record o ON o.uid = r.organ_uid WHERE r.uid = ? AND r.kind = 'signal' AND r.deleted_at IS NULL AND r.replica_root IS NULL AND o.slug = 'local-organ' AND o.kind = 'organ' AND o.deleted_at IS NULL)")
        .bind(signal).fetch_one(connection).await
}
