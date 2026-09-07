use std::io::{self, Write};

use serde_json::Value;
use sqlx::{SqliteConnection, SqlitePool};

use crate::StoreError;

pub const MAX_POLICY_BYTES: usize = 1024 * 1024;
pub const MAX_POLICY_ROWS: usize = 4096;
pub const MAX_TOTAL_POLICY_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct RolePolicyRow {
    pub role_id: i64,
    pub policy: Option<Value>,
    pub revision: i64,
}

struct BoundedWriter {
    bytes: Vec<u8>,
}

impl BoundedWriter {
    fn new() -> Self {
        Self { bytes: Vec::new() }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let length = self
            .bytes
            .len()
            .checked_add(buf.len())
            .ok_or_else(|| io::Error::other("Role policy size overflow"))?;
        if length > MAX_POLICY_BYTES {
            return Err(io::Error::other("Role policy exceeds its byte limit"));
        }
        self.bytes.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn protocol(message: impl Into<String>) -> StoreError {
    sqlx::Error::Protocol(message.into())
}

fn validate_revision(revision: i64) -> Result<(), StoreError> {
    if revision <= 0 {
        return Err(protocol("Role policy has an invalid revision"));
    }
    Ok(())
}

fn validate_expected_revision(expected_revision: i64) -> Result<(), StoreError> {
    if expected_revision < 0 {
        return Err(protocol("Role policy expected revision cannot be negative"));
    }
    if expected_revision == i64::MAX {
        return Err(protocol("Role policy revision overflow"));
    }
    Ok(())
}

fn conflict(expected_revision: i64) -> StoreError {
    protocol(format!(
        "Role policy revision conflict at expected revision {expected_revision}"
    ))
}

fn encode_policy(policy: &Value) -> Result<String, StoreError> {
    if !policy.is_object() {
        return Err(protocol("Role policy must be a JSON object"));
    }
    let mut writer = BoundedWriter::new();
    serde_json::to_writer(&mut writer, policy)
        .map_err(|error| protocol(format!("Role policy is not serialisable: {error}")))?;
    String::from_utf8(writer.bytes)
        .map_err(|error| protocol(format!("Role policy is not UTF-8: {error}")))
}

fn decode_policy(policy: &str) -> Result<Value, StoreError> {
    let value: Value = serde_json::from_str(policy)
        .map_err(|error| protocol(format!("Role policy is not valid JSON: {error}")))?;
    if !value.is_object() {
        return Err(protocol("Role policy is not a JSON object"));
    }
    Ok(value)
}

fn decode_row(
    role_id: i64,
    policy: Option<String>,
    revision: i64,
    policy_bytes: i64,
) -> Result<RolePolicyRow, StoreError> {
    validate_revision(revision)?;
    let policy_bytes = usize::try_from(policy_bytes)
        .map_err(|_| protocol("Role policy has an invalid byte length"))?;
    if policy_bytes > MAX_POLICY_BYTES {
        return Err(protocol("Role policy exceeds its byte limit"));
    }
    let policy = policy.map(|policy| decode_policy(&policy)).transpose()?;
    Ok(RolePolicyRow {
        role_id,
        policy,
        revision,
    })
}

async fn validate_role_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<(), StoreError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM role WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&mut *connection)
        .await?
        .is_some();
    if !exists {
        return Err(protocol("Role policy requires an existing Role"));
    }
    Ok(())
}

pub async fn get(pool: &SqlitePool, role_id: i64) -> Result<Option<RolePolicyRow>, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let policy = get_on(&mut tx, role_id).await?;
    tx.commit().await?;
    Ok(policy)
}

pub async fn get_on(
    connection: &mut SqliteConnection,
    role_id: i64,
) -> Result<Option<RolePolicyRow>, StoreError> {
    let row = sqlx::query_as::<_, (i64, Option<String>, i64, i64)>(
        "SELECT role_id,
                CASE
                    WHEN policy IS NULL THEN NULL
                    WHEN length(CAST(policy AS BLOB)) <= ? THEN policy
                    ELSE NULL
                END,
                revision,
                COALESCE(length(CAST(policy AS BLOB)), 0)
           FROM role_policy
          WHERE role_id = ?",
    )
    .bind(i64::try_from(MAX_POLICY_BYTES).expect("policy limit fits SQLite"))
    .bind(role_id)
    .fetch_optional(&mut *connection)
    .await?;
    row.map(|(role_id, policy, revision, bytes)| decode_row(role_id, policy, revision, bytes))
        .transpose()
}

pub async fn all(pool: &SqlitePool) -> Result<Vec<RolePolicyRow>, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let policies = all_on(&mut tx).await?;
    tx.commit().await?;
    Ok(policies)
}

pub async fn all_on(connection: &mut SqliteConnection) -> Result<Vec<RolePolicyRow>, StoreError> {
    let rows = sqlx::query_as::<
        _,
        (
            Option<i64>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            i64,
            i64,
            i64,
        ),
    >(
        "WITH bounds AS (
             SELECT COUNT(*) AS row_count,
                    COALESCE(SUM(COALESCE(length(CAST(policy AS BLOB)), 0)), 0) AS total_bytes,
                    COALESCE(MAX(COALESCE(length(CAST(policy AS BLOB)), 0)), 0) AS largest_bytes
               FROM role_policy
         )
         SELECT policy.role_id, policy.policy, policy.revision,
                COALESCE(length(CAST(policy.policy AS BLOB)), 0),
                bounds.row_count, bounds.total_bytes, bounds.largest_bytes
           FROM bounds
           LEFT JOIN role_policy AS policy
             ON bounds.row_count <= ?
            AND bounds.total_bytes <= ?
            AND bounds.largest_bytes <= ?
          ORDER BY policy.role_id",
    )
    .bind(i64::try_from(MAX_POLICY_ROWS).expect("row limit fits SQLite"))
    .bind(i64::try_from(MAX_TOTAL_POLICY_BYTES).expect("total limit fits SQLite"))
    .bind(i64::try_from(MAX_POLICY_BYTES).expect("policy limit fits SQLite"))
    .fetch_all(&mut *connection)
    .await?;
    let Some(first) = rows.first() else {
        return Err(protocol("Role policy bounded read returned no bounds"));
    };
    let row_count = usize::try_from(first.4)
        .map_err(|_| protocol("Role policy list has an invalid row count"))?;
    let total_bytes = usize::try_from(first.5)
        .map_err(|_| protocol("Role policy list has an invalid total byte length"))?;
    let largest_bytes = usize::try_from(first.6)
        .map_err(|_| protocol("Role policy list has an invalid policy byte length"))?;
    if row_count > MAX_POLICY_ROWS {
        return Err(protocol("Role policy list exceeds its row limit"));
    }
    if total_bytes > MAX_TOTAL_POLICY_BYTES {
        return Err(protocol("Role policy list exceeds its total byte limit"));
    }
    if largest_bytes > MAX_POLICY_BYTES {
        return Err(protocol("Role policy list contains an oversized policy"));
    }
    if row_count == 0 {
        return Ok(Vec::new());
    }
    if rows.len() != row_count {
        return Err(protocol("Role policy list is incomplete"));
    }
    rows.into_iter()
        .map(|(role_id, policy, revision, policy_bytes, _, _, _)| {
            decode_row(
                role_id.ok_or_else(|| protocol("Role policy list is incomplete"))?,
                policy,
                revision.ok_or_else(|| protocol("Role policy list is incomplete"))?,
                policy_bytes.ok_or_else(|| protocol("Role policy list is incomplete"))?,
            )
        })
        .collect()
}

pub async fn compare_and_set(
    pool: &SqlitePool,
    role_id: i64,
    policy: Option<&Value>,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    let mut tx = crate::write_tx(pool).await?;
    let policy = compare_and_set_on(&mut tx, role_id, policy, expected_revision).await?;
    tx.commit().await?;
    Ok(policy)
}

pub async fn compare_and_set_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    policy: Option<&Value>,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    validate_expected_revision(expected_revision)?;
    validate_role_on(connection, role_id).await?;
    let policy = policy.map(encode_policy).transpose()?;
    let changed = if expected_revision == 0 {
        sqlx::query(
            "INSERT INTO role_policy (role_id, policy, revision)
             VALUES (?, ?, 1)
             ON CONFLICT(role_id) DO NOTHING",
        )
        .bind(role_id)
        .bind(policy.as_deref())
        .execute(&mut *connection)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE role_policy
                SET policy = ?, revision = revision + 1
              WHERE role_id = ? AND revision = ? AND revision < ?",
        )
        .bind(policy.as_deref())
        .bind(role_id)
        .bind(expected_revision)
        .bind(i64::MAX)
        .execute(&mut *connection)
        .await?
        .rows_affected()
    };
    if changed != 1 {
        return Err(conflict(expected_revision));
    }
    get_on(connection, role_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

pub async fn set(
    pool: &SqlitePool,
    role_id: i64,
    policy: &Value,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    compare_and_set(pool, role_id, Some(policy), expected_revision).await
}

pub async fn set_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    policy: &Value,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    compare_and_set_on(connection, role_id, Some(policy), expected_revision).await
}

pub async fn clear(
    pool: &SqlitePool,
    role_id: i64,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    compare_and_set(pool, role_id, None, expected_revision).await
}

pub async fn clear_on(
    connection: &mut SqliteConnection,
    role_id: i64,
    expected_revision: i64,
) -> Result<RolePolicyRow, StoreError> {
    compare_and_set_on(connection, role_id, None, expected_revision).await
}
