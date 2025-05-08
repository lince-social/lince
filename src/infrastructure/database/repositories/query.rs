use crate::{
    domain::entities::operation::Query, infrastructure::database::management::lib::connection,
};
use std::io::{Error, ErrorKind};

pub async fn repository_query_get_by_id(id: u32) -> Result<Query, Error> {
    let pool = connection().await.unwrap();
    let sql = format!("SELECT query FROM query WHERE id = {}", id);

    let res: Result<Option<Query>, sqlx::Error> =
        sqlx::query_as::<_, Query>(&sql).fetch_optional(&pool).await;

    match res {
        Ok(Some(query)) => Ok(query),
        Ok(None) => Err(Error::new(
            ErrorKind::NotFound,
            format!("No query with id = {}", id),
        )),
        Err(e) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Database error at get query by id = {}: {}", id, e),
        )),
    }
}
pub async fn repository_query_execute(sql: String) -> Result<(), Error> {
    let pool = connection().await.unwrap();
    let res = sqlx::query(&sql).execute(&pool).await;
    match res {
        Ok(_) => Ok(()),
        Err(error) => Err(Error::new(
            ErrorKind::InvalidInput,
            format!("Failed to run query: {}. Error: {}", sql, error),
        )),
    }
}
