use std::collections::HashMap;

use serde_json::{Value, from_str, to_string};
use sqlx::{Executor, QueryBuilder, Row, Sqlite, SqlitePool, sqlite::SqliteRow};

use super::protocol;
use crate::StoreError;

/// Reads one stored extension, including on a deleted Record. Invalid JSON is omitted.
pub async fn get_extension(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
) -> Result<Option<Value>, StoreError> {
    Ok(
        sqlx::query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(pool)
            .await?
            .and_then(|r| from_str(r.get::<&str, _>("fds")).ok()),
    )
}

/// Reads an extension for selected Records, including deleted Records.
pub async fn get_extensions(
    pool: &SqlitePool,
    records: &[&str],
    namespace: &str,
) -> Result<HashMap<String, Value>, StoreError> {
    if records.is_empty() {
        return Ok(HashMap::new());
    }
    let selected = to_string(records).map_err(|error| protocol(error.to_string()))?;
    let rows = sqlx::query(
        "SELECT record_uid, fds FROM record_extension
         WHERE namespace = ? AND record_uid IN (SELECT value FROM json_each(?))",
    )
    .bind(namespace)
    .bind(selected)
    .fetch_all(pool)
    .await?;
    Ok(map_extensions(rows))
}

/// Reads an extension across all live Records.
pub async fn all_extensions<'e>(
    executor: impl Executor<'e, Database = Sqlite>,
    namespace: &str,
) -> Result<HashMap<String, Value>, StoreError> {
    Ok(map_extensions(extension_rows(executor, namespace).await?))
}

pub(crate) async fn extension_rows<'e>(
    executor: impl Executor<'e, Database = Sqlite>,
    namespace: &str,
) -> Result<Vec<SqliteRow>, StoreError> {
    sqlx::query(
        "SELECT e.record_uid, e.fds
           FROM record_extension e
           JOIN record r ON r.uid = e.record_uid
          WHERE e.namespace = ? AND r.deleted_at IS NULL",
    )
    .bind(namespace)
    .fetch_all(executor)
    .await
}

pub(crate) fn map_extensions(rows: Vec<SqliteRow>) -> HashMap<String, Value> {
    rows.into_iter().filter_map(map_extension).collect()
}

fn map_extension(row: SqliteRow) -> Option<(String, Value)> {
    let uid = row.get("record_uid");
    from_str(row.get::<&str, _>("fds"))
        .ok()
        .map(|value| (uid, value))
}

/// Reads an extension for selected live Records in bounded batches.
pub async fn extensions_for_records(
    pool: &SqlitePool,
    namespace: &str,
    records: &[String],
) -> Result<HashMap<String, Value>, StoreError> {
    let mut output = HashMap::new();
    for records in records.chunks(512) {
        let mut query = QueryBuilder::new(
            "SELECT e.record_uid, e.fds FROM record_extension e JOIN record r ON r.uid = e.record_uid WHERE r.deleted_at IS NULL AND e.namespace = ",
        );
        query.push_bind(namespace).push(" AND e.record_uid IN (");
        let mut arguments = query.separated(",");
        for uid in records {
            arguments.push_bind(uid);
        }
        arguments.push_unseparated(")");
        output.extend(
            query
                .build()
                .fetch_all(pool)
                .await?
                .into_iter()
                .filter_map(map_extension),
        );
    }
    Ok(output)
}
