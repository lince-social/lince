//! Visibility rules (blueprint XV.1): default hidden; whole-row grants in v1.
//! Enforcement lives in exactly one place — Protein's `execute_for`.

use sqlx::{Row, SqlitePool};
use std::collections::HashSet;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityRuleRow {
    pub uid: String,
    pub subject_kind: String,
    pub subject_uid: Option<String>,
    pub target_uid: String,
    pub field: Option<String>,
    pub grant_level: String,
}

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

/// Exact persisted disclosure grants for one target. Intrinsic Transfer
/// recipients (creator, parties, invitees) are derived by the Transfer
/// Protein and remain distinct from these explicit rules.
pub async fn rules_for_target(
    pool: &SqlitePool,
    target_uid: &str,
) -> Result<Vec<VisibilityRuleRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT uid, subject_kind, subject_uid, target_uid, field, grant_level
         FROM visibility_rule WHERE target_uid = ?
         ORDER BY subject_kind, subject_uid, field, uid",
    )
    .bind(target_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| VisibilityRuleRow {
        uid: row.get("uid"),
        subject_kind: row.get("subject_kind"),
        subject_uid: row.get("subject_uid"),
        target_uid: row.get("target_uid"),
        field: row.get("field"),
        grant_level: row.get("grant_level"),
    })
    .collect())
}
