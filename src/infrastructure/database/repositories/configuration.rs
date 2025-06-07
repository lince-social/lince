use crate::domain::{
    entities::configuration::Configuration, repositories::configuration::ConfigurationRepository,
};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

pub struct ConfigurationRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl ConfigurationRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ConfigurationRepository for ConfigurationRepositoryImpl {
    async fn set_active(&self, id: &str) -> Result<(), Error> {
        sqlx::query(&format!(
            "UPDATE configuration SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
            id
        ))
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(())
    }

    async fn get_active(&self) -> Result<Configuration, Error> {
        sqlx::query_as("SELECT * FROM configuration WHERE quantity = 1")
            .fetch_one(&*self.pool)
            .await
            .map_err(Error::other)
    }
}
