use crate::{
    application::schema::record::{record_head::RecordHead, record_quantity::RecordQuantity},
    infrastructure::database::management::lib::connection,
    domain::repositories::record::RecordRepository,
};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

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
        let query = format!(
            "UPDATE record SET quantity = {} WHERE id = {}",
            quantity, id
        );

        match sqlx::query(&query).execute(&*self.pool).await {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(ErrorKind::InvalidData, e)),
        }
    }

    async fn get_quantity_by_id(&self, id: u32) -> Result<f64, Error> {
        let record: Result<_, sqlx::Error> =
            sqlx::query_as(&format!("SELECT quantity FROM record WHERE id = {}", id))
                .fetch_one(&*self.pool)
                .await;
        if record.is_err() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("No record with id: {} found", id),
            ));
        }
        let record = record.unwrap();

        Ok(record.quantity)
    }

    async fn get_head_by_id(&self, id: u32) -> Result<String, Error> {
        let record: Result<_, sqlx::Error> =
            sqlx::query_as(&format!("SELECT head FROM record WHERE id = {}", id))
                .fetch_one(&*self.pool)
                .await;
        if record.is_err() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("No record with id: {} found", id),
            ));
        }
        let record = record.unwrap();

        Ok(record.head)
    }
}
