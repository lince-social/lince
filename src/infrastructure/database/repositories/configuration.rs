use crate::{
    application::schema::{
        configuration::row::{ConfigurationForBarScheme, ConfigurationRow},
        view::queried_view::{QueriedView, QueriedViewWithConfigId},
    },
    infrastructure::database::management::lib::connection,
};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn repository_configuration_get_active() -> Result<ConfigurationRow, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let configuration: ConfigurationForBarScheme =
        sqlx::query_as("SELECT id, name, quantity FROM configuration WHERE quantity = 1")
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
pub async fn repository_configuration_get_inactive() -> Result<Vec<ConfigurationRow>, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let configurations: Vec<ConfigurationForBarScheme> =
        sqlx::query_as("SELECT id, name, quantity FROM configuration WHERE quantity <> 1")
            .fetch_all(&pool)
            .await
            .unwrap();

    let views: Vec<QueriedViewWithConfigId> = sqlx::query_as(
        "SELECT cv.configuration_id, v.id, v.name, v.query, cv.quantity
        FROM configuration_view cv
        JOIN view v ON v.id = cv.view_id
        WHERE cv.configuration_id IN (SELECT id FROM configuration WHERE quantity <> 1)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut map = configurations
        .into_iter()
        .map(|c| (c.id, (c, vec![])))
        .collect::<HashMap<_, _>>();
    for v in views {
        if let Some((_, vs)) = map.get_mut(&v.configuration_id) {
            vs.push(QueriedView {
                id: v.id,
                quantity: v.quantity,
                name: v.name,
                query: v.query,
            });
        }
    }
    Ok(map.into_values().collect())
}

pub async fn repository_configuration_set_active(id: String) {
    let pool = connection().await.unwrap();
    let query = format!(
        "UPDATE configuration SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
        id
    );
    let _ = sqlx::query(&query).execute(&pool).await;
}
