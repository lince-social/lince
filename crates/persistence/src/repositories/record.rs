use async_trait::async_trait;
use domain::clean::record::Record;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait RecordRepository: Send + Sync {
    async fn set_quantity(&self, id: u32, quantity: f64) -> Result<(), Error>;
    async fn get_by_id(&self, id: u32) -> Result<Record, Error>;
}

pub struct RecordRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl RecordRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RecordRepository for RecordRepositoryImpl {
    async fn set_quantity(&self, id: u32, quantity: f64) -> Result<(), Error> {
        match sqlx::query(&format!(
            "UPDATE record SET quantity = {} WHERE id = {}",
            quantity, id
        ))
        .execute(&*self.pool)
        .await
        {
            Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
            _ => Ok(()),
        }
    }

    async fn get_by_id(&self, id: u32) -> Result<Record, Error> {
        sqlx::query_as(&format!("SELECT * FROM record WHERE id = {}", id))
            .fetch_one(&*self.pool)
            .await
            .map_err(|_| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("No record with id: {} found", id),
                )
            })
    }
}
