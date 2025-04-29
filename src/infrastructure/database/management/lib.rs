use sqlx::{Pool, Sqlite, sqlite};
use std::io::{Error, ErrorKind};

pub async fn connection() -> Result<Pool<Sqlite>, Error> {
    let config_dir = dirs::config_dir().unwrap();
    let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
    let opt = sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = sqlite::SqlitePool::connect_with(opt).await;
    if pool.is_err() {
        return Err(Error::new(ErrorKind::Other, "Pool error"));
    }
    let pool = pool.unwrap();
    Ok(pool)
}
