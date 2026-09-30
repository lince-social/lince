use nucleus::simulation::{CheckDefinition, CheckSet};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub async fn list(pool: &SqlitePool) -> Result<Vec<CheckSet>, StoreError> {
    sqlx::query(
        "SELECT uid, name, revision, checks_json FROM simulation_check_set ORDER BY name, uid",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| {
        Ok(CheckSet {
            uid: row.try_get("uid")?,
            name: row.try_get("name")?,
            revision: row.try_get::<i64, _>("revision")? as u64,
            checks: serde_json::from_str(&row.try_get::<String, _>("checks_json")?)
                .map_err(|error| StoreError::Decode(Box::new(error)))?,
        })
    })
    .collect()
}

pub async fn save(
    pool: &SqlitePool,
    uid: &str,
    name: &str,
    revision: u64,
    checks: &[CheckDefinition],
) -> Result<CheckSet, StoreError> {
    if uid.is_empty()
        || uid.len() > 200
        || name.trim().is_empty()
        || name.len() > 200
        || checks.len() > 1024
        || revision >= i64::MAX as u64
    {
        return Err(StoreError::Protocol("invalid simulation check set".into()));
    }
    let json =
        serde_json::to_string(checks).map_err(|error| StoreError::Encode(Box::new(error)))?;
    if json.len() > 1024 * 1024 {
        return Err(StoreError::Protocol(
            "simulation check set is too large".into(),
        ));
    }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let current: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM simulation_check_set WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?;
    if current.map_or(0, |value| value as u64) != revision {
        return Err(StoreError::Protocol(
            "simulation check set changed; reload before saving".into(),
        ));
    }
    let revision = revision + 1;
    sqlx::query("INSERT INTO simulation_check_set(uid, name, revision, checks_json) VALUES (?, ?, ?, ?) ON CONFLICT(uid) DO UPDATE SET name = excluded.name, revision = excluded.revision, checks_json = excluded.checks_json")
        .bind(uid).bind(name.trim()).bind(revision as i64).bind(json).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(CheckSet {
        uid: uid.into(),
        name: name.trim().into(),
        revision,
        checks: checks.to_vec(),
    })
}

pub async fn remove(pool: &SqlitePool, uid: &str, revision: u64) -> Result<bool, StoreError> {
    let result = sqlx::query("DELETE FROM simulation_check_set WHERE uid = ? AND revision = ?")
        .bind(uid)
        .bind(i64::try_from(revision).unwrap_or(-1))
        .execute(pool)
        .await?;
    Ok(result.rows_affected() == 1)
}
