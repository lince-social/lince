use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait OperationRepository: Send + Sync {
    async fn get_column_names(&self, table: String) -> Result<Vec<String>, Error>;
    async fn create(&self, table: String, data: HashMap<String, String>) -> Result<(), Error>;
}

pub struct OperationRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl OperationRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OperationRepository for OperationRepositoryImpl {
    async fn get_column_names(&self, table: String) -> Result<Vec<String>, Error> {
        let query = format!("PRAGMA table_info({})", table); // beware: not sanitized
        let rows = sqlx::query(&query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(rows
            .into_iter()
            .filter_map(|r| r.try_get("name").ok())
            .collect())
    }

    async fn create(&self, table: String, data: HashMap<String, String>) -> Result<(), Error> {
        let mut columns = Vec::new();
        let mut values = Vec::new();

        for (k, v) in data {
            if v.is_empty() {
                continue;
            }
            columns.push(k);
            values.push(if v.parse::<i64>().is_ok() || v.parse::<f64>().is_ok() {
                v
            } else {
                format!("'{}'", v)
            });
        }

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            table,
            columns.join(", "),
            values.join(", ")
        );
        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(())
    }
}
