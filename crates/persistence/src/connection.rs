use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{fs::create_dir_all, io::Error, path::PathBuf, time::Duration};

pub fn sqlite_connect_options() -> Result<SqliteConnectOptions, Error> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| Error::other("Unable to resolve user config directory"))?;
    let lince_config_dir: PathBuf = config_dir.join("lince");
    create_dir_all(&lince_config_dir)?;
    let db_path = lince_config_dir.join("lince.db");

    Ok(SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(3))
        .synchronous(SqliteSynchronous::Normal)
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
