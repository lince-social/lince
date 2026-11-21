use std::{collections::BTreeMap, fmt::from_fn};

use nucleus::karma::CanonicalHash;
use sha2::{Digest, Sha256};
use sqlx::query_as;

use super::{decode_values, encode, table_rows};
use crate::{Store, StoreError};

impl Store {
    /// Hashes the logical Store state in its canonical snapshot format.
    /// # Errors
    /// Returns an error if the snapshot cannot be read or encoded.
    pub async fn state_hash(&self) -> Result<CanonicalHash, StoreError> {
        let mut tx = self.pool.begin().await?;
        let metadata: Vec<(String, String)> = query_as("SELECT t.name, c.name FROM sqlite_schema AS t JOIN pragma_table_info(t.name) AS c WHERE t.type = 'table' AND (substr(t.name, 1, 7) <> 'sqlite_' OR t.name = 'sqlite_sequence') AND substr(t.name, 1, 5) <> '_sqlx' AND substr(t.name, 1, 11) <> 'projection_' ORDER BY t.name, c.cid")
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
            let rows = table_rows(&mut tx, &name).await?;
            let mut encoded_rows = Vec::with_capacity(rows.len());
            for row in &rows {
                let values = decode_values(row).collect::<Result<Vec<_>, _>>()?;
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
        let digest = hash.finalize();
        let hex = from_fn(|formatter| {
            for byte in &digest {
                write!(formatter, "{byte:02x}")?;
            }
            Ok(())
        });
        CanonicalHash::parse(format!("sha256:{hex}"))
            .map_err(|error| StoreError::Protocol(error.to_string()))
    }
}
