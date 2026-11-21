//! Stored Person grants for remote Organ logins.

use nucleus::execution::now;
use sqlx::{Row, SqlitePool, query, query_scalar, sqlite::SqliteRow};

use crate::StoreError;

/// An Organ’s stored mapping to a Person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Login {
    /// Organ granted the login.
    pub organ_uid: String,
    /// Person represented by that login.
    pub person_uid: String,
    /// Time when the mapping was first created.
    pub created_at: String,
}

/// Assigns a Person to the Organ, keeping the original grant time on reassignment.
/// # Errors
/// Returns an error if the mapping conflicts with another Organ or cannot be saved.
pub async fn grant(pool: &SqlitePool, organ_uid: &str, person_uid: &str) -> Result<(), StoreError> {
    query(
        "INSERT INTO organ_login (organ_uid, person_uid, created_at) VALUES (?, ?, ?)
         ON CONFLICT(organ_uid) DO UPDATE SET person_uid = excluded.person_uid",
    )
    .bind(organ_uid)
    .bind(person_uid)
    .bind(now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

/// Reads the Person assigned to the Organ, if a grant exists.
/// # Errors
/// Returns an error if the assignment cannot be read.
pub async fn person_for_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Option<String>, StoreError> {
    query_scalar("SELECT person_uid FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .fetch_optional(pool)
        .await
}

/// Lists stored grants by original grant time.
/// # Errors
/// Returns an error if a grant cannot be read or decoded.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Login>, StoreError> {
    query("SELECT * FROM organ_login ORDER BY created_at")
        .try_map(|row: SqliteRow| {
            Ok(Login {
                organ_uid: row.try_get("organ_uid")?,
                person_uid: row.try_get("person_uid")?,
                created_at: row.try_get("created_at")?,
            })
        })
        .fetch_all(pool)
        .await
}

/// Removes the Organ’s grant and advances its admission generation if a grant existed.
/// # Errors
/// Returns an error if the grant cannot be removed.
pub async fn revoke(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    query("DELETE FROM organ_login WHERE organ_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}
