//! Lince storage layer. The ONLY crate that speaks SQL (blueprint 0.1).
//! Everything above talks to typed repository functions; replacing this crate
//! (AniccaDB later) must not change the engine or Protein contracts.

pub mod concepts;
pub mod facts;
pub mod freqs;
pub mod links;
pub mod misc;
pub mod places;
pub mod records;
pub mod rules;
pub mod transfers;
pub mod visibility;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub use sqlx; // engine uses transactions; sqlx types come from here so only
              // `store` decides the driver.

#[derive(Debug, Clone)]
pub struct Store {
    pub pool: SqlitePool,
}

pub type StoreError = sqlx::Error;

impl Store {
    /// Open (creating if missing) a database file and run migrations.
    pub async fn open(url: &str) -> Result<Store, StoreError> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
        sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| match e {
            sqlx::migrate::MigrateError::Execute(e) => e,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
        Ok(Store { pool })
    }

    /// In-memory store for tests and DST. Single connection so the memory db
    /// is shared across all uses of the pool.
    pub async fn open_memory() -> Result<Store, StoreError> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await.map_err(|e| match e {
            sqlx::migrate::MigrateError::Execute(e) => e,
            other => sqlx::Error::Protocol(other.to_string()),
        })?;
        Ok(Store { pool })
    }
}
