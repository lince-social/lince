use async_trait::async_trait;
use domain::clean::frequency::Frequency;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait FrequencyRepository: Send + Sync {
    async fn get(&self, id: u32) -> Result<Option<Frequency>, Error>;
    async fn update(&self, frequency: Frequency) -> Result<(), Error>;
}

pub struct FrequencyRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl FrequencyRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FrequencyRepository for FrequencyRepositoryImpl {
    async fn get(&self, id: u32) -> Result<Option<Frequency>, Error> {
        let query = format!(
            "SELECT * FROM frequency WHERE id = {} and quantity <> 0",
            id
        );
        sqlx::query_as(&query)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }

    async fn update(&self, frequency: Frequency) -> Result<(), Error> {
        sqlx::query(&format!(
            "UPDATE frequency SET quantity = {}, next_date = '{}' WHERE id = {}",
            frequency.quantity, frequency.next_date, frequency.id
        ))
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(())
    }
}
