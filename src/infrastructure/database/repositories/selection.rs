use crate::{
    application::schema::{
        selection::row::{ConfigurationForBarScheme, ConfigurationRow},
        view::queried_view::{QueriedView, QueriedViewWithConfigId},
    },
    infrastructure::database::management::lib::connection,
};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
};

pub async fn repository_selection_get_active() -> Result<ConfigurationRow, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let selection: ConfigurationForBarScheme =
        sqlx::query_as("SELECT id, name, quantity FROM selection WHERE quantity = 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    let views: Vec<QueriedView> = sqlx::query_as(
        "
        SELECT v.id, cv.quantity, v.name, v.query
        FROM view v
        JOIN selection_view cv ON v.id = cv.view_id
        JOIN selection c ON c.id = cv.selection_id
        WHERE c.quantity = 1
        ",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    Ok((selection, views))
}
pub async fn repository_selection_get_inactive() -> Result<Vec<ConfigurationRow>, Error> {
    let pool = connection().await;
    if pool.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error connecting to database",
        ));
    }
    let pool = pool.unwrap();

    let selections: Vec<ConfigurationForBarScheme> =
        sqlx::query_as("SELECT id, name, quantity FROM selection WHERE quantity <> 1")
            .fetch_all(&pool)
            .await
            .unwrap();

    let views: Vec<QueriedViewWithConfigId> = sqlx::query_as(
        "SELECT cv.selection_id, v.id, v.name, v.query, cv.quantity
        FROM selection_view cv
        JOIN view v ON v.id = cv.view_id
        WHERE cv.selection_id IN (SELECT id FROM selection WHERE quantity <> 1)",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut map = selections
        .into_iter()
        .map(|c| (c.id, (c, vec![])))
        .collect::<HashMap<_, _>>();
    for v in views {
        if let Some((_, vs)) = map.get_mut(&v.selection_id) {
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

pub async fn repository_selection_set_active(id: String) {
    let pool = connection().await.unwrap();
    let query = format!(
        "UPDATE selection SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
        id
    );
    let _ = sqlx::query(&query).execute(&pool).await;
}
