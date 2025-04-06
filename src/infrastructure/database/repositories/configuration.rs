use crate::domain::entities::configuration::Configuration;
use std::io::{Error, ErrorKind};

use super::record::connection;

pub async fn repository_configuration_get_active() -> Result<Configuration, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let configuration: Configuration =
        sqlx::query_as("SELECT * FROM configuration WHERE quantity = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    Ok(configuration)
}
pub async fn repository_configuration_get_inactive() -> Result<Vec<Configuration>, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let configuration: Vec<Configuration> =
        sqlx::query_as("SELECT * FROM configuration WHERE quantity <> 1")
            .fetch_all(&pool)
            .await
            .unwrap();

    Ok(configuration)
}

// pub async fn set_active(id: String) {
//     let conn = connection().await.unwrap();
//     let query =
//         format!("UPDATE configuration SET quantity = CASE WHEN id = {id} THEN 1 ELSE 0 END");
// }
