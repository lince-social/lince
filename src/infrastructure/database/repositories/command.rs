// use crate::{
//     domain::entities::command::Command, infrastructure::database::management::lib::connection,
// };
// use std::io::{Error, ErrorKind};

// pub async fn repository_command_get_by_id(id: u32) -> Result<Command, Error> {
//     let pool = connection().await.unwrap();
//     let sql = format!("SELECT * FROM command WHERE id = {}", id);

//     let res: Result<Option<Command>, sqlx::Error> =
//         sqlx::query_as(&sql).fetch_optional(&pool).await;

//     match res {
//         Ok(Some(command)) => Ok(command),
//         Ok(None) => Err(Error::new(
//             ErrorKind::NotFound,
//             format!("No command with id = {}", id),
//         )),
//         Err(_) => Err(Error::new(
//             ErrorKind::InvalidData,
//             format!("Database error at get command by id = {}", id),
//         )),
//     }
// }
use crate::{
    domain::entities::command::Command, infrastructure::database::management::lib::connection,
};
use std::io::{Error, ErrorKind};

pub async fn repository_command_get_by_id(id: u32) -> Result<Command, Error> {
    let pool = connection().await.unwrap();

    let res: Result<Option<Command>, sqlx::Error> =
        sqlx::query_as::<_, Command>("SELECT * FROM command WHERE id = $1")
            .bind(id)
            .fetch_optional(&pool)
            .await;

    match res {
        Ok(Some(command)) => Ok(command),
        Ok(None) => Err(Error::new(
            ErrorKind::NotFound,
            format!("No command with id = {}", id),
        )),
        Err(e) => Err(Error::new(
            ErrorKind::InvalidData,
            format!("Database error at get command by id = {}: {}", id, e),
        )),
    }
}
