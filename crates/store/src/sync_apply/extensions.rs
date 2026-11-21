use serde_json::{Map, Value, from_str};
use sqlx::{Row, SqliteConnection, SqlitePool, query};

use super::Stamp;
use crate::{StoreError, write_tx};

async fn extension_fds(
    conn: &mut SqliteConnection,
    record_uid: &str,
    namespace: &str,
) -> Result<Value, StoreError> {
    Ok(
        query("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = ?")
            .bind(record_uid)
            .bind(namespace)
            .fetch_optional(&mut *conn)
            .await?
            .and_then(|r| from_str(r.get::<&str, _>("fds")).ok())
            .unwrap_or_else(|| Value::Object(Map::new())),
    )
}

async fn write_extension_fds(
    conn: &mut SqliteConnection,
    record_uid: &str,
    namespace: &str,
    fds: &Value,
) -> Result<(), StoreError> {
    query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(record_uid)
    .bind(namespace)
    .bind(fds.to_string())
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// Stores a field of an extension. A missing or legacy non-object value starts as an empty object.
pub async fn set_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
    value: Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    let mut fields = match extension_fds(&mut tx, record_uid, namespace).await? {
        Value::Object(fields) => fields,
        _ => Map::new(),
    };
    fields.insert(key.into(), value);
    write_extension_fds(&mut tx, record_uid, namespace, &Value::Object(fields)).await?;
    tx.commit().await
}

/// Removes an extension field unless the deletion has been superseded.
pub async fn tombstone_extension_key(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    key: &str,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    let mut fds = extension_fds(&mut tx, record_uid, namespace).await?;
    if let Some(map) = fds.as_object_mut() {
        map.remove(key);
    }
    write_extension_fds(&mut tx, record_uid, namespace, &fds).await?;
    tx.commit().await
}

/// Applies a complete extension value from a legacy operation unless superseded.
pub async fn set_extension_whole(
    pool: &SqlitePool,
    record_uid: &str,
    namespace: &str,
    value: &Value,
    stamp: Stamp<'_>,
) -> Result<(), StoreError> {
    let mut tx = write_tx(pool).await?;
    if stamp.superseded(&mut tx).await? {
        return Ok(());
    }
    write_extension_fds(&mut tx, record_uid, namespace, value).await?;
    tx.commit().await
}
