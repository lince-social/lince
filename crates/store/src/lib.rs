//! Lince storage layer. The ONLY crate that speaks SQL (blueprint 0.1).
//! Everything above talks to typed repository functions; replacing this crate
//! (AniccaDB later) must not change the engine or Protein contracts.

pub mod action_intents;
pub mod assertions;
pub mod auth;
pub mod cells;
pub mod communication;
pub mod concepts;
pub mod config;
pub mod door;
pub mod entries;
pub mod exact;
pub mod executor;
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
        ensure_identity(&pool).await?;
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
        ensure_identity(&pool).await?;
        Ok(Store { pool })
    }
}

/// Mint this Cell's identity — its Organ and its Cell Record — as part of
/// OPENING the store, not as a later bootstrap step.
///
/// It has to be here. `sync_ops::log_local` stamps every local write with the
/// actor Cell and the origin Organ, and before this it answered a missing
/// identity by silently skipping the op: a write that landed in the read model
/// and never entered the log, so it synced to nobody and no error said so.
/// "Most unit tests have no Organ" was the reason that path existed, and the
/// fix is for the state to be unreachable rather than handled.
///
/// The base URL starts empty and `organs::ensure_local` fills it in when the
/// web Cell binds a port. A URL is a reachability hint, not identity (Ontology
/// §11), so it is not something identity may wait on.
async fn ensure_identity(pool: &SqlitePool) -> Result<(), StoreError> {
    organs::ensure_local(pool, "").await?;
    Ok(())
}
