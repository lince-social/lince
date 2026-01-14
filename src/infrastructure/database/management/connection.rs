use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
};
use std::{io::Error, time::Duration};

pub async fn connection() -> Result<Pool<Sqlite>, Error> {
    let config_dir = dirs::config_dir().unwrap();
    let db_path = format!("{}/lince/lince.db", config_dir.display());

    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(3))
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1) // correct for SQLite
        .acquire_timeout(Duration::from_secs(3))
        .connect_with(options)
        .await
        .map_err(Error::other)?;

    Ok(pool)
}
