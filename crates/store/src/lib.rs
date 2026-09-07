pub mod access_snapshot;
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
pub mod operation_receipts;
pub mod organs;
pub mod people;
pub mod places;
pub mod private_contacts;
pub mod read_filter;
pub mod record_changes;
pub mod record_docs;
pub mod record_move;
pub mod record_revisions;
pub mod records;
pub mod recurrence;
pub mod replica;
pub mod role_permissions;
pub mod role_policies;
pub mod roster;
pub mod seed;
pub mod senses;
pub mod session_access;
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
        let pool = connect_file(url, sqlx::sqlite::SqliteSynchronous::Normal, true, false).await?;
        initialize(pool).await
    }

    pub async fn open_durable(url: &str) -> Result<Store, StoreError> {
        let pool = connect_file(url, sqlx::sqlite::SqliteSynchronous::Full, true, true).await?;
        migrate(&pool).await?;
        Ok(Store { pool })
    }

    pub async fn open_existing_durable(url: &str) -> Result<Store, StoreError> {
        let pool = connect_file(url, sqlx::sqlite::SqliteSynchronous::Full, false, true).await?;
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
        initialize(pool).await
    }
}

async fn connect_file(
    url: &str,
    synchronous: sqlx::sqlite::SqliteSynchronous,
    create_if_missing: bool,
    require_file_backed: bool,
) -> Result<SqlitePool, StoreError> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(create_if_missing)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(synchronous)
        .busy_timeout(std::time::Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(opts)
        .await?;
    if require_file_backed {
        require_file_backed_pool(&pool).await?;
    }
    Ok(pool)
}

async fn require_file_backed_pool(pool: &SqlitePool) -> Result<(), StoreError> {
    let filename: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
            .fetch_one(pool)
            .await?;
    if filename.is_empty() {
        return Err(StoreError::Configuration(
            "durable Store requires a file-backed SQLite database".into(),
        ));
    }
    Ok(())
}

async fn initialize(pool: SqlitePool) -> Result<Store, StoreError> {
    migrate(&pool).await?;
    linguas::ensure_local(&pool).await?;
    ensure_identity(&pool).await?;
    Ok(Store { pool })
}

async fn migrate(pool: &SqlitePool) -> Result<(), StoreError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| match e {
            sqlx::migrate::MigrateError::Execute(e) => e,
            other => sqlx::Error::Protocol(other.to_string()),
        })
}

async fn ensure_identity(pool: &SqlitePool) -> Result<(), StoreError> {
    organs::ensure_local(pool, "").await?;
    Ok(())
}
