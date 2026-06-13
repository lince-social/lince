use async_trait::async_trait;
use domain::clean::configuration::Configuration;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait ConfigurationRepository: Send + Sync {
    async fn set_active(&self, id: &str) -> Result<(), Error>;
    async fn get_active(&self) -> Result<Configuration, Error>;
    async fn set_delete_confirmation_for_active(&self, enabled: bool) -> Result<(), Error>;
    async fn set_desktop_startup_for_active(
        &self,
        start_on_login: Option<bool>,
        start_silent: Option<bool>,
    ) -> Result<(), Error>;
    async fn set_language_for_active_if_unset(&self, language: &str) -> Result<(), Error>;
}

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

    async fn set_delete_confirmation_for_active(&self, enabled: bool) -> Result<(), Error> {
        let enabled_as_int = if enabled { 1_i64 } else { 0_i64 };
        sqlx::query("UPDATE configuration SET delete_confirmation = ? WHERE quantity = 1")
            .bind(enabled_as_int)
            .execute(&*self.pool)
            .await
            .map_err(Error::other)?;
        Ok(())
    }

    async fn set_desktop_startup_for_active(
        &self,
        start_on_login: Option<bool>,
        start_silent: Option<bool>,
    ) -> Result<(), Error> {
        sqlx::query(
            "
            UPDATE configuration
            SET desktop_start_on_login = ?,
                desktop_start_silent = ?
            WHERE quantity = 1
            ",
        )
        .bind(start_on_login.map(|value| if value { 1_i64 } else { 0_i64 }))
        .bind(start_silent.map(|value| if value { 1_i64 } else { 0_i64 }))
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        Ok(())
    }

    async fn set_language_for_active_if_unset(&self, language: &str) -> Result<(), Error> {
        sqlx::query(
            "
            UPDATE configuration
            SET language = ?
            WHERE quantity = 1
              AND (language IS NULL OR trim(language) = '')
            ",
        )
        .bind(language)
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        Ok(())
    }
}
