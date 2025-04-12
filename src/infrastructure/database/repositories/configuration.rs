use crate::{
    application::schema::view::queried_view::QueriedView,
    domain::entities::configuration::Configuration,
};
use std::io::{Error, ErrorKind};

use super::record::connection;

pub async fn repository_configuration_get_active()
-> Result<(Configuration, Vec<QueriedView>), Error> {
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

    let views: Vec<QueriedView> = sqlx::query_as(
        "
        SELECT v.id, cv.quantity, v.name, v.query
        FROM view v
        JOIN configuration_view cv ON v.id = cv.view_id
        JOIN configuration c ON c.id = cv.configuration_id
        WHERE c.quantity = 1
        ",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    Ok((configuration, views))
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

pub async fn repository_configuration_set_active(id: String) {
    let pool = connection().await.unwrap();
    let query = format!(
        "UPDATE configuration SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
        id
    );
    let _ = sqlx::query(&query).execute(&pool).await;
}
