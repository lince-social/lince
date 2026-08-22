use crate::StoreError;
use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

async fn flagged(
    pool: &SqlitePool,
    contact_organ: &str,
    flag: &str,
) -> Result<HashSet<String>, StoreError> {
    let sql =
        format!("SELECT record_uid FROM contact_share WHERE contact_organ = ? AND {flag} = 1");
    Ok(sqlx::query(&sql)
        .bind(contact_organ)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("record_uid"))
        .collect())
}

pub async fn held(pool: &SqlitePool, contact_organ: &str) -> Result<HashSet<String>, StoreError> {
    flagged(pool, contact_organ, "held").await
}

pub async fn picked(pool: &SqlitePool, contact_organ: &str) -> Result<HashSet<String>, StoreError> {
    flagged(pool, contact_organ, "picked").await
}

pub async fn set_picked(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
    picked: bool,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO contact_share (contact_organ, record_uid, picked, held, added_at)
         VALUES (?, ?, ?, 0, ?)
         ON CONFLICT (contact_organ, record_uid) DO UPDATE SET picked = excluded.picked",
    )
    .bind(contact_organ)
    .bind(record_uid)
    .bind(picked as i64)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_held(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO contact_share (contact_organ, record_uid, picked, held, added_at)
         VALUES (?, ?, 0, 1, ?)
         ON CONFLICT (contact_organ, record_uid) DO UPDATE SET held = 1",
    )
    .bind(contact_organ)
    .bind(record_uid)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn forget(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM contact_share WHERE contact_organ = ? AND record_uid = ?")
        .bind(contact_organ)
        .bind(record_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_picked_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    contact_organ: &str,
    record_uid: &str,
    picked: bool,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO contact_share (contact_organ, record_uid, picked, held, added_at)
         VALUES (?, ?, ?, 0, ?)
         ON CONFLICT (contact_organ, record_uid) DO UPDATE SET picked = excluded.picked",
    )
    .bind(contact_organ)
    .bind(record_uid)
    .bind(picked as i64)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn set_watermark_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    contact_organ: &str,
    seq: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET share_seen_seq = ? WHERE record_uid = ?")
        .bind(seq)
        .bind(contact_organ)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn forget_watermark(pool: &SqlitePool, contact_organ: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET share_seen_seq = NULL WHERE record_uid = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}
