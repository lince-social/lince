use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    pub organ_uid: String,
    pub root_key: String,
    pub label: String,
    pub quota_bytes: i64,
    pub registered_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldBundle {
    pub uid: String,
    pub to_organ: String,
    pub from_organ: String,
    pub from_cell: String,
    pub from_node: String,
    pub body: String,
    pub bytes: i64,
    pub received_at: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Waiting {
    pub bundles: i64,
    pub bytes: i64,
    pub oldest_expires_at: Option<String>,
}

fn map_registration(row: sqlx::sqlite::SqliteRow) -> Registration {
    Registration {
        organ_uid: row.get("organ_uid"),
        root_key: row.get("root_key"),
        label: row.get("label"),
        quota_bytes: row.get("quota_bytes"),
        registered_at: row.get("registered_at"),
    }
}

fn map_bundle(row: sqlx::sqlite::SqliteRow) -> HeldBundle {
    HeldBundle {
        uid: row.get("uid"),
        to_organ: row.get("to_organ"),
        from_organ: row.get("from_organ"),
        from_cell: row.get("from_cell"),
        from_node: row.get("from_node"),
        body: row.get("body"),
        bytes: row.get("bytes"),
        received_at: row.get("received_at"),
        expires_at: row.get("expires_at"),
    }
}

pub async fn register(
    pool: &SqlitePool,
    organ_uid: &str,
    root_key: &str,
    label: &str,
    quota_bytes: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_registration
           (organ_uid, root_key, label, quota_bytes, registered_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           label = excluded.label,
           quota_bytes = excluded.quota_bytes",
    )
    .bind(organ_uid)
    .bind(root_key)
    .bind(label)
    .bind(quota_bytes)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn deregister(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM mailbox_registration WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn registration(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<Registration>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_registration WHERE organ_uid = ?")
            .bind(organ_uid)
            .fetch_optional(pool)
            .await?
            .map(map_registration),
    )
}

pub async fn registrations(pool: &SqlitePool) -> Result<Vec<Registration>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_registration ORDER BY label, organ_uid")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map_registration)
            .collect(),
    )
}

pub async fn held_bytes(pool: &SqlitePool, organ_uid: &str) -> Result<i64, StoreError> {
    Ok(
        sqlx::query(
            "SELECT COALESCE(SUM(bytes), 0) AS held FROM mailbox_bundle WHERE to_organ = ?",
        )
        .bind(organ_uid)
        .fetch_one(pool)
        .await?
        .get("held"),
    )
}

pub async fn deposit(pool: &SqlitePool, bundle: &HeldBundle) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_bundle
           (uid, to_organ, from_organ, from_cell, from_node, body, bytes, received_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&bundle.uid)
    .bind(&bundle.to_organ)
    .bind(&bundle.from_organ)
    .bind(&bundle.from_cell)
    .bind(&bundle.from_node)
    .bind(&bundle.body)
    .bind(bundle.bytes)
    .bind(&bundle.received_at)
    .bind(&bundle.expires_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn for_recipient(
    pool: &SqlitePool,
    organ_uid: &str,
    limit: i64,
) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM mailbox_bundle WHERE to_organ = ? ORDER BY received_at, uid LIMIT ?",
    )
    .bind(organ_uid)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

pub async fn collected(
    pool: &SqlitePool,
    organ_uid: &str,
    uids: &[String],
) -> Result<u64, StoreError> {
    let mut dropped = 0;
    for uid in uids {
        dropped += sqlx::query("DELETE FROM mailbox_bundle WHERE uid = ? AND to_organ = ?")
            .bind(uid)
            .bind(organ_uid)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(dropped)
}

pub async fn waiting(pool: &SqlitePool, organ_uid: &str) -> Result<Waiting, StoreError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS bundles,
                COALESCE(SUM(bytes), 0) AS bytes,
                MIN(expires_at) AS oldest
           FROM mailbox_bundle WHERE to_organ = ?",
    )
    .bind(organ_uid)
    .fetch_one(pool)
    .await?;
    Ok(Waiting {
        bundles: row.get("bundles"),
        bytes: row.get("bytes"),
        oldest_expires_at: row.get("oldest"),
    })
}

pub async fn sweep_expired(pool: &SqlitePool) -> Result<u64, StoreError> {
    let now = Utc::now().to_rfc3339();
    let mut tx = crate::write_tx(pool).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO mailbox_expiry_notice
           (uid, to_organ, from_organ, from_cell, from_node, bytes, received_at,
            expired_at, notified_at)
         SELECT uid, to_organ, from_organ, from_cell, from_node, bytes, received_at, ?, NULL
           FROM mailbox_bundle WHERE expires_at <= ?",
    )
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let swept = sqlx::query("DELETE FROM mailbox_bundle WHERE expires_at <= ?")
        .bind(&now)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok(swept)
}

pub async fn backdate_expiry(pool: &SqlitePool, uid: &str, when: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE mailbox_bundle SET expires_at = ? WHERE uid = ?")
        .bind(when)
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn pending_notices(pool: &SqlitePool) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, to_organ, from_organ, from_cell, from_node, '' AS body, bytes,
                received_at, expired_at AS expires_at
           FROM mailbox_expiry_notice WHERE notified_at IS NULL ORDER BY expired_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

pub async fn expiries_for_node(
    pool: &SqlitePool,
    from_node: &str,
) -> Result<Vec<HeldBundle>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, to_organ, from_organ, from_cell, from_node, '' AS body, bytes,
                received_at, expired_at AS expires_at
           FROM mailbox_expiry_notice WHERE from_node = ? ORDER BY expired_at",
    )
    .bind(from_node)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_bundle)
    .collect())
}

pub async fn notices_handed(pool: &SqlitePool, from_node: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE mailbox_expiry_notice SET notified_at = ?
          WHERE from_node = ? AND notified_at IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(from_node)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn notices_heard(
    pool: &SqlitePool,
    from_node: &str,
    uids: &[String],
) -> Result<u64, StoreError> {
    let mut dropped = 0;
    for uid in uids {
        dropped += sqlx::query("DELETE FROM mailbox_expiry_notice WHERE uid = ? AND from_node = ?")
            .bind(uid)
            .bind(from_node)
            .execute(pool)
            .await?
            .rows_affected();
    }
    Ok(dropped)
}

pub async fn carried_for(pool: &SqlitePool, organ_uid: &str) -> Result<Waiting, StoreError> {
    waiting(pool, organ_uid).await
}

#[derive(Debug, Clone)]
pub struct CarryRequest {
    pub organ_uid: String,
    pub root_key: String,
    pub label: String,
    pub asked_at: String,
}

pub async fn ask_to_be_carried(
    pool: &SqlitePool,
    organ_uid: &str,
    root_key: &str,
    label: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO mailbox_request (organ_uid, root_key, label, asked_at)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET
           root_key = excluded.root_key,
           label = excluded.label,
           asked_at = excluded.asked_at",
    )
    .bind(organ_uid)
    .bind(root_key)
    .bind(label)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn requests(pool: &SqlitePool) -> Result<Vec<CarryRequest>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_request ORDER BY asked_at")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| CarryRequest {
                organ_uid: r.get("organ_uid"),
                root_key: r.get("root_key"),
                label: r.get("label"),
                asked_at: r.get("asked_at"),
            })
            .collect(),
    )
}

pub async fn request(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<CarryRequest>, StoreError> {
    Ok(requests(pool)
        .await?
        .into_iter()
        .find(|row| row.organ_uid == organ_uid))
}

pub async fn answer_request(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM mailbox_request WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct MailboxInvite {
    pub label: String,
    pub quota_bytes: i64,
    pub expires_at: String,
    pub created_at: String,
    pub used_at: Option<String>,
    pub used_by: Option<String>,
}

pub async fn put_invite(
    pool: &SqlitePool,
    token_hash: &str,
    label: &str,
    quota_bytes: i64,
    expires_at: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT OR REPLACE INTO mailbox_invite
           (token_hash, label, quota_bytes, expires_at, created_at, used_at, used_by)
         VALUES (?, ?, ?, ?, ?, NULL, NULL)",
    )
    .bind(token_hash)
    .bind(label)
    .bind(quota_bytes)
    .bind(expires_at)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn redeem_invite(
    pool: &SqlitePool,
    token_hash: &str,
    organ_uid: &str,
) -> Result<Option<(String, i64)>, StoreError> {
    let now = Utc::now().to_rfc3339();
    let claimed = sqlx::query(
        "UPDATE mailbox_invite SET used_at = ?, used_by = ?
          WHERE token_hash = ? AND used_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(organ_uid)
    .bind(token_hash)
    .bind(&now)
    .execute(pool)
    .await?
    .rows_affected();
    if claimed == 0 {
        return Ok(None);
    }
    let row = sqlx::query("SELECT label, quota_bytes FROM mailbox_invite WHERE token_hash = ?")
        .bind(token_hash)
        .fetch_one(pool)
        .await?;
    Ok(Some((row.get("label"), row.get("quota_bytes"))))
}

pub async fn invites(pool: &SqlitePool) -> Result<Vec<MailboxInvite>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM mailbox_invite ORDER BY created_at DESC LIMIT 50")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| MailboxInvite {
                label: r.get("label"),
                quota_bytes: r.get("quota_bytes"),
                expires_at: r.get("expires_at"),
                created_at: r.get("created_at"),
                used_at: r.get("used_at"),
                used_by: r.get("used_by"),
            })
            .collect(),
    )
}
