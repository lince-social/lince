pub mod action_intents;
pub mod assertions;
pub mod auth;
pub mod backoff;
pub mod budget;
pub mod cells;
pub mod communication;
pub mod concepts;
pub mod config;
pub mod contact_rate;
pub mod contact_share;
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
pub mod mail_left;
pub mod mailbox;
pub mod misc;
pub mod offers;
pub mod organs;
pub mod people;
pub mod places;
pub mod read_filter;
pub mod record_changes;
pub mod record_docs;
pub mod record_move;
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

pub use sqlx;

#[derive(Debug, Clone)]
pub struct Store {
    pub pool: SqlitePool,
}

pub type StoreError = sqlx::Error;

pub async fn write_tx(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, StoreError> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

impl Store {
    pub async fn open(url: &str) -> Result<Store, StoreError> {
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(10));
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

async fn ensure_identity(pool: &SqlitePool) -> Result<(), StoreError> {
    organs::ensure_local(pool, "").await?;
    Ok(())
}
