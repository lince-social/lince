//! Physical SQLite backups and canonical snapshots of Store contents.

mod backup;
mod cache;
mod hashing;

use std::path::Path;

pub use cache::StateHasher;
use serde::{Deserialize, Serialize};
use sqlx::{
    AssertSqlSafe, Column, Row, SqliteConnection, TypeInfo, ValueRef, query, query_scalar,
    sqlite::SqliteRow,
};

use crate::{Store, StoreError};

/// A SQLite value in the stable logical snapshot format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Value {
    /// SQL NULL.
    Null,
    /// A signed SQLite INTEGER.
    Integer(i64),
    /// A SQLite REAL represented by its exact IEEE 754 bits.
    RealBits(u64),
    /// A SQLite TEXT value.
    Text(String),
    /// The bytes of a SQLite BLOB.
    Blob(Vec<u8>),
}

/// A logical snapshot table with columns in schema order and rows in canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    /// Table name as stored in the SQLite schema.
    pub name: String,
    /// Column names from SQLite's table metadata.
    pub columns: Vec<String>,
    /// Rows ordered by the JSON encoding of their values.
    pub rows: Vec<Vec<Value>>,
}

impl Store {
    /// Copies a consistent SQLite snapshot to a new file.
    /// # Errors
    /// Returns an error if the destination already exists, the path is invalid,
    /// or SQLite cannot complete the backup.
    pub async fn snapshot_into(&self, path: &Path) -> Result<(), StoreError> {
        backup::snapshot(&self.pool, path).await
    }

    /// Reads a consistent logical snapshot in canonical table and row order.
    ///
    /// Includes `sqlite_sequence` and excludes other SQLite internals,
    /// SQLx metadata, and derived projection tables.
    /// # Errors
    /// Returns an error if data cannot be read, decoded, or encoded for ordering.
    pub async fn logical_snapshot(&self) -> Result<Vec<Table>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let names: Vec<String> = query_scalar("SELECT name FROM sqlite_schema WHERE type = 'table' AND (substr(name, 1, 7) <> 'sqlite_' OR name = 'sqlite_sequence') AND substr(name, 1, 5) <> '_sqlx' AND substr(name, 1, 11) <> 'projection_' ORDER BY name")
            .fetch_all(&mut *tx).await?;
        let mut tables = Vec::with_capacity(names.len());
        for name in names {
            let columns: Vec<String> =
                query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                    .bind(&name)
                    .fetch_all(&mut *tx)
                    .await?;
            let mut ordered = table_rows(&mut tx, &name)
                .await?
                .into_iter()
                .map(|row| {
                    let row = decode_values(&row)
                        .map(|value| value.map(BorrowedValue::into_owned))
                        .collect::<Result<Vec<_>, _>>()?;
                    encode(&row).map(|key| (key, row))
                })
                .collect::<Result<Vec<_>, _>>()?;
            ordered.sort_by(|left, right| left.0.cmp(&right.0));
            tables.push(Table {
                name,
                columns,
                rows: ordered.into_iter().map(|(_, row)| row).collect(),
            });
        }
        tx.rollback().await?;
        Ok(tables)
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
enum BorrowedValue<'a> {
    Null,
    Integer(i64),
    RealBits(u64),
    Text(&'a str),
    Blob(&'a [u8]),
}

fn encode(value: &impl Serialize) -> Result<Vec<u8>, StoreError> {
    serde_json::to_vec(value).map_err(|error| StoreError::Protocol(error.to_string()))
}

impl BorrowedValue<'_> {
    fn into_owned(self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Integer(value) => Value::Integer(value),
            Self::RealBits(bits) => Value::RealBits(bits),
            Self::Text(text) => Value::Text(text.to_owned()),
            Self::Blob(bytes) => Value::Blob(bytes.to_vec()),
        }
    }
}

fn decode_values(row: &SqliteRow) -> impl Iterator<Item = Result<BorrowedValue<'_>, StoreError>> {
    row.columns().iter().map(move |column| {
        let index = column.ordinal();
        let raw = row.try_get_raw(index)?;
        if raw.is_null() {
            return Ok(BorrowedValue::Null);
        }
        match raw.type_info().name() {
            "INTEGER" => row.try_get(index).map(BorrowedValue::Integer),
            "REAL" => row
                .try_get::<f64, _>(index)
                .map(|value| BorrowedValue::RealBits(value.to_bits())),
            "TEXT" => row.try_get(index).map(BorrowedValue::Text),
            "BLOB" => row.try_get(index).map(BorrowedValue::Blob),
            kind => Err(StoreError::Protocol(format!(
                "unsupported snapshot value {kind}"
            ))),
        }
    })
}

async fn table_rows(
    connection: &mut SqliteConnection,
    name: &str,
) -> Result<Vec<SqliteRow>, StoreError> {
    // SQLite identifiers require doubled embedded quotes and cannot be bound.
    let quoted = name.replace('"', "\"\"");
    query(AssertSqlSafe(format!("SELECT * FROM \"{quoted}\"")))
        .fetch_all(connection)
        .await
}
