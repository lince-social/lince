use crate::domain::repositories::operation::OperationRepository;
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
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
        let query = format!("PRAGMA table_info({})", table);
        let column_names: Vec<String> = sqlx::query(&query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?
            .into_iter()
            .map(|row| row.get("name"))
            .collect();
        Ok(column_names)
    }

    async fn create(&self, query: String) -> Result<(), Error> {
        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(())
    }
}
