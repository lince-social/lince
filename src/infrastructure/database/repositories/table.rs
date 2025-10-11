use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait TableRepository: Send + Sync {
    async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error>;
}

pub struct TableRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl TableRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl TableRepository for TableRepositoryImpl {
    async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error> {
        let query = format!("DELETE FROM {} WHERE id = {}", table, id);

        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(())
    }
}
