use sqlx::{Pool, Sqlite, sqlite};
use std::io::Error;

pub async fn connection() -> Result<Pool<Sqlite>, Error> {
    let config_dir = dirs::config_dir().unwrap();

    let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";

    let opt = sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);

    let pool = sqlite::SqlitePool::connect_with(opt)
        .await
        .map_err(Error::other)?;

    Ok(pool)
}
