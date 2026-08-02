//! Named vocabularies. A Lingua groups Concepts without owning local Record
//! structure; the same Concept may be adopted by several Linguas.

use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const LOCAL_UID: &str = "g_local";

pub async fn ensure_local(pool: &SqlitePool) -> Result<String, StoreError> {
    sqlx::query(
        "INSERT INTO lingua (uid, name, visibility, created_at)
         VALUES (?, 'Local', 'private', ?)
         ON CONFLICT(uid) DO NOTHING",
    )
    .bind(LOCAL_UID)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(LOCAL_UID.to_string())
}

pub async fn resolve(pool: &SqlitePool, token: &str) -> Result<Option<String>, StoreError> {
    Ok(
        sqlx::query("SELECT uid FROM lingua WHERE uid = ? OR name = ? LIMIT 1")
            .bind(token)
            .bind(token)
            .fetch_optional(pool)
            .await?
            .map(|row| row.get("uid")),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinguaRow {
    pub uid: String,
    pub name: String,
    pub owner_organ: Option<String>,
    pub visibility: String,
    pub created_at: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> LinguaRow {
    LinguaRow {
        uid: row.get("uid"),
        name: row.get("name"),
        owner_organ: row.get("owner_organ"),
        visibility: row.get("visibility"),
        created_at: row.get("created_at"),
    }
}

pub async fn create(
    pool: &SqlitePool,
    name: &str,
    owner_organ: Option<&str>,
    visibility: &str,
) -> Result<String, StoreError> {
    if !matches!(visibility, "private" | "shared" | "public") {
        return Err(sqlx::Error::Protocol(format!(
            "invalid Lingua visibility `{visibility}`"
        )));
    }
    let uid = nucleus::new_uid("g");
    sqlx::query(
        "INSERT INTO lingua (uid, name, owner_organ, visibility, created_at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&uid)
    .bind(name)
    .bind(owner_organ)
    .bind(visibility)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(uid)
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<LinguaRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, name, owner_organ, visibility, created_at
           FROM lingua ORDER BY name, uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

pub async fn get(pool: &SqlitePool, uid: &str) -> Result<Option<LinguaRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, name, owner_organ, visibility, created_at FROM lingua WHERE uid = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?
    .map(map))
}

pub async fn rename(pool: &SqlitePool, uid: &str, name: &str) -> Result<bool, StoreError> {
    Ok(sqlx::query("UPDATE lingua SET name = ? WHERE uid = ?")
        .bind(name)
        .bind(uid)
        .execute(pool)
        .await?
        .rows_affected()
        > 0)
}

pub async fn delete(pool: &SqlitePool, uid: &str) -> Result<bool, StoreError> {
    let mut transaction = pool.begin().await?;
    sqlx::query("DELETE FROM lingua_concept WHERE lingua_uid = ?")
        .bind(uid)
        .execute(&mut *transaction)
        .await?;
    let deleted = sqlx::query("DELETE FROM lingua WHERE uid = ?")
        .bind(uid)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
        > 0;
    transaction.commit().await?;
    Ok(deleted)
}

pub async fn adopt(
    pool: &SqlitePool,
    lingua_uid: &str,
    concept_uid: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO lingua_concept (lingua_uid, concept_uid, adopted_at)
         VALUES (?, ?, ?) ON CONFLICT(lingua_uid, concept_uid) DO NOTHING",
    )
    .bind(lingua_uid)
    .bind(concept_uid)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_concept(
    pool: &SqlitePool,
    lingua_uid: &str,
    concept_uid: &str,
) -> Result<bool, StoreError> {
    Ok(
        sqlx::query("DELETE FROM lingua_concept WHERE lingua_uid = ? AND concept_uid = ?")
            .bind(lingua_uid)
            .bind(concept_uid)
            .execute(pool)
            .await?
            .rows_affected()
            > 0,
    )
}

pub async fn concept_uids(pool: &SqlitePool, lingua_uid: &str) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT concept_uid FROM lingua_concept WHERE lingua_uid = ? ORDER BY adopted_at, concept_uid",
    )
    .bind(lingua_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("concept_uid"))
    .collect())
}

pub async fn lingua_uids_for_concept(
    pool: &SqlitePool,
    concept_uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT lingua_uid FROM lingua_concept WHERE concept_uid = ? ORDER BY adopted_at, lingua_uid",
    )
    .bind(concept_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("lingua_uid"))
    .collect())
}
