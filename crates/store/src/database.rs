use crate::{
    Store, StoreError, linguas, organs, projection, signal_endpoints, transfer_replication,
};
use nucleus::performance::Span;
use sqlx::{
    SqlSafeStr, Sqlite, SqlitePool, Transaction,
    migrate::{MigrateError, Migrator},
    query, query_as, query_scalar,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{path::Path, str::FromStr, time::Duration};

const READ_LOCK_TIMEOUT: Duration = Duration::from_secs(2);
const WRITE_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const FILE_CONNECTIONS: u32 = 4;
static MIGRATIONS: Migrator = sqlx::migrate!("./migrations");

/// Begins an immediate write transaction and advances its commit sequence.
///
/// # Errors
///
/// Returns an error if acquiring the connection, beginning the transaction, or
/// advancing the sequence fails.
pub async fn write_tx(pool: &SqlitePool) -> Result<Transaction<'static, Sqlite>, StoreError> {
    let connection = {
        let _acquire = Span::new("store-write-pool-acquire-await");
        pool.acquire().await?
    };
    let mut tx = {
        let _begin = Span::new("store-write-begin-await");
        Transaction::begin(
            connection,
            Some(SqlSafeStr::into_sql_str("BEGIN IMMEDIATE")),
        )
        .await?
    };
    let _sequence = Span::new("store-write-sequence-await");
    query("UPDATE commit_sequence SET value = value + 1 WHERE id = 1")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

impl Store {
    /// Opens a read-only snapshot and validates the applied migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if the file is missing, cannot be read, or has invalid migrations.
    pub async fn open_readonly(path: &Path) -> Result<Store, StoreError> {
        if !path.is_file() {
            return Err(StoreError::Protocol(format!(
                "Lince database does not exist or is not a file: {}",
                path.display()
            )));
        }
        let options = SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .create_if_missing(false)
            .busy_timeout(READ_LOCK_TIMEOUT);
        let pool = single_connection_pool()
            .min_connections(1)
            .connect_with(options)
            .await?;
        query("BEGIN DEFERRED").execute(&pool).await?;
        let applied: Vec<(i64, Vec<u8>, bool)> =
            query_as("SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&pool)
                .await?;
        if applied.len() != MIGRATIONS.iter().count()
            || MIGRATIONS.iter().zip(&applied).any(|(expected, actual)| {
                expected.version != actual.0
                    || expected.checksum.as_ref() != actual.1.as_slice()
                    || !actual.2
            })
        {
            return Err(StoreError::Protocol("The database is not a current, valid Lince database; open it with Lince to initialize or migrate it".into()));
        }
        let _: i64 =
            query_scalar("SELECT count(*) FROM record WHERE kind = 'organ' AND deleted_at IS NULL")
                .fetch_one(&pool)
                .await?;
        Ok(Store::from_pool(pool))
    }

    /// Opens the Store and initializes migrations, Lingua, and the local identity.
    ///
    /// # Errors
    ///
    /// Returns an error if the pool cannot be opened or initialization fails.
    pub async fn open(url: &str) -> Result<Store, StoreError> {
        let pool = connect_file(url, FileMode::Initialize).await?;
        initialize(pool).await
    }

    /// Opens or creates a durable file-backed Store and applies its migrations.
    ///
    /// # Errors
    ///
    /// Returns an error if a durable file connection or migration fails.
    pub async fn open_durable(url: &str) -> Result<Store, StoreError> {
        let pool = connect_file(url, FileMode::CreateDurable).await?;
        migrate(&pool).await?;
        Ok(Store::from_pool(pool))
    }

    /// Reopens an existing durable file-backed Store with its stored schema.
    ///
    /// # Errors
    ///
    /// Returns an error if the existing file cannot be opened with durable settings.
    pub async fn open_existing_durable(url: &str) -> Result<Store, StoreError> {
        let pool = connect_file(url, FileMode::ReopenDurable).await?;
        Ok(Store::from_pool(pool))
    }

    /// Opens and initializes an in-memory Store on one retained connection.
    ///
    /// # Errors
    ///
    /// Returns an error if the connection or initialization fails.
    pub async fn open_memory() -> Result<Store, StoreError> {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?.foreign_keys(true);
        let pool = single_connection_pool().connect_with(opts).await?;
        initialize(pool).await
    }
}

enum FileMode {
    Initialize,
    CreateDurable,
    ReopenDurable,
}

fn single_connection_pool() -> SqlitePoolOptions {
    SqlitePoolOptions::new()
        .max_connections(1)
        .idle_timeout(None)
        .max_lifetime(None)
}

async fn connect_file(url: &str, mode: FileMode) -> Result<SqlitePool, StoreError> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(!matches!(mode, FileMode::ReopenDurable))
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(WRITE_LOCK_TIMEOUT);
    let pool = SqlitePoolOptions::new()
        .max_connections(FILE_CONNECTIONS)
        .connect_with(opts)
        .await?;
    if !matches!(mode, FileMode::Initialize) {
        require_file_backed_pool(&pool).await?;
    }
    Ok(pool)
}

async fn require_file_backed_pool(pool: &SqlitePool) -> Result<(), StoreError> {
    let filename: String =
        query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
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
    organs::ensure_local(&pool, "").await?;
    Ok(Store::from_pool(pool))
}

async fn migrate(pool: &SqlitePool) -> Result<(), StoreError> {
    MIGRATIONS.run(pool).await.map_err(|e| match e {
        MigrateError::Execute(e) => e,
        other => StoreError::Protocol(other.to_string()),
    })?;
    signal_endpoints::ensure_authority(pool).await?;
    transfer_replication::install(pool).await?;
    projection::install(pool).await
}
