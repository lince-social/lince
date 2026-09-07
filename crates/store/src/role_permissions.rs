use std::collections::BTreeSet;

use sqlx::{Acquire, Sqlite, SqliteConnection, Transaction};

use crate::StoreError;
use crate::auth::{MAX_PERMISSION_KEY_BYTES, MAX_ROLE_PERMISSION_BYTES, MAX_ROLE_PERMISSIONS};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct PermissionIdentity {
    pub id: i64,
    pub subject: String,
    pub action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolePermissionSet {
    pub role_id: i64,
    pub revision: i64,
    pub permissions: Vec<PermissionIdentity>,
}

fn refusal(message: &str) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn validate_set(permissions: &[PermissionIdentity]) -> Result<(), StoreError> {
    if permissions.len() > MAX_ROLE_PERMISSIONS {
        return Err(refusal("Role permissions exceed the count limit"));
    }
    let mut bytes = 0usize;
    let mut ids = BTreeSet::new();
    let mut identities = BTreeSet::new();
    let mut keys = BTreeSet::new();
    for permission in permissions {
        let key_bytes = permission
            .subject
            .len()
            .checked_add(permission.action.len())
            .and_then(|length| length.checked_add(1))
            .ok_or_else(|| refusal("Role permission byte count overflow"))?;
        if permission.id <= 0
            || key_bytes > MAX_PERMISSION_KEY_BYTES
            || permission.subject.trim().is_empty()
            || permission.action.trim().is_empty()
        {
            return Err(refusal("Role permission identity is invalid or oversized"));
        }
        bytes = bytes
            .checked_add(key_bytes)
            .ok_or_else(|| refusal("Role permission byte count overflow"))?;
        if bytes > MAX_ROLE_PERMISSION_BYTES {
            return Err(refusal("Role permissions exceed the aggregate byte limit"));
        }
        if !ids.insert(permission.id)
            || !identities.insert((&permission.subject, &permission.action))
            || !keys.insert(format!("{}:{}", permission.subject, permission.action))
        {
            return Err(refusal(
                "Role permission identities are duplicate or ambiguous",
            ));
        }
    }
    Ok(())
}

async fn revision_on(connection: &mut SqliteConnection, role_id: i64) -> Result<i64, StoreError> {
    if role_id <= 0 {
        return Err(refusal(
            "Role permission set requires a positive Role identity",
        ));
    }
    let row = sqlx::query_as::<_, (i64, Option<i64>)>(
        "SELECT CASE WHEN typeof(r.id) = 'integer' AND r.id > 0
                           AND typeof(r.name) = 'text' AND length(trim(r.name)) > 0
                      THEN 1 ELSE 0 END,
                CASE WHEN typeof(v.role_id) = 'integer' AND v.role_id > 0
                           AND typeof(v.revision) = 'integer' AND v.revision > 0
                      THEN v.revision END
           FROM role r
           LEFT JOIN role_permission_revision v ON v.role_id = r.id
          WHERE r.id = ?",
    )
    .bind(role_id)
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| refusal("Role permission set requires an existing Role"))?;
    if row.0 != 1 {
        return Err(refusal("Role permission set has an invalid Role identity"));
    }
    row.1
        .ok_or_else(|| refusal("Role permission revision is missing or corrupt"))
}

async fn read_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<RolePermissionSet, StoreError> {
    let revision = revision_on(connection, role_id).await?;
    let bounds = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "WITH members AS (
             SELECT role_id, permission_id FROM role_permission
              WHERE role_id = ? LIMIT ?
         )
         SELECT COUNT(*),
                COALESCE(SUM(length(CAST(p.subject AS BLOB)) + 1
                             + length(CAST(p.action AS BLOB))), 0),
                COALESCE(MAX(length(CAST(p.subject AS BLOB)) + 1
                             + length(CAST(p.action AS BLOB))), 0),
                COALESCE(SUM(CASE
                    WHEN typeof(rp.role_id) = 'integer' AND rp.role_id > 0
                     AND typeof(rp.permission_id) = 'integer' AND rp.permission_id > 0
                     AND typeof(p.id) = 'integer' AND p.id > 0
                     AND typeof(p.subject) = 'text' AND typeof(p.action) = 'text'
                     AND length(CAST(p.subject AS BLOB)) + 1
                         + length(CAST(p.action AS BLOB)) <= ?
                     AND length(trim(p.subject)) > 0 AND length(trim(p.action)) > 0
                    THEN 0 ELSE 1 END), 0)
           FROM members rp
           LEFT JOIN permission p ON p.id = rp.permission_id",
    )
    .bind(role_id)
    .bind(MAX_ROLE_PERMISSIONS as i64 + 1)
    .bind(MAX_PERMISSION_KEY_BYTES as i64)
    .fetch_one(&mut *connection)
    .await?;
    if bounds.0 < 0
        || bounds.0 > MAX_ROLE_PERMISSIONS as i64
        || bounds.1 < 0
        || bounds.1 > MAX_ROLE_PERMISSION_BYTES as i64
        || bounds.2 < 0
        || bounds.2 > MAX_PERMISSION_KEY_BYTES as i64
        || bounds.3 != 0
    {
        return Err(refusal(
            "Role permission set has invalid or oversized stored data",
        ));
    }
    let rows = sqlx::query_as::<_, (i64, Option<String>, Option<String>)>(
        "SELECT rp.permission_id,
                CASE WHEN typeof(p.subject) = 'text' AND typeof(p.action) = 'text'
                           AND length(CAST(p.subject AS BLOB)) + 1
                             + length(CAST(p.action AS BLOB)) <= ?
                      THEN p.subject END,
                CASE WHEN typeof(p.subject) = 'text' AND typeof(p.action) = 'text'
                           AND length(CAST(p.subject AS BLOB)) + 1
                             + length(CAST(p.action AS BLOB)) <= ?
                      THEN p.action END
           FROM role_permission rp
           LEFT JOIN permission p ON p.id = rp.permission_id
          WHERE rp.role_id = ?
          ORDER BY rp.permission_id LIMIT ?",
    )
    .bind(MAX_PERMISSION_KEY_BYTES as i64)
    .bind(MAX_PERMISSION_KEY_BYTES as i64)
    .bind(role_id)
    .bind(MAX_ROLE_PERMISSIONS as i64 + 1)
    .fetch_all(&mut *connection)
    .await?;
    if rows.len() != bounds.0 as usize {
        return Err(refusal("Role permission set read is incomplete"));
    }
    let permissions = rows
        .into_iter()
        .map(|(id, subject, action)| {
            Ok(PermissionIdentity {
                id,
                subject: subject.ok_or_else(|| refusal("Role permission subject is invalid"))?,
                action: action.ok_or_else(|| refusal("Role permission action is invalid"))?,
            })
        })
        .collect::<Result<Vec<_>, StoreError>>()?;
    validate_set(&permissions)?;
    Ok(RolePermissionSet {
        role_id,
        revision,
        permissions,
    })
}

pub async fn get_on(
    transaction: &mut Transaction<'_, Sqlite>,
    role_id: i64,
) -> Result<RolePermissionSet, StoreError> {
    read_on(transaction, role_id).await
}

async fn resolve_on(
    connection: &mut SqliteConnection,
    permission: &PermissionIdentity,
) -> Result<(), StoreError> {
    let valid = sqlx::query_scalar::<_, i64>(
        "SELECT CASE WHEN typeof(id) = 'integer' AND id > 0
                           AND typeof(subject) = 'text' AND typeof(action) = 'text'
                           AND subject = ? AND action = ?
                      THEN 1 ELSE 0 END
           FROM permission WHERE id = ?",
    )
    .bind(&permission.subject)
    .bind(&permission.action)
    .bind(permission.id)
    .fetch_optional(connection)
    .await?;
    if valid != Some(1) {
        return Err(refusal(
            "Role permission identity is unknown or has changed",
        ));
    }
    Ok(())
}

async fn replace_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    expected_revision: i64,
    proposed: &[PermissionIdentity],
) -> Result<RolePermissionSet, StoreError> {
    let current = read_on(connection, role_id).await?;
    if current.revision != expected_revision {
        return Err(refusal("Role permission revision conflict"));
    }
    for permission in proposed {
        resolve_on(connection, permission).await?;
    }
    let current_ids: BTreeSet<_> = current.permissions.iter().map(|entry| entry.id).collect();
    let proposed_ids: BTreeSet<_> = proposed.iter().map(|entry| entry.id).collect();
    let removed: Vec<_> = current_ids.difference(&proposed_ids).copied().collect();
    let added: Vec<_> = proposed_ids.difference(&current_ids).copied().collect();
    let increments = (removed.len() + added.len()) as i64;
    let final_revision = current
        .revision
        .checked_add(increments)
        .ok_or_else(|| refusal("Role permission revision overflow"))?;
    if increments == 0 {
        return Ok(current);
    }
    for permission_id in removed {
        let changed =
            sqlx::query("DELETE FROM role_permission WHERE role_id = ? AND permission_id = ?")
                .bind(role_id)
                .bind(permission_id)
                .execute(&mut *connection)
                .await?
                .rows_affected();
        if changed != 1 {
            return Err(refusal(
                "Role permission replacement removed an incomplete set",
            ));
        }
    }
    for permission_id in added {
        let changed =
            sqlx::query("INSERT INTO role_permission (role_id, permission_id) VALUES (?, ?)")
                .bind(role_id)
                .bind(permission_id)
                .execute(&mut *connection)
                .await?
                .rows_affected();
        if changed != 1 {
            return Err(refusal(
                "Role permission replacement inserted an incomplete set",
            ));
        }
    }
    let result = read_on(connection, role_id).await?;
    let mut expected = proposed.to_vec();
    expected.sort();
    if result.permissions != expected || result.revision != final_revision {
        return Err(refusal(
            "Role permission replacement or revision is inconsistent",
        ));
    }
    Ok(result)
}

pub async fn compare_and_set_on(
    transaction: &mut Transaction<'_, Sqlite>,
    role_id: i64,
    expected_revision: i64,
    proposed: &[PermissionIdentity],
) -> Result<RolePermissionSet, StoreError> {
    if expected_revision <= 0 {
        return Err(refusal(
            "Role permission expected revision must be positive",
        ));
    }
    validate_set(proposed)?;
    let mut savepoint = transaction.begin().await?;
    let result = replace_on(&mut savepoint, role_id, expected_revision, proposed).await;
    match result {
        Ok(result) => {
            savepoint.commit().await?;
            Ok(result)
        }
        Err(error) => {
            savepoint.rollback().await?;
            Err(error)
        }
    }
}
