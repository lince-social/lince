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
    subject_kind: &str,
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

pub async fn visible_targets(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE grant_level = 'visible' AND (subject_uid = ? OR subject_kind = 'public')
         UNION
         SELECT DISTINCT record_uid AS uid FROM fact WHERE actor_uid = ?",
    )
    .bind(subject_uid)
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| r.get("uid"))
    .collect())
}

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

pub async fn role_targets(
    pool: &SqlitePool,
    subject_uid: &str,
    role_id: i64,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query_scalar::<_, String>(
        "WITH local AS (
             SELECT uid FROM record WHERE slug = ? AND kind = 'organ' AND deleted_at IS NULL
         ), rules AS (
             SELECT target_uid, field, grant_level,
                    (subject_kind = 'public'
                     OR (subject_kind = 'actor' AND subject_uid = ?)
                     OR (subject_kind = 'role' AND subject_uid = CAST(? AS TEXT))
                     OR (subject_kind = 'organ' AND subject_uid IN (SELECT uid FROM local))) AS matches
               FROM visibility_rule
         )
         SELECT r.uid FROM record r
          WHERE r.deleted_at IS NULL AND r.kind != 'message_draft'
            AND r.replica_root IS NULL AND r.organ_uid IN (SELECT uid FROM local)
            AND NOT EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid
                            AND (v.field IS NOT NULL OR (v.grant_level = 'hidden' AND v.matches)))
            AND (NOT EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid AND v.grant_level = 'visible')
                 OR EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid AND v.grant_level = 'visible' AND v.matches))",
    )
    .bind(crate::organs::LOCAL_ORGAN_SLUG)
    .bind(subject_uid)
    .bind(role_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect())
}

pub async fn hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| row.get("uid"))
    .collect())
}

pub async fn set_hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
    target_uid: &str,
    hidden: bool,
) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND target_uid = ?
            AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .bind(target_uid)
    .execute(pool)
    .await?;
    if hidden {
        sqlx::query(
            "INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'organ', ?, ?, 'hidden')",
        )
        .bind(nucleus::new_uid("v"))
        .bind(organ_uid)
        .bind(target_uid)
        .execute(pool)
        .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenRecordRow {
    pub uid: String,
    pub head: String,
    pub slug: Option<String>,
}

pub async fn hidden_records_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<HiddenRecordRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT v.target_uid AS uid,
                COALESCE(r.head, '') AS head,
                r.slug AS slug
           FROM visibility_rule v
           LEFT JOIN record r ON r.uid = v.target_uid
          WHERE v.subject_kind = 'organ' AND v.subject_uid = ?
            AND v.grant_level = 'hidden'
          ORDER BY head, uid",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| HiddenRecordRow {
        uid: row.get("uid"),
        head: row.get("head"),
        slug: row.get("slug"),
    })
    .collect())
}

pub async fn records_of_op(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(match tbl {
        "record" => vec![uid.to_string()],
        "fact" => {
            sqlx::query_scalar::<_, Option<String>>("SELECT record_uid FROM fact WHERE uid = ?")
                .bind(uid)
                .fetch_optional(pool)
                .await?
                .flatten()
                .into_iter()
                .collect()
        }
        "record_assertion" => {
            sqlx::query("SELECT subject_uid, object_uid FROM record_assertion WHERE uid = ?")
                .bind(uid)
                .fetch_optional(pool)
                .await?
                .into_iter()
                .flat_map(|row| {
                    let subject: String = row.get("subject_uid");
                    let object: Option<String> = row.get("object_uid");
                    std::iter::once(subject).chain(object)
                })
                .collect()
        }
        _ => Vec::new(),
    })
}

pub async fn op_hidden_from(
    pool: &SqlitePool,
    hidden: &HashSet<String>,
    tbl: &str,
    uid: &str,
) -> Result<bool, StoreError> {
    if hidden.is_empty() {
        return Ok(false);
    }
    if tbl == "record" {
        return Ok(hidden.contains(uid));
    }
    Ok(records_of_op(pool, tbl, uid)
        .await?
        .iter()
        .any(|record| hidden.contains(record)))
}
