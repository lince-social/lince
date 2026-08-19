//! Lince storage layer. The ONLY crate that speaks SQL (blueprint 0.1).
//! Everything above talks to typed repository functions; replacing this crate
//! (AniccaDB later) must not change the engine or Protein contracts.

pub mod action_intents;
pub mod assertions;
pub mod auth;
pub mod budget;
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
pub mod mail_left;
pub mod mailbox;
pub mod logins;
pub mod misc;
pub mod organs;
pub mod people;
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

/// Begin a transaction that is going to WRITE.
///
/// **Use this instead of `pool.begin()` for anything that writes**, which in
/// this crate is every transaction — they exist to make multi-statement writes
/// atomic.
///
/// `pool.begin()` issues a plain `BEGIN`, which SQLite treats as *deferred*:
/// the transaction is assumed to be a reader, takes a read snapshot, and only
/// asks for the write lock when it first writes. If any other connection
/// committed in between, that upgrade is REFUSED immediately — code 517,
/// `SQLITE_BUSY_SNAPSHOT`, surfaced as "database is locked". It cannot wait,
/// because waiting would deadlock two transactions each holding a read
/// snapshot, so no `busy_timeout` and no journal mode avoids it. Almost every
/// transaction here reads before it writes (read the quantity, then append the
/// Fact; read the assertions, then transition them), so almost every one of
/// them could lose that race.
///
/// `BEGIN IMMEDIATE` takes the write lock up front. There is no upgrade to
/// lose, and a second writer WAITS on the busy timeout instead of failing.
/// Readers are untouched: under WAL they keep reading the last committed
/// snapshot while a writer works, which is the "many readers, one writer"
/// model SQLite is built for and the shape of this app — many live Protein
/// subscriptions reading, one Action writing.
pub async fn write_tx(
    pool: &SqlitePool,
) -> Result<sqlx::Transaction<'static, sqlx::Sqlite>, StoreError> {
    pool.begin_with("BEGIN IMMEDIATE").await
}

impl Store {
    /// Open (creating if missing) a database file and run migrations.
    pub async fn open(url: &str) -> Result<Store, StoreError> {
        // **WAL, and a busy timeout.** Without both, this pool's four
        // connections contend on SQLite's default rollback journal, where a
        // writer takes an exclusive lock and every other connection is refused
        // outright — `database is locked`, SQLITE_BUSY, code 5. It surfaced
        // the first time a single Action wrote a few hundred rows in a row
        // (importing the documentation bundle) while the board held live
        // Protein subscriptions open for reading.
        //
        // Not caught by any test, and that is the part worth remembering:
        // `open_memory()` below runs on ONE connection, so nothing in the
        // suite can produce contention at all. Concurrency bugs here are
        // invisible until the real app runs.
        //
        // WAL lets readers carry on while a writer works, which is exactly the
        // shape of this app — many subscriptions reading, one Action writing.
        // `Normal` synchronous is the standard companion to WAL: durable
        // across a process crash, and only at risk in a power cut, which is
        // the right trade for a local-first app that already keeps an op log.
        let opts = SqliteConnectOptions::from_str(url)?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(10));
        // **Many readers, one writer** — the model SQLite is built for, and
        // the shape of this app: many live Protein subscriptions reading while
        // one Action writes.
        //
        // It only works because every write transaction goes through
        // `write_tx` (`BEGIN IMMEDIATE`). With plain `BEGIN` this pool was
        // where the owner's `database is locked` came from: several
        // connections, each transaction reading before it wrote, and whichever
        // one lost the race refused outright. See `write_tx` for why no
        // timeout can fix that, and `tests/concurrency.rs` for the
        // reproduction.
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
