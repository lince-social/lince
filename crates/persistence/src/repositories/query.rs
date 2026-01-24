use async_trait::async_trait;
use domain::dirty::operation::Query;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait QueryRepository: Send + Sync {
    async fn get_by_id(&self, id: u32) -> Result<Query, Error>;
    async fn execute(&self, sql: &str) -> Result<(), Error>;
}

pub struct QueryRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl QueryRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl QueryRepository for QueryRepositoryImpl {
    async fn get_by_id(&self, id: u32) -> Result<Query, Error> {
        let sql = format!("SELECT query FROM query WHERE id = {}", id);

        let res: Result<Option<Query>, sqlx::Error> = sqlx::query_as::<_, Query>(&sql)
            .fetch_optional(&*self.pool)
            .await;

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

    async fn execute(&self, sql: &str) -> Result<(), Error> {
        sqlx::query(sql).execute(&*self.pool).await.map_err(|e| {
            Error::new(
                ErrorKind::InvalidInput,
                format!("Failed to run query: {}. Error: {}", sql, e),
            )
        })?;

        Ok(())
    }
}
