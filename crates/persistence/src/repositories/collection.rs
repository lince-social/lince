use async_trait::async_trait;
use domain::{
    clean::{
        collection::Collection,
        table::{Row as RowEntity, Table},
    },
    dirty::{collection::CollectionRow, view::QueriedView},
};
use futures::future::join_all;
use sqlx::{Column, Pool, Row, Sqlite, TypeInfo};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    iter::once,
    sync::Arc,
};
use utils::ok;

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_all(&self) -> Result<Vec<CollectionRow>, Error>;
    async fn get_active(&self) -> Result<Option<CollectionRow>, Error>;
    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error>;
    async fn set_active(&self, id: &str) -> Result<(), Error>;

    async fn toggle_by_view_id(
        &self,
        collection_id: u32,
        view_id: u32,
    ) -> Result<Vec<CollectionRow>, Error>;
    async fn toggle_by_collection_id(&self, id: u32) -> Result<(), Error>;
    async fn execute_queries(&self, queries: Vec<String>) -> Result<Vec<(String, Table)>, Error>;
    async fn get_active_view_data(&self) -> Result<(Vec<(String, Table)>, Vec<String>), Error>;
    
    // New methods for pinned views
    async fn get_pinned_views(&self) -> Result<Vec<QueriedView>, Error>;
    async fn get_pinned_view_data(&self) -> Result<Vec<(String, Table)>, Error>;
    async fn pin_view(&self, view_id: u32, position_x: f64, position_y: f64) -> Result<(), Error>;
    async fn unpin_view(&self, view_id: u32) -> Result<(), Error>;
    async fn update_view_position(&self, view_id: u32, position_x: f64, position_y: f64) -> Result<(), Error>;
}

pub struct CollectionRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl CollectionRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[derive(sqlx::FromRow)]
pub struct QueriedViewWithCollectionId {
    pub collection_id: u32,
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
    pub pinned: i32,
    pub position_x: Option<f64>,
    pub position_y: Option<f64>,
    pub z_index: i32,
}

#[async_trait]
impl CollectionRepository for CollectionRepositoryImpl {
    async fn get_all(&self) -> Result<Vec<CollectionRow>, Error> {
        let mut inactive = self.get_inactive().await?;
        inactive.sort_by_key(|(collection, _)| collection.id);
        Ok(once(self.get_active().await?.unwrap())
            .chain(inactive)
            .collect())
    }

    async fn set_active(&self, id: &str) -> Result<(), Error> {
        sqlx::query(&format!(
            "UPDATE collection SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
            id
        ))
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        Ok(())
    }

    async fn get_active(&self) -> Result<Option<CollectionRow>, Error> {
        let collection: Option<Collection> = ok!(sqlx::query_as(
            "SELECT id, name, quantity FROM collection WHERE quantity = 1"
        )
        .fetch_optional(&*self.pool)
        .await);
        if collection.is_none() {
            return Ok(None);
        }
        let collection = collection.unwrap();

        let views: Vec<QueriedView> = sqlx::query_as(
            "
            SELECT v.id, cv.quantity, v.name, v.query, v.pinned, v.position_x, v.position_y, v.z_index
            FROM view v
            JOIN collection_view cv ON v.id = cv.view_id
            JOIN collection c ON c.id = cv.collection_id
            WHERE c.quantity = 1
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .unwrap();

        Ok(Some((collection, views)))
    }

    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error> {
        let collections: Vec<Collection> = sqlx::query_as(
            "SELECT id, name, quantity FROM collection WHERE quantity <> 1 ORDER BY id",
        )
        .fetch_all(&*self.pool)
        .await
        .unwrap();

        let views: Vec<QueriedViewWithCollectionId> = sqlx::query_as(
            "SELECT cv.collection_id, v.id, v.name, v.query, cv.quantity, v.pinned, v.position_x, v.position_y, v.z_index
            FROM collection_view cv
            JOIN view v ON v.id = cv.view_id
            WHERE cv.collection_id IN (SELECT id FROM collection WHERE quantity <> 1)
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .unwrap();

        let mut map = collections
            .into_iter()
            .map(|c| (c.id, (c, vec![])))
            .collect::<HashMap<_, _>>();
        for v in views {
            if let Some((_, vs)) = map.get_mut(&v.collection_id) {
                vs.push(QueriedView {
                    id: v.id,
                    quantity: v.quantity,
                    name: v.name,
                    query: v.query,
                    pinned: v.pinned,
                    position_x: v.position_x,
                    position_y: v.position_y,
                    z_index: v.z_index,
                });
            }
        }
        Ok(map.into_values().collect())
    }

    async fn toggle_by_view_id(
        &self,
        collection_id: u32,
        view_id: u32,
    ) -> Result<Vec<CollectionRow>, Error> {
        let _ = sqlx::query(&format!(
            "UPDATE collection_view
           SET quantity = CASE
              WHEN quantity = 1 THEN 0
              ELSE 1
            END
           WHERE view_id = {} AND collection_id = {}",
            &view_id, &collection_id
        ))
        .execute(&*self.pool)
        .await;

        self.get_all().await
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

            async move {
                let rows = sqlx::query(&query_string).fetch_all(&*self.pool).await;
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

        let (special_queries, sql_queries) = queries.into_iter().partition(|query| {
            [
                "karma_orchestra".to_string(),
                "karma_view".to_string(),
                "testing".to_string(),
            ]
            .contains(query)
        });

        let res = self.execute_queries(sql_queries).await.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error when querying main data. {}", e),
            )
        })?;
        Ok((res, special_queries))
    }

    async fn get_pinned_views(&self) -> Result<Vec<QueriedView>, Error> {
        let views: Vec<QueriedView> = sqlx::query_as(
            "
            SELECT v.id, 1 as quantity, v.name, v.query, v.pinned, v.position_x, v.position_y, v.z_index
            FROM view v
            WHERE v.pinned = 1
            ORDER BY v.z_index DESC
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(Error::other)?;
        
        Ok(views)
    }

    async fn get_pinned_view_data(&self) -> Result<Vec<(String, Table)>, Error> {
        let queries: Vec<String> = sqlx::query_scalar(
            "SELECT v.query FROM view v WHERE v.pinned = 1"
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(Error::other)?;

        self.execute_queries(queries).await
    }

    async fn pin_view(&self, view_id: u32, position_x: f64, position_y: f64) -> Result<(), Error> {
        // Get max z_index and increment it
        let max_z_index: Option<i32> = sqlx::query_scalar("SELECT MAX(z_index) FROM view WHERE pinned = 1")
            .fetch_optional(&*self.pool)
            .await
            .map_err(Error::other)?
            .flatten();
        
        let new_z_index = max_z_index.unwrap_or(0) + 1;
        
        sqlx::query(
            "UPDATE view SET pinned = 1, position_x = ?, position_y = ?, z_index = ? WHERE id = ?"
        )
        .bind(position_x)
        .bind(position_y)
        .bind(new_z_index)
        .bind(view_id)
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        
        Ok(())
    }

    async fn unpin_view(&self, view_id: u32) -> Result<(), Error> {
        sqlx::query(
            "UPDATE view SET pinned = 0, position_x = NULL, position_y = NULL, z_index = 0 WHERE id = ?"
        )
        .bind(view_id)
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        
        Ok(())
    }

    async fn update_view_position(&self, view_id: u32, position_x: f64, position_y: f64) -> Result<(), Error> {
        sqlx::query(
            "UPDATE view SET position_x = ?, position_y = ? WHERE id = ?"
        )
        .bind(position_x)
        .bind(position_y)
        .bind(view_id)
        .execute(&*self.pool)
        .await
        .map_err(Error::other)?;
        
        Ok(())
    }
}
