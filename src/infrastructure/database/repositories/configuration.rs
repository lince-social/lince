use sqlx::SqlitePool;

use crate::domain::entities::configuration::Configuration;
use std::io::{Error, ErrorKind};

pub async fn get_active() -> Result<Option<Configuration>, Error> {
    let pool = SqlitePool::connect("/home/eduardo/.config/lince/lince.db").await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let configuration: Option<Configuration> =
        sqlx::query_as("SELECT * FROM configuration WHERE quantity = 1")
            .fetch_optional(&pool)
            .await
            .unwrap();

    Ok(configuration)
}

// pub async fn get_inactive() {
//     let conn = connection().await.unwrap();
//     let query = "SELECT * FROM configuration WHERE quantity <> 1";
// }

// pub async fn set_active(id: String) {
//     let conn = connection().await.unwrap();
//     let query =
//         format!("UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END");
// }
