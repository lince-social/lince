use std::collections::BTreeMap;
use std::fmt::Write;

use nucleus::karma::CanonicalHash;
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{Column, Row, TypeInfo, ValueRef};

use crate::{Store, StoreError};

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

impl Store {
    pub async fn state_hash(&self) -> Result<CanonicalHash, StoreError> {
        let mut tx = self.pool.begin().await?;
        let metadata: Vec<(String, String)> = sqlx::query_as("SELECT t.name, c.name FROM sqlite_schema AS t JOIN pragma_table_info(t.name) AS c WHERE t.type = 'table' AND (substr(t.name, 1, 7) <> 'sqlite_' OR t.name = 'sqlite_sequence') AND substr(t.name, 1, 5) <> '_sqlx' AND substr(t.name, 1, 11) <> 'projection_' ORDER BY t.name, c.cid")
            .fetch_all(&mut *tx).await?;
        let mut tables: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, column) in metadata {
            tables.entry(name).or_default().push(column);
        }
        let mut hash = Sha256::new();
        hash.update(b"lince.canonical-json.v1\0lince.store.state.v1\0[");
        for (table_index, (name, columns)) in tables.into_iter().enumerate() {
            if table_index != 0 {
                hash.update(b",");
            }
            hash.update(b"{\"columns\":");
            hash.update(encode(&columns)?);
            hash.update(b",\"name\":");
            hash.update(encode(&name)?);
            hash.update(b",\"rows\":[");
            let quoted = name.replace('"', "\"\"");
            let rows = sqlx::query(&format!("SELECT * FROM \"{quoted}\""))
                .fetch_all(&mut *tx)
                .await?;
            let mut encoded_rows = Vec::with_capacity(rows.len());
            for row in &rows {
                let mut values = Vec::with_capacity(row.columns().len());
                for column in row.columns() {
                    let index = column.ordinal();
                    let raw = row.try_get_raw(index)?;
                    let value = if raw.is_null() {
                        BorrowedValue::Null
                    } else {
                        match raw.type_info().name() {
                            "INTEGER" => BorrowedValue::Integer(row.try_get(index)?),
                            "REAL" => {
                                BorrowedValue::RealBits(row.try_get::<f64, _>(index)?.to_bits())
                            }
                            "TEXT" => BorrowedValue::Text(row.try_get(index)?),
                            "BLOB" => BorrowedValue::Blob(row.try_get(index)?),
                            kind => {
                                return Err(StoreError::Protocol(format!(
                                    "unsupported snapshot value {kind}"
                                )));
                            }
                        }
                    };
                    values.push(value);
                }
                encoded_rows.push(encode(&values)?);
            }
            drop(rows);
            encoded_rows.sort_unstable();
            for (index, row) in encoded_rows.into_iter().enumerate() {
                if index != 0 {
                    hash.update(b",");
                }
                hash.update(row);
            }
            hash.update(b"]}");
        }
        hash.update(b"]");
        tx.rollback().await?;
        let mut encoded = String::from("sha256:");
        for byte in hash.finalize() {
            write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
        }
        CanonicalHash::parse(encoded).map_err(|error| StoreError::Protocol(error.to_string()))
    }
}
