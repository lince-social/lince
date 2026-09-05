use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone)]
pub struct StoredRoster {
    pub organ_uid: String,
    pub root_key: String,
    pub version: i64,
    pub not_after: String,
    pub payload: String,
    pub signature: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> StoredRoster {
    StoredRoster {
        organ_uid: row.get("organ_uid"),
        root_key: row.get("root_key"),
        version: row.get("version"),
        not_after: row.get("not_after"),
        payload: row.get("payload"),
        signature: row.get("signature"),
    }
}

pub async fn get(pool: &SqlitePool, organ_uid: &str) -> Result<Option<StoredRoster>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM organ_roster WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .map(map),
    )
}

pub async fn put(pool: &SqlitePool, roster: &StoredRoster) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_roster
           (organ_uid, root_key, version, not_after, payload, signature, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           version = excluded.version,
           not_after = excluded.not_after,
           payload = excluded.payload,
           signature = excluded.signature,
           updated_at = excluded.updated_at
         WHERE excluded.version > organ_roster.version",
    )
    .bind(&roster.organ_uid)
    .bind(&roster.root_key)
    .bind(roster.version)
    .bind(&roster.not_after)
    .bind(&roster.payload)
    .bind(&roster.signature)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn organ_holding_cell(
    pool: &SqlitePool,
    cell_uid: &str,
) -> Result<Option<String>, StoreError> {
    let rows = sqlx::query("SELECT organ_uid, payload FROM organ_roster")
        .fetch_all(pool)
        .await?;
    for row in rows {
        let payload: String = row.get("payload");
        let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&payload) else {
            continue;
        };
        let named = parsed
            .get("cells")
            .and_then(|cells| cells.as_array())
            .is_some_and(|cells| {
                cells
                    .iter()
                    .any(|cell| cell.get("cell_uid").and_then(|uid| uid.as_str()) == Some(cell_uid))
            });
        if named {
            return Ok(Some(row.get("organ_uid")));
        }
    }
    Ok(None)
}

pub async fn project_local_capabilities(
    pool: &SqlitePool,
    capabilities: &[String],
) -> Result<(), StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query("DELETE FROM local_capability")
        .execute(&mut *tx)
        .await?;
    for capability in capabilities {
        sqlx::query("INSERT OR IGNORE INTO local_capability (capability) VALUES (?)")
            .bind(capability)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn put_public_packet(
    pool: &SqlitePool,
    organ_uid: &str,
    packet: &[u8],
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_public_record (organ_uid, packet, updated_at)
         VALUES (?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           packet = excluded.packet,
           updated_at = excluded.updated_at",
    )
    .bind(organ_uid)
    .bind(packet)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn public_packet(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<Vec<u8>>, StoreError> {
    Ok(
        sqlx::query_scalar("SELECT packet FROM organ_public_record WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn clear_public_packet(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM organ_public_record WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn put_enrolment_token(
    pool: &SqlitePool,
    token_hash: &str,
    expires_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR REPLACE INTO enrolment_token (token_hash, expires_at, used_at, created_at)
         VALUES (?, ?, NULL, ?)",
    )
    .bind(token_hash)
    .bind(expires_at)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn redeem_enrolment_token(
    pool: &SqlitePool,
    token_hash: &str,
) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE enrolment_token SET used_at = ?
          WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(token_hash)
    .bind(&now)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected > 0)
}

pub async fn enrolment_is_open(pool: &SqlitePool) -> Result<bool, StoreError> {
    let now = Utc::now().to_rfc3339();
    let outstanding: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM enrolment_token WHERE used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .fetch_one(pool)
    .await?;
    Ok(outstanding > 0)
}

pub async fn record_succession(
    pool: &SqlitePool,
    organ_uid: &str,
    old_key: &str,
    new_key: &str,
    signature: &str,
    created_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO identity_succession
           (organ_uid, old_key, new_key, signature, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(organ_uid)
    .bind(old_key)
    .bind(new_key)
    .bind(signature)
    .bind(created_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn successions(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(
        sqlx::query("SELECT old_key, new_key FROM identity_succession WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get("old_key"), row.get("new_key")))
            .collect(),
    )
}

pub struct SuccessionRow {
    pub old_key: String,
    pub new_key: String,
    pub signature: String,
    pub created_at: String,
}

pub async fn published_successions(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<SuccessionRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT old_key, new_key, signature, created_at
           FROM identity_succession WHERE organ_uid = ? ORDER BY created_at",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| SuccessionRow {
        old_key: row.get("old_key"),
        new_key: row.get("new_key"),
        signature: row.get("signature"),
        created_at: row.get("created_at"),
    })
    .collect())
}

pub async fn record_revocation(
    pool: &SqlitePool,
    organ_uid: &str,
    revoked_key: &str,
    signature: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR IGNORE INTO identity_revocation
           (organ_uid, revoked_key, signature, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(organ_uid)
    .bind(revoked_key)
    .bind(signature)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn is_revoked(pool: &SqlitePool, organ_uid: &str, key: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM identity_revocation WHERE organ_uid = ? AND revoked_key = ?",
    )
    .bind(organ_uid)
    .bind(key)
    .fetch_one(pool)
    .await?
        > 0)
}

pub async fn revocations_of(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<(String, String)>, StoreError> {
    Ok(
        sqlx::query("SELECT revoked_key, signature FROM identity_revocation WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|row| (row.get("revoked_key"), row.get("signature")))
            .collect(),
    )
}
