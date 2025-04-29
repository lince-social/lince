use super::view::repository_execute_queries;
use crate::{
    domain::entities::{karma::Karma, table::Table},
    infrastructure::database::management::lib::connection,
};
use std::io::{Error, ErrorKind};

pub async fn repository_karma_condition() -> Result<Vec<(String, Table)>, Error> {
    let query = "SELECT * FROM karma_condition";
    repository_execute_queries(vec![query.to_string()]).await
}

pub async fn repository_karma_consequence() -> Result<Vec<(String, Table)>, Error> {
    let query = "SELECT * FROM karma_consequence";
    repository_execute_queries(vec![query.to_string()]).await
}

pub async fn repository_karma_get_joined() -> Result<Vec<(String, Table)>, Error> {
    let query = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0;
            ";

    repository_execute_queries(vec![query.to_string()]).await
}

pub async fn repository_karma_get_deliver() -> Result<Vec<Karma>, Error> {
    let pool = connection().await.unwrap();
    let query = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0;
            ";

    let data: Result<Vec<Karma>, sqlx::Error> = sqlx::query_as(query).fetch_all(&pool).await;
    if data.is_err() {
        return Err(Error::new(
            ErrorKind::InvalidData,
            "Error when querying karma",
        ));
    }
    let data = data.unwrap();
    Ok(data)
    // repository_execute_queries(vec![query.to_string()]).await
}
