use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{fs::create_dir_all, io::Error, path::PathBuf, time::Duration};

pub fn sqlite_db_path() -> Result<PathBuf, Error> {
    let lince_config_dir: PathBuf = utils::config::lince_data_dir()
        .ok_or_else(|| Error::other("Unable to resolve user config directory"))?;
    create_dir_all(&lince_config_dir)?;
    // `lince.db` is now owned exclusively by the new `store` crate's Cell schema
    // (`crates/store/migrations/0001_init.sql`). This legacy layer keeps its own
    // file so the two sqlx migration sets never collide on the same
    // `_sqlx_migrations` table. It is still load-bearing at boot (config, local
    // admin, karma cache, views/collections live in this schema), so it keeps
    // creating/migrating THIS file — but it never touches `lince.db`. As each
    // sand ports onto the new store, this schema shrinks toward
    // consultation-only, and the file is dropped once nothing reads it.
    Ok(lince_config_dir.join("lince-legacy.db"))
}

pub fn sqlite_connect_options() -> Result<SqliteConnectOptions, Error> {
    let db_path = sqlite_db_path()?;

    Ok(SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(3))
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true))
}

pub fn sqlite_read_only_connect_options() -> Result<SqliteConnectOptions, Error> {
    let db_path = sqlite_db_path()?;

    Ok(SqliteConnectOptions::new()
        .filename(db_path)
        .read_only(true)
        .busy_timeout(Duration::from_secs(3))
        .foreign_keys(true))
}

pub async fn connection() -> Result<Pool<Sqlite>, Error> {
    let options = sqlite_connect_options()?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(options)
        .await
        .map_err(Error::other)?;

    Ok(pool)
}

pub async fn read_only_connection() -> Result<Pool<Sqlite>, Error> {
    let options = sqlite_read_only_connect_options()?;

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(options)
        .await
        .map_err(Error::other)?;

    Ok(pool)
}
