//! Lince storage layer. The ONLY crate that speaks SQL (blueprint 0.1).
//! Everything above talks to typed repository functions; replacing this crate
//! (AniccaDB later) must not change the engine or Protein contracts.

pub mod action_intents;
pub mod assertions;
pub mod auth;
pub mod communication;
pub mod concepts;
pub mod config;
pub mod entries;
pub mod exact;
pub mod facts;
pub mod frequency;
pub mod invites;
pub mod karma;
pub mod ledger;
pub mod linguas;
pub mod logins;
pub mod misc;
pub mod organs;
pub mod places;
pub mod record_docs;
pub mod records;
pub mod recurrence;
pub mod replica;
pub mod roster;
pub mod seed;
pub mod senses;
pub mod sync_apply;
pub mod sync_ops;
pub mod transfer_delivery;
pub mod transfers;
pub mod visibility;

use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
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
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| match e {
                sqlx::migrate::MigrateError::Execute(e) => e,
                other => sqlx::Error::Protocol(other.to_string()),
            })?;
        linguas::ensure_local(&pool).await?;
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
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| match e {
                sqlx::migrate::MigrateError::Execute(e) => e,
                other => sqlx::Error::Protocol(other.to_string()),
            })?;
        linguas::ensure_local(&pool).await?;
        Ok(Store { pool })
    }
}
