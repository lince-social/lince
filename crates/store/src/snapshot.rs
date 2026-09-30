mod cache;
mod hashing;

pub use cache::StateHasher;

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, TypeInfo, ValueRef};

use crate::{Store, StoreError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum Value {
    Null,
    Integer(i64),
    RealBits(u64),
    Text(String),
    Blob(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub name: String,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}

impl Store {
    pub async fn snapshot_into(&self, path: &Path) -> Result<(), StoreError> {
        if path.exists() {
            return Err(StoreError::Protocol(
                "snapshot destination already exists".into(),
            ));
        }
        let path = path
            .to_str()
            .ok_or_else(|| StoreError::Protocol("snapshot path is not UTF-8".into()))?;
        let mut connection = self.pool.acquire().await?;
        let filename: String =
            sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
                .fetch_one(&mut *connection)
                .await?;
        if filename.is_empty() {
            let bytes = connection.serialize(None).await?;
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(StoreError::Io)?;
            file.write_all(&bytes).map_err(StoreError::Io)?;
            file.sync_all().map_err(StoreError::Io)?;
            return Ok(());
        }
        sqlx::query("VACUUM main INTO ?")
            .bind(path)
            .execute(&mut *connection)
            .await?;
        Ok(())
    }

    pub async fn logical_snapshot(&self) -> Result<Vec<Table>, StoreError> {
        let mut tx = self.pool.begin().await?;
        let names: Vec<String> = sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type = 'table' AND (substr(name, 1, 7) <> 'sqlite_' OR name = 'sqlite_sequence') AND substr(name, 1, 5) <> '_sqlx' AND substr(name, 1, 11) <> 'projection_' ORDER BY name")
            .fetch_all(&mut *tx).await?;
        let mut tables = Vec::new();
        for name in names {
            let quoted = name.replace('"', "\"\"");
            let columns: Vec<String> =
                sqlx::query_scalar("SELECT name FROM pragma_table_info(?) ORDER BY cid")
                    .bind(&name)
                    .fetch_all(&mut *tx)
                    .await?;
            let rows = sqlx::query(&format!("SELECT * FROM \"{quoted}\""))
                .fetch_all(&mut *tx)
                .await?;
            let mut values = Vec::with_capacity(rows.len());
            for row in rows {
                let mut values_row = Vec::new();
                for column in row.columns() {
                    let raw = row.try_get_raw(column.ordinal())?;
                    let value = if raw.is_null() {
                        Value::Null
                    } else {
                        match raw.type_info().name() {
                            "INTEGER" => Value::Integer(row.try_get(column.ordinal())?),
                            "REAL" => {
                                Value::RealBits(row.try_get::<f64, _>(column.ordinal())?.to_bits())
                            }
                            "TEXT" => Value::Text(row.try_get(column.ordinal())?),
                            "BLOB" => Value::Blob(row.try_get(column.ordinal())?),
                            kind => {
                                return Err(StoreError::Protocol(format!(
                                    "unsupported snapshot value {kind}"
                                )));
                            }
                        }
                    };
                    values_row.push(value);
                }
                values.push(values_row);
            }
            let mut ordered = values
                .into_iter()
                .map(|row| {
                    serde_json::to_vec(&row)
                        .map(|key| (key, row))
                        .map_err(|error| StoreError::Protocol(error.to_string()))
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
