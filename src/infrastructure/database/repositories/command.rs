use crate::domain::clean::command::Command;
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait CommandRepository: Send + Sync {
    async fn get_by_id(&self, id: u32) -> Result<Option<Command>, Error>;
}

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
    async fn get_by_id(&self, id: u32) -> Result<Option<Command>, Error> {
        sqlx::query_as::<_, Command>("SELECT * FROM command WHERE id = $1")
            .bind(id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| {
                Error::new(
                    ErrorKind::InvalidData,
                    format!("Database error at get command by id = {}: {}", id, e),
                )
            })
    }
}
