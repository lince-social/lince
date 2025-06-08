use crate::{
    domain::{
        entities::{karma::Karma, table::Table},
        repositories::karma::KarmaRepository,
    },
};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

pub struct KarmaRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl KarmaRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl KarmaRepository for KarmaRepositoryImpl {
    async fn get_condition(&self) -> Result<Vec<(String, Table)>, Error> {
        let query = "SELECT * FROM karma_condition";
        let result: Vec<(String, Table)> = sqlx::query_as(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(result)
    }

    async fn get_consequence(&self) -> Result<Vec<(String, Table)>, Error> {
        let query = "SELECT * FROM karma_consequence";
        let result: Vec<(String, Table)> = sqlx::query_as(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(result)
    }

    async fn get_joined(&self) -> Result<Vec<(String, Table)>, Error> {
        let query = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0;
            ";
        let result: Vec<(String, Table)> = sqlx::query_as(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(result)
    }

    async fn get_deliver(&self) -> Result<Vec<Karma>, Error> {
        let query = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0;
            ";

        let data: Vec<Karma> = sqlx::query_as(query)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;
        Ok(data)
    }
}
