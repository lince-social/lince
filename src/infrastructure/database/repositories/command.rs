use crate::domain::{entities::command::Command, repositories::command::CommandRepository};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

pub struct CommandRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl CommandRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CommandRepository for CommandRepositoryImpl {
    async fn get_by_id(&self, id: u32) -> Result<Command, Error> {
        let res: Result<Option<Command>, sqlx::Error> =
            sqlx::query_as::<_, Command>("SELECT * FROM command WHERE id = $1")
                .bind(id)
                .fetch_optional(&*self.pool)
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
}
