use crate::domain::{
    entities::{
        karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence,
    },
    repositories::karma::KarmaRepository,
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
    async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error> {
        sqlx::query_as("SELECT * FROM karma_condition")
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }

    async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error> {
        sqlx::query_as("SELECT * FROM karma_consequence")
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))
    }

    async fn get(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error> {
        let mut sql = "
            SELECT
                k.id,
                k.quantity,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id"
            .to_string();
        // WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0

        if let Some(record_id) = condition_record_id {
            sql.push_str(&format!(" AND kcd.condition LIKE \"%{record_id}%\""));
        }

        sql.push(';');

        let data: Vec<Karma> = sqlx::query_as(&sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(data)
    }
}
