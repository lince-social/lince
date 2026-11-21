//! Stored visibility grants, authority lookups, and Organ export restrictions.

use std::{
    collections::{HashMap, HashSet},
    iter::once,
};

use nucleus::new_uid;
use sqlx::{Row, SqliteConnection, SqlitePool, query, query_as, query_scalar, sqlite::SqliteRow};

use crate::{
    StoreError,
    data_visibility::{denied_for_person_on, denied_records},
    organs::LOCAL_ORGAN_SLUG,
    write_tx,
};

/// A stored visibility rule for a Record or one of its fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityRuleRow {
    /// Rule identity.
    pub uid: String,
    /// Kind of subject to which the rule applies.
    pub subject_kind: String,
    /// Subject identity, when the kind names a subject.
    pub subject_uid: Option<String>,
    /// Record governed by the rule.
    pub target_uid: String,
    /// Field governed by a field-specific rule.
    pub field: Option<String>,
    /// Stored visibility level.
    pub grant_level: String,
}

/// Grants the subject visibility of a Record and returns the rule identity.
/// # Errors
/// Returns an error if storing the rule fails.
pub async fn grant(
    pool: &SqlitePool,
    subject_kind: &str,
    subject_uid: Option<&str>,
    target_uid: &str,
) -> Result<String, StoreError> {
    let uid = new_uid("v");
    query(
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

/// Returns targets visible through a grant, authorship, or call admission, excluding saved denials.
/// # Errors
/// Returns an error if reading the targets or evaluating saved policies fails.
pub async fn visible_targets(
    pool: &SqlitePool,
    subject_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    let denied = denied_for_person_on(&mut *pool.acquire().await?, subject_uid).await?;
    Ok(query_scalar(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE grant_level = 'visible' AND (subject_kind = 'public'
             OR (subject_kind = 'actor' AND subject_uid = ?1)
             OR (subject_kind = 'role' AND subject_uid IN (SELECT CAST(role_id AS TEXT) FROM person_access WHERE person_uid = ?1 UNION SELECT CAST(role_id AS TEXT) FROM person_role WHERE person_uid = ?1))
             OR (subject_kind = 'organ' AND subject_uid = (SELECT uid FROM record WHERE slug = 'local-organ' AND kind = 'organ' AND deleted_at IS NULL)
                 AND EXISTS (SELECT 1 FROM person_access a JOIN record p ON p.uid = a.person_uid
                             WHERE a.person_uid = ?1 AND a.role_id IS NOT NULL AND p.deleted_at IS NULL)))
         UNION
         SELECT DISTINCT record_uid AS uid FROM fact WHERE actor_uid = ?1
         UNION
         SELECT r.uid FROM record r JOIN record_extension e ON e.record_uid = ?1
          WHERE e.namespace = 'lince.call-admission' AND r.deleted_at IS NULL
            AND json_extract(e.fds, '$.' || r.replica_root) = 1
            AND NOT EXISTS (SELECT 1 FROM visibility_rule v WHERE v.target_uid = r.uid
                 AND (v.field IS NOT NULL OR (v.grant_level = 'hidden'
                      AND (v.subject_kind = 'public' OR (v.subject_kind = 'actor' AND v.subject_uid = ?1)
                           OR (v.subject_kind = 'role' AND v.subject_uid IN (SELECT CAST(role_id AS TEXT) FROM person_access WHERE person_uid = ?1 UNION SELECT CAST(role_id AS TEXT) FROM person_role WHERE person_uid = ?1))))))",
    )
    .bind(subject_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .filter(|uid| !denied.contains(uid))
    .collect())
}

/// Lists recorded visibility rules in subject and field order.
/// # Errors
/// Returns an error if the rules cannot be read or decoded.
pub async fn rules_for_target(
    pool: &SqlitePool,
    target_uid: &str,
) -> Result<Vec<VisibilityRuleRow>, StoreError> {
    query(
        "SELECT uid, subject_kind, subject_uid, target_uid, field, grant_level
         FROM visibility_rule WHERE target_uid = ?
         ORDER BY subject_kind, subject_uid, field, uid",
    )
    .bind(target_uid)
    .try_map(|row| {
        Ok(VisibilityRuleRow {
            uid: row.try_get("uid")?,
            subject_kind: row.try_get("subject_kind")?,
            subject_uid: row.try_get("subject_uid")?,
            target_uid: row.try_get("target_uid")?,
            field: row.try_get("field")?,
            grant_level: row.try_get("grant_level")?,
        })
    })
    .fetch_all(pool)
    .await
}

/// Returns the Records visible to the Person while acting through the specified role.
/// # Errors
/// Returns an error if reading Records or evaluating saved policies fails.
pub async fn role_targets(
    pool: &SqlitePool,
    subject_uid: &str,
    role_id: i64,
) -> Result<HashSet<String>, StoreError> {
    role_targets_on(&mut *pool.acquire().await?, subject_uid, role_id).await
}

/// Returns the role-scoped Record visibility on the caller’s SQLite connection.
/// # Errors
/// Returns an error if reading Records or evaluating saved policies fails.
pub async fn role_targets_on(
    connection: &mut SqliteConnection,
    subject_uid: &str,
    role_id: i64,
) -> Result<HashSet<String>, StoreError> {
    let denied = denied_for_person_on(connection, subject_uid).await?;
    Ok(query_scalar::<_, String>(
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
            AND ((r.replica_root IS NULL AND r.organ_uid IN (SELECT uid FROM local))
                 OR (r.replica_root IS NOT NULL AND EXISTS (
                     SELECT 1 FROM rules v WHERE v.target_uid = r.uid AND v.grant_level = 'visible' AND v.matches))
                 OR EXISTS (SELECT 1 FROM record_extension e WHERE e.record_uid = ?
                     AND e.namespace = 'lince.call-admission' AND json_extract(e.fds, '$.' || r.replica_root) = 1))
            AND NOT EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid
                            AND (v.field IS NOT NULL OR (v.grant_level = 'hidden' AND v.matches)))
            AND (NOT EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid AND v.grant_level = 'visible')
                 OR EXISTS (SELECT 1 FROM rules v WHERE v.target_uid = r.uid AND v.grant_level = 'visible' AND v.matches))",
    )
    .bind(LOCAL_ORGAN_SLUG)
    .bind(subject_uid)
    .bind(role_id)
    .bind(subject_uid)
    .fetch_all(connection)
    .await?
    .into_iter()
    .filter(|uid| !denied.contains(uid))
    .collect())
}

/// Returns the union of explicit hides and saved Record denials for the Organ.
/// # Errors
/// Returns an error if reading either source of visibility restrictions fails.
pub async fn hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<HashSet<String>, StoreError> {
    let mut hidden: HashSet<String> = query_scalar(
        "SELECT target_uid AS uid FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .collect();
    hidden.extend(denied_records(pool, organ_uid).await?);
    Ok(hidden)
}

/// Toggles the explicit hidden rule for a Record and Organ.
/// # Errors
/// Returns an error if updating the explicit rule or committing the transaction fails.
pub async fn set_hidden_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
    target_uid: &str,
    hidden: bool,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    query(
        "DELETE FROM visibility_rule
          WHERE subject_kind = 'organ' AND subject_uid = ? AND target_uid = ?
            AND grant_level = 'hidden'",
    )
    .bind(organ_uid)
    .bind(target_uid)
    .execute(&mut *tx)
    .await?;
    if hidden {
        query(
            "INSERT INTO visibility_rule (uid, subject_kind, subject_uid, target_uid, grant_level)
             VALUES (?, 'organ', ?, ?, 'hidden')",
        )
        .bind(new_uid("v"))
        .bind(organ_uid)
        .bind(target_uid)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Diagnostic listing for an explicit hidden rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenRecordRow {
    /// Hidden Record identity.
    pub uid: String,
    /// Record title or empty text when the Record is absent.
    pub head: String,
    /// Record name, when known.
    pub slug: Option<String>,
}

fn hidden_record(row: &SqliteRow) -> Result<HiddenRecordRow, StoreError> {
    Ok(HiddenRecordRow {
        uid: row.try_get("uid")?,
        head: row.try_get("head")?,
        slug: row.try_get("slug")?,
    })
}

/// Lists explicit hides for one Organ in title and Record identity order.
/// # Errors
/// Returns an error if the diagnostic rows cannot be read or decoded.
pub async fn hidden_records_from_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<HiddenRecordRow>, StoreError> {
    query(
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
    .try_map(|row| hidden_record(&row))
    .fetch_all(pool)
    .await
}

/// Groups explicit hidden Records by requested Organ, omitting Organs with no rows.
/// # Errors
/// Returns an error if the Organ list cannot be encoded or the rows cannot be read.
pub async fn hidden_records_from_organs(
    pool: &SqlitePool,
    organs: &[&str],
) -> Result<HashMap<String, Vec<HiddenRecordRow>>, StoreError> {
    if organs.is_empty() {
        return Ok(HashMap::new());
    }
    let selected =
        serde_json::to_string(organs).map_err(|error| StoreError::Protocol(error.to_string()))?;
    let rows = query(
        "SELECT v.subject_uid, v.target_uid AS uid,
                COALESCE(r.head, '') AS head, r.slug AS slug
         FROM visibility_rule v
         LEFT JOIN record r ON r.uid = v.target_uid
         WHERE v.subject_kind = 'organ'
           AND v.subject_uid IN (SELECT value FROM json_each(?))
           AND v.grant_level = 'hidden'
         ORDER BY v.subject_uid, head, uid",
    )
    .bind(selected)
    .try_map(|row| Ok((row.try_get("subject_uid")?, hidden_record(&row)?)))
    .fetch_all(pool)
    .await?;
    let mut output = HashMap::<_, Vec<_>>::new();
    for (subject_uid, record) in rows {
        output.entry(subject_uid).or_default().push(record);
    }
    Ok(output)
}

/// Returns the Records associated with a supported synchronization operation.
/// # Errors
/// Returns an error if a Record association cannot be read or decoded.
pub async fn records_of_op(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Vec<String>, StoreError> {
    Ok(match tbl {
        "record" | "record_extension" => vec![uid.to_owned()],
        "place" => query_scalar("SELECT uid FROM record WHERE place_uid=? UNION SELECT uid FROM sync_op WHERE tbl='record' AND field='place_uid' AND json_valid(value) AND json_extract(value,'$')=?")
            .bind(uid).bind(uid).fetch_all(pool).await?,
        "fact" => {
            query_scalar::<_, Option<String>>("SELECT record_uid FROM fact WHERE uid = ?")
                .bind(uid)
                .fetch_optional(pool)
                .await?
                .flatten()
                .into_iter()
                .collect()
        }
        "record_assertion" => {
            query_as::<_, (String, Option<String>)>(
                "SELECT subject_uid, object_uid FROM record_assertion WHERE uid = ?",
            )
            .bind(uid)
            .fetch_optional(pool)
            .await?
            .into_iter()
            .flat_map(|(subject, object)| once(subject).chain(object))
            .collect()
        }
        _ => Vec::new(),
    })
}

/// Checks whether the operation touches any hidden Record.
/// # Errors
/// Returns an error if looking up the operation’s associated Records fails.
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
