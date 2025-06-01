use crate::{
    domain::entities::collection::Configuration,
    infrastructure::database::management::lib::connection,
};
use std::io::{Error, ErrorKind};

pub async fn repository_configuration_set_active(id: &str) -> Result<(), Error> {
    let pool = connection().await?;

    let query = format!(
        "UPDATE configuration SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
        id
    );

    sqlx::query(&query)
        .execute(&pool)
        .await
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

    Ok(())
}

pub async fn repository_configuration_get_active() -> Result<Configuration, Error> {
    let pool = connection().await?;

    sqlx::query_as("SELECT * FROM configuration WHERE quantity = 1")
        .fetch_one(&pool)
        .await
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))
}
