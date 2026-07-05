//! Visibility rules (blueprint XV.1): default hidden; whole-row grants in v1.
//! Enforcement lives in exactly one place — Protein's `execute_for`.

use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

use crate::StoreError;

pub async fn grant(
    pool: &SqlitePool,
    subject_kind: &str, // organ | actor | public | fiote
    subject_uid: Option<&str>,
    target_uid: &str,
) -> Result<String, StoreError> {
    let uid = nucleus::new_uid("v");
    sqlx::query(
        "INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level)
         VALUES (?, ?, ?, ?, 'visible')",
    )
    .bind(&uid)
    .bind(subject_kind)
    .bind(subject_uid)
    .bind(target_uid)
    .execute(pool)
    .await?;
    Ok(uid)
}

/// Every target visible to a subject: its own grants plus public ones.
pub async fn visible_targets(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT target_uid FROM visibility_rule
         WHERE grant_level = 'visible' AND (subject_uid = ? OR subject_kind = 'public')",
    )
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get("target_uid"))
    .collect())
}
