use sqlx::{Pool, Sqlite};

use crate::{
    domain::{
        entities::configuration::Configuration,
        repositories::configuration::ConfigurationRepository,
    },
    infrastructure::database::management::lib::connection,
};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

use async_trait::async_trait;

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
        // let pool = connection().await?;

        let query = format!(
            "UPDATE configuration SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
            id
        );

        sqlx::query(&query)
            .execute(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(())
    }

    async fn get_active(&self) -> Result<Configuration, Error> {
        let pool = connection().await?;

        sqlx::query_as("SELECT * FROM configuration WHERE quantity = 1")
            .fetch_one(&pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }
}
