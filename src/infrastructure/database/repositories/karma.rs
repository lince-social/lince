use crate::domain::{
    clean::{karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence},
    dirty::karma::KarmaView,
};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

#[async_trait]
pub trait KarmaRepository: Send + Sync {
    async fn get_condition(&self) -> Result<Vec<KarmaCondition>, Error>;
    async fn get_consequence(&self) -> Result<Vec<KarmaConsequence>, Error>;
    async fn get(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error>;
    async fn get_view(&self) -> Result<Vec<KarmaView>, Error>;
    async fn get_active(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error>;
}

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

    async fn get_active(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error> {
        let mut sql = "
            SELECT
                k.id,
                k.quantity,
                k.name,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            WHERE k.quantity > 0 AND kcd.quantity > 0 AND kcs.quantity > 0
            "
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

    async fn get(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error> {
        let mut sql = "
            SELECT
                k.id,
                k.quantity,
                k.name,
                kcd.condition,
                k.operator,
                kcs.consequence
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            "
        .to_string();

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
    async fn get_view(&self) -> Result<Vec<KarmaView>, Error> {
        // Query
        // vec Karmaview
        // regex de:
        //  condition name
        // consequence name
        // record head
        // record quantity
        // command name
        let sql = "
            SELECT
                k.id AS karma_id,
                k.name AS karma_name,
                k.quantity AS karma_quantity,

                kcd.condition AS karma_condition_condition,
                -- condition: explanation = record.head when rq<id> present, otherwise the condition text
                COALESCE(rcon.head, kcd.condition) AS karma_condition_explanation,
                -- condition value: record.quantity when rq present, otherwise NULL (maps to Option<String>)
                CAST(rcon.quantity AS TEXT) AS karma_condition_value,
                kcd.name AS karma_condition_name,
                kcd.quantity AS karma_condition_quantity,
                kcd.id AS karma_condition_id,

                k.operator AS karma_operator,

                kcs.consequence AS karma_consequence_consequence,
                kcs.name AS karma_consequence_name,
                kcs.quantity AS karma_consequence_quantity,
                kcs.id AS karma_consequence_id,

                -- consequence: explanation = record.head when rq<id> present, otherwise the consequence text
                COALESCE(rc.head, kcs.consequence) AS karma_consequence_explanation,
                -- consequence value: record.quantity when rq present, otherwise NULL (maps to Option<String>)
                CAST(rc.quantity AS TEXT) AS karma_consequence_value
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            LEFT JOIN record rc ON instr(kcs.consequence, 'rq') > 0 AND rc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER)
            LEFT JOIN record rcon ON instr(kcd.condition, 'rq') > 0 AND rcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER)
            "
        .to_string();

        let data: Vec<KarmaView> = sqlx::query_as(&sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(data)
    }
}
//     pub karma_id: u32,
//     pub karma_name: String,
//     pub karma_quantity: i32,

//     pub karma_condition_condition: String,
//     pub karma_condition_condition_value: String,
//     pub karma_condition_condition_explanation: String,
//     pub karma_condition_name: String,
//     pub karma_condition_quantity: i32,
//     pub karma_condition_id: u32,

//     pub karma_operator: String,

//     pub karma_consequence_id: u32,
//     pub karma_consequence_quantity: i32,
//     pub karma_consequence_name: String,
//     pub karma_consequence_consequence: String,
//     pub karma_consequence_consequque_value: String,
//     pub karma_consequence_consequence_explanation: String,
// }

// #[derive(FromRow, Deserialize, Serialize, Debug, Clone)]
// pub struct KarmaCondition {
//     pub id: u32,
//     pub quantity: i32,
//     pub name: String,
//     pub condition: String,
// }

// #[derive(FromRow, Deserialize, Serialize, Debug, Clone)]
// pub struct KarmaConsequence {
//     pub id: u32,
//     pub quantity: i32,
//     pub name: String,
//     pub consequence: String,
// }
