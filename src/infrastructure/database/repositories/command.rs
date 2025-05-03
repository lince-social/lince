use crate::infrastructure::database::management::lib::connection;
use std::io::{Error, ErrorKind};

pub async fn repository_command_get_by_id(id: u32) -> Result<String, Error> {
    let pool = connection().await.unwrap();
    let sql = format!("SELECT command FROM command WHERE id = {}", id);

    let res: Result<Option<String>, sqlx::Error> = sqlx::query_scalar(&sql)
        .bind(id.clone())
        .fetch_optional(&pool)
        .await;

    match res {
        Ok(Some(command)) => Ok(command),
        Ok(None) => Err(Error::new(
            ErrorKind::NotFound,
            format!("No command with id = {}", id),
        )),
        Err(_) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Database error at get command by id = {}", id),
        )),
    }
}
