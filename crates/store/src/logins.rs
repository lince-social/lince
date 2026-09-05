use chrono::Utc;
use sqlx::{Row, SqlitePool};

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    pub organ_uid: String,
    pub person_uid: String,
    pub created_at: String,
}

fn map(row: sqlx::sqlite::SqliteRow) -> Login {
    Login {
        organ_uid: row.get("organ_uid"),
        person_uid: row.get("person_uid"),
        created_at: row.get("created_at"),
    }
}

pub async fn grant(pool: &SqlitePool, organ_uid: &str, person_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO organ_login (organ_uid, person_uid, created_at) VALUES (?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET person_uid = excluded.person_uid",
    )
    .bind(organ_uid)
    .bind(person_uid)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn person_for_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<String>, StoreError> {
    sqlx::query_scalar("SELECT person_uid FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .fetch_optional(pool)
        .await
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Login>, StoreError> {
    Ok(sqlx::query("SELECT * FROM organ_login ORDER BY created_at")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

pub async fn revoke(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}
