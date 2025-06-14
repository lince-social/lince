use crate::{
    domain::{
        entities::table::{Row as RowEntity, Table},
        repositories::view::ViewRepository,
    },
    infrastructure::database::management::lib::connection,
};
use async_trait::async_trait;
use futures::future::join_all;
use sqlx::{Column, Pool, Row, Sqlite, TypeInfo};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    sync::Arc,
};

pub struct ViewRepositoryImpl {
    pub pool: Arc<Pool<Sqlite>>,
}

impl ViewRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ViewRepository for ViewRepositoryImpl {
    async fn toggle_by_view_id(&self, collection_id: u32, view_id: u32) -> Result<(), Error> {
        let pool = connection().await.unwrap();
        let _ = sqlx::query(&format!(
            "UPDATE collection_view
           SET quantity = CASE
              WHEN quantity = 1 THEN 0
              ELSE 1
            END
           WHERE view_id = {} AND collection_id = {}",
            &view_id, &collection_id
        ))
        .execute(&pool)
        .await;

        Ok(())
    }

    async fn toggle_by_collection_id(&self, collection_id: u32) -> Result<(), Error> {
        sqlx::query(
            "
        UPDATE collection_view
        SET quantity = CASE
            WHEN EXISTS (
                SELECT 1
                FROM collection_view
                WHERE collection_id = $1
                  AND quantity = 1
            )
            THEN 0
            ELSE 1
        END
        WHERE collection_id = $1;
        ",
        )
        .bind(collection_id)
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            Error::other(format!(
                "Failed to toggle view by collection id. Error: {e}"
            ))
        })?;

        Ok(())
    }

    async fn execute_queries(&self, queries: Vec<String>) -> Result<Vec<(String, Table)>, Error> {
        let pool = connection().await.unwrap();

        let task_futures = queries.into_iter().map(|query_string| {
            let table_name = query_string
                .split_whitespace()
                .enumerate()
                .find_map(|(i, word)| {
                    if word.eq_ignore_ascii_case("from") {
                        query_string.split_whitespace().nth(i + 1)
                    } else {
                        None
                    }
                })
                .unwrap_or("unknown_table")
                .to_string();

            let pool = pool.clone();
            async move {
                let rows = sqlx::query(&query_string).fetch_all(&pool).await;
                if rows.is_err() {
                    return Err(Error::new(ErrorKind::InvalidData, "Error when querying"));
                }
                let rows = rows.unwrap();
                let mut result_rows: Table = Vec::with_capacity(rows.len());

                for row in rows {
                    let mut row_map: RowEntity = HashMap::new();
                    for (i, col) in row.columns().iter().enumerate() {
                        let col_name = col.name();
                        let type_name = col.type_info().name().to_uppercase();
                        let value = match type_name.as_str() {
                            "INTEGER" => row
                                .try_get::<i64, _>(i)
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| "NULL".to_string()),
                            "REAL" | "FLOAT" => row
                                .try_get::<f64, _>(i)
                                .map(|v| v.to_string())
                                .unwrap_or_else(|_| "NULL".to_string()),
                            _ => row
                                .try_get::<String, _>(i)
                                .unwrap_or_else(|_| "NULL".to_string()),
                        };
                        row_map.insert(col_name.to_string(), value);
                    }
                    result_rows.push(row_map);
                }

                Ok::<_, Error>((table_name, result_rows))
            }
        });

        let results = join_all(task_futures).await;

        results.into_iter().collect()
    }

    async fn get_active_view_data(&self) -> Result<(Vec<(String, Table)>, Vec<String>), Error> {
        let queries: Vec<String> = sqlx::query_scalar(
            "SELECT v.query AS query
             FROM collection_view cv
             JOIN view v ON cv.view_id = v.id
             JOIN collection c ON cv.collection_id = c.id
             WHERE cv.quantity = 1 AND c.quantity = 1",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error when querying main data. Error: {}", e),
            )
        })?;

        let (special_queries, sql_queries) = queries
            .into_iter()
            .partition(|query| ["karma_orchestra".to_string()].contains(&query));

        let res = self.execute_queries(sql_queries).await.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error when querying main data. {}", e),
            )
        })?;
        Ok((res, special_queries))
    }
}
