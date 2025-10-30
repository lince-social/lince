use crate::domain::{
    clean::{karma::Karma, karma_condition::KarmaCondition, karma_consequence::KarmaConsequence},
    dirty::karma::{KarmaConditionView, KarmaConsequenceView, KarmaView},
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
    async fn get_condition_view(&self) -> Result<Vec<KarmaConditionView>, Error>;
    async fn get_consequence_view(&self) -> Result<Vec<KarmaConsequenceView>, Error>;
    async fn get_active(&self, condition_record_id: Option<u32>) -> Result<Vec<Karma>, Error>;
    async fn get_condition_tokens(
        &self,
        search: Option<String>,
    ) -> Result<Vec<(u32, String, String, String)>, Error>;
    async fn get_consequence_tokens(
        &self,
        search: Option<String>,
    ) -> Result<Vec<(u32, String, String, String)>, Error>;
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
        let sql = "
            SELECT
                k.id AS karma_id,
                k.name AS karma_name,
                k.quantity AS karma_quantity,

                kcd.condition AS karma_condition_condition,
                CASE
                    WHEN instr(kcd.condition, 'rq') > 0 THEN
                        replace(
                            kcd.condition,
                            'rq' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER) AS TEXT),
                            rcon.head
                        )
                    WHEN instr(kcd.condition, 'c') > 0 THEN
                        replace(
                            kcd.condition,
                            'c' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdcon.name
                        )
                    WHEN instr(kcd.condition, 'f') > 0 THEN
                        replace(
                            kcd.condition,
                            'f' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER) AS TEXT),
                            fcon.name
                        )
                    ELSE kcd.condition
                END AS karma_condition_explanation,
                CASE WHEN instr(kcd.condition, 'rq') > 0 THEN CAST(rcon.quantity AS TEXT) ELSE NULL END AS karma_condition_value,
                kcd.name AS karma_condition_name,
                kcd.quantity AS karma_condition_quantity,
                kcd.id AS karma_condition_id,

                k.operator AS karma_operator,

                kcs.consequence AS karma_consequence_consequence,
                kcs.name AS karma_consequence_name,
                kcs.quantity AS karma_consequence_quantity,
                kcs.id AS karma_consequence_id,

                CASE
                    WHEN instr(kcs.consequence, 'rq') > 0 THEN
                        replace(
                            kcs.consequence,
                            'rq' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER) AS TEXT),
                            rc.head
                        )
                    WHEN instr(kcs.consequence, 'c') > 0 THEN
                        replace(
                            kcs.consequence,
                            'c' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdc.name
                        )
                    WHEN instr(kcs.consequence, 'f') > 0 THEN
                        replace(
                            kcs.consequence,
                            'f' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER) AS TEXT),
                            fcmd.name
                        )
                    ELSE kcs.consequence
                END AS karma_consequence_explanation,
                CASE WHEN instr(kcs.consequence, 'rq') > 0 THEN CAST(rc.quantity AS TEXT) ELSE NULL END AS karma_consequence_value
            FROM karma k
            JOIN karma_condition kcd ON kcd.id = k.condition_id
            JOIN karma_consequence kcs ON kcs.id = k.consequence_id
            LEFT JOIN record rc ON instr(kcs.consequence, 'rq') > 0 AND rc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdc ON instr(kcs.consequence, 'c') > 0 AND cmdc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcmd ON instr(kcs.consequence, 'f') > 0 AND fcmd.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER)
            LEFT JOIN record rcon ON instr(kcd.condition, 'rq') > 0 AND rcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdcon ON instr(kcd.condition, 'c') > 0 AND cmdcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcon ON instr(kcd.condition, 'f') > 0 AND fcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER)
            "
        .to_string();

        let data: Vec<KarmaView> = sqlx::query_as(&sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(data)
    }

    async fn get_condition_view(&self) -> Result<Vec<KarmaConditionView>, Error> {
        let sql =
            "
            SELECT
                CASE
                    WHEN instr(kcd.condition, 'rq') > 0 THEN
                        replace(
                            kcd.condition,
                            'rq' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER) AS TEXT),
                            rcon.head
                        )
                    WHEN instr(kcd.condition, 'c') > 0 THEN
                        replace(
                            kcd.condition,
                            'c' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdcon.name
                        )
                    WHEN instr(kcd.condition, 'f') > 0 THEN
                        replace(
                            kcd.condition,
                            'f' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER) AS TEXT),
                            fcon.name
                        )
                    ELSE kcd.condition
                END AS explanation,
                CASE WHEN instr(kcd.condition, 'rq') > 0 THEN CAST(rcon.quantity AS TEXT) ELSE NULL END AS value,
                kcd.condition AS condition,
                kcd.name AS name,
                kcd.quantity AS quantity,
                kcd.id AS id
            FROM karma_condition kcd
            LEFT JOIN record rcon ON instr(kcd.condition, 'rq') > 0 AND rcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdcon ON instr(kcd.condition, 'c') > 0 AND cmdcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcon ON instr(kcd.condition, 'f') > 0 AND fcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER)
            ";

        let data: Vec<KarmaConditionView> = sqlx::query_as(sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(data)
    }
    async fn get_consequence_view(&self) -> Result<Vec<KarmaConsequenceView>, Error> {
        let sql =
            "
            SELECT
                kcs.id AS id,
                kcs.quantity AS quantity,
                kcs.name AS name,
                kcs.consequence AS consequence,
                CASE
                    WHEN instr(kcs.consequence, 'rq') > 0 THEN
                        replace(
                            kcs.consequence,
                            'rq' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER) AS TEXT),
                            rc.head
                        )
                    WHEN instr(kcs.consequence, 'c') > 0 THEN
                        replace(
                            kcs.consequence,
                            'c' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdc.name
                        )
                    WHEN instr(kcs.consequence, 'f') > 0 THEN
                        replace(
                            kcs.consequence,
                            'f' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER) AS TEXT),
                            fcmd.name
                        )
                    ELSE kcs.consequence
                END AS explanation,
                CASE WHEN instr(kcs.consequence, 'rq') > 0 THEN CAST(rc.quantity AS TEXT) ELSE NULL END AS value
            FROM karma_consequence kcs
            LEFT JOIN record rc ON instr(kcs.consequence, 'rq') > 0 AND rc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdc ON instr(kcs.consequence, 'c') > 0 AND cmdc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcmd ON instr(kcs.consequence, 'f') > 0 AND fcmd.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER)
            ";

        let data: Vec<KarmaConsequenceView> = sqlx::query_as(sql)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        Ok(data)
    }

    async fn get_condition_tokens(
        &self,
        search: Option<String>,
    ) -> Result<Vec<(u32, String, String, String)>, Error> {
        let base = r#"
            SELECT
                kcd.id AS id,
                CASE WHEN instr(kcd.condition, 'rq') > 0 THEN CAST(rcon.quantity AS TEXT) ELSE '' END AS value,
                CASE
                    WHEN instr(kcd.condition, 'rq') > 0 THEN
                        replace(
                            kcd.condition,
                            'rq' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER) AS TEXT),
                            rcon.head
                        )
                    WHEN instr(kcd.condition, 'c') > 0 THEN
                        replace(
                            kcd.condition,
                            'c' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdcon.name
                        )
                    WHEN instr(kcd.condition, 'f') > 0 THEN
                        replace(
                            kcd.condition,
                            'f' || CAST(CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER) AS TEXT),
                            fcon.name
                        )
                    ELSE kcd.condition
                END AS explanation,
                kcd.condition AS raw_condition
            FROM karma_condition kcd
            LEFT JOIN record rcon ON instr(kcd.condition, 'rq') > 0 AND rcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdcon ON instr(kcd.condition, 'c') > 0 AND cmdcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcon ON instr(kcd.condition, 'f') > 0 AND fcon.id = CAST(substr(kcd.condition, instr(kcd.condition, 'f') + 1) AS INTEGER)
            WHERE instr(kcd.condition, 'rq') > 0 OR instr(kcd.condition, 'c') > 0 OR instr(kcd.condition, 'f') > 0
        "#;

        if let Some(s) = search {
            let like = format!("%{}%", s);
            // To allow searching by the raw token text (e.g. "-1 * f2") as well as
            // the replaced explanation (e.g. "-1 * Test"), build a version of the
            // base query that also returns the raw condition text and use it only
            // for filtering. The outer select returns only id,value,explanation so
            // the row mapping remains the same.
            let base_with_raw = base;

            let sql = format!(
                "SELECT id, value, explanation, raw_condition FROM ({}) t WHERE CAST(t.id AS TEXT) LIKE ? OR t.value LIKE ? OR t.explanation LIKE ? OR t.raw_condition LIKE ?",
                base_with_raw
            );
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(&sql)
                .bind(like.clone())
                .bind(like.clone())
                .bind(like.clone())
                .bind(like)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            let data = rows
                .into_iter()
                .filter_map(|(id, value, explanation, raw)| {
                    if id <= 0 {
                        None
                    } else {
                        Some((id as u32, value, explanation, raw))
                    }
                })
                .collect();
            Ok(data)
        } else {
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(base)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            let data = rows
                .into_iter()
                .filter_map(|(id, value, explanation, raw)| {
                    if id <= 0 {
                        None
                    } else {
                        Some((id as u32, value, explanation, raw))
                    }
                })
                .collect();

            Ok(data)
        }
    }

    async fn get_consequence_tokens(
        &self,
        search: Option<String>,
    ) -> Result<Vec<(u32, String, String, String)>, Error> {
        let base = r#"
            SELECT
                kcs.id AS id,
                CASE WHEN instr(kcs.consequence, 'rq') > 0 THEN CAST(rc.quantity AS TEXT) ELSE '' END AS value,
                CASE
                    WHEN instr(kcs.consequence, 'rq') > 0 THEN
                        replace(
                            kcs.consequence,
                            'rq' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER) AS TEXT),
                            rc.head
                        )
                    WHEN instr(kcs.consequence, 'c') > 0 THEN
                        replace(
                            kcs.consequence,
                            'c' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER) AS TEXT),
                            cmdc.name
                        )
                    WHEN instr(kcs.consequence, 'f') > 0 THEN
                        replace(
                            kcs.consequence,
                            'f' || CAST(CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER) AS TEXT),
                            fcmd.name
                        )
                    ELSE kcs.consequence
                END AS explanation,
                kcs.consequence AS raw_consequence
            FROM karma_consequence kcs
            LEFT JOIN record rc ON instr(kcs.consequence, 'rq') > 0 AND rc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'rq') + 2) AS INTEGER)
            LEFT JOIN command cmdc ON instr(kcs.consequence, 'c') > 0 AND cmdc.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'c') + 1) AS INTEGER)
            LEFT JOIN frequency fcmd ON instr(kcs.consequence, 'f') > 0 AND fcmd.id = CAST(substr(kcs.consequence, instr(kcs.consequence, 'f') + 1) AS INTEGER)
            WHERE instr(kcs.consequence, 'rq') > 0 OR instr(kcs.consequence, 'c') > 0 OR instr(kcs.consequence, 'f') > 0
        "#;

        if let Some(s) = search {
            let like = format!("%{}%", s);
            let sql = format!(
                "SELECT id, value, explanation, raw_consequence FROM ({}) t WHERE CAST(t.id AS TEXT) LIKE ? OR t.value LIKE ? OR t.explanation LIKE ? OR t.raw_consequence LIKE ?",
                base
            );
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(&sql)
                .bind(like.clone())
                .bind(like.clone())
                .bind(like.clone())
                .bind(like)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            let data = rows
                .into_iter()
                .filter_map(|(id, value, explanation, raw)| {
                    if id <= 0 {
                        None
                    } else {
                        Some((id as u32, value, explanation, raw))
                    }
                })
                .collect();
            Ok(data)
        } else {
            let rows: Vec<(i64, String, String, String)> = sqlx::query_as(base)
                .fetch_all(&*self.pool)
                .await
                .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

            let data = rows
                .into_iter()
                .filter_map(|(id, value, explanation, raw)| {
                    if id <= 0 {
                        None
                    } else {
                        Some((id as u32, value, explanation, raw))
                    }
                })
                .collect();

            Ok(data)
        }
    }
}
