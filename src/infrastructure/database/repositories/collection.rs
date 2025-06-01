use crate::{
    application::schema::{
        collection::row::{ConfigurationForBarScheme, ConfigurationRow},
        view::queried_view::{QueriedView, QueriedViewWithConfigId},
    },
    infrastructure::database::management::lib::connection,
};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn repository_collection_get_active() -> Result<ConfigurationRow, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let collection: ConfigurationForBarScheme =
        sqlx::query_as("SELECT id, name, quantity FROM collection WHERE quantity = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    let views: Vec<QueriedView> = sqlx::query_as(
        "
        SELECT v.id, cv.quantity, v.name, v.query
        FROM view v
        JOIN collection_view cv ON v.id = cv.view_id
        JOIN collection c ON c.id = cv.collection_id
        WHERE c.quantity = 1
        ",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    Ok((collection, views))
}
pub async fn repository_collection_get_inactive() -> Result<Vec<ConfigurationRow>, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let collections: Vec<ConfigurationForBarScheme> =
        sqlx::query_as("SELECT id, name, quantity FROM collection WHERE quantity <> 1")
            .fetch_all(&pool)
            .await
            .unwrap();

    let views: Vec<QueriedViewWithConfigId> = sqlx::query_as(
        "SELECT cv.collection_id, v.id, v.name, v.query, cv.quantity
        FROM collection_view cv
        JOIN view v ON v.id = cv.view_id
        WHERE cv.collection_id IN (SELECT id FROM collection WHERE quantity <> 1)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut map = collections
        .into_iter()
        .map(|c| (c.id, (c, vec![])))
        .collect::<HashMap<_, _>>();
    for v in views {
        if let Some((_, vs)) = map.get_mut(&v.collection_id) {
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

pub async fn repository_collection_set_active(id: String) {
    let pool = connection().await.unwrap();
    let query = format!(
        "UPDATE collection SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
        id
    );
    let _ = sqlx::query(&query).execute(&pool).await;
}
