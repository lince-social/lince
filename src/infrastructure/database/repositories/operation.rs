use crate::domain::repositories::operation::OperationRepository;
use async_trait::async_trait;
use sqlx::{Pool, Row, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

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

    async fn create(&self, query: String) -> Result<(), Error> {
        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(())
    }
}
