use async_trait::async_trait;
use domain::{
    clean::{
        collection::Collection,
        table::{Row as RowEntity, Table},
    },
    dirty::{collection::CollectionRow, operation::DatabaseTable, view::QueriedView},
};
use futures::future::join_all;
use sqlx::{Column, Pool, Row, Sqlite, TypeInfo};
use std::{
    collections::HashMap,
    io::{Error, ErrorKind},
    iter::once,
    str::FromStr,
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
    async fn get_active_view_data(&self)
    -> Result<(Vec<(u32, String, Table)>, Vec<String>), Error>;
    async fn get_all_collection_view_column_widths(
        &self,
    ) -> Result<HashMap<u32, HashMap<String, f32>>, Error>;
    async fn update_collection_view_column_widths(
        &self,
        collection_view_id: u32,
        widths: HashMap<String, f32>,
    ) -> Result<(), Error>;
}

pub struct CollectionRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl CollectionRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

fn is_special_query(query: &str) -> bool {
    if parse_creation_view_query(query).is_some() {
        return true;
    }
    matches!(
        query,
        "karma_orchestra" | "karma_view" | "testing" | "command_buffer"
    )
}

fn parse_creation_view_query(query: &str) -> Option<DatabaseTable> {
    let normalized = query.trim().to_lowercase().replace(['-', ' '], "_");
    let table_name = normalized
        .strip_prefix("create_view_")
        .or_else(|| normalized.strip_prefix("creation_view_"))
        .or_else(|| normalized.strip_prefix("create_modal_"))
        .or_else(|| normalized.strip_prefix("creation_modal_"))
        .or_else(|| normalized.strip_prefix("cv_"))?;
    DatabaseTable::from_str(table_name).ok()
}

#[derive(sqlx::FromRow)]
pub struct QueriedViewWithCollectionId {
    pub collection_id: u32,
    pub id: u32,
    pub quantity: i32,
    pub name: String,
    pub query: String,
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
            SELECT v.id, cv.quantity, v.name, v.query
            FROM view v
            JOIN collection_view cv ON v.id = cv.view_id
            JOIN collection c ON c.id = cv.collection_id
            WHERE c.quantity = 1
            ORDER BY cv.id
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
            "SELECT cv.collection_id, v.id, v.name, v.query, cv.quantity
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

    async fn get_active_view_data(
        &self,
    ) -> Result<(Vec<(u32, String, Table)>, Vec<String>), Error> {
        let query_rows: Vec<(u32, String)> = sqlx::query_as(
            "SELECT cv.id AS collection_view_id, v.query AS query
             FROM collection_view cv
             JOIN view v ON cv.view_id = v.id
             JOIN collection c ON cv.collection_id = c.id
             WHERE cv.quantity = 1 AND c.quantity = 1
             ORDER BY cv.id",
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error when querying main data. Error: {}", e),
            )
        })?;

        let (special_rows, sql_rows): (Vec<_>, Vec<_>) = query_rows
            .into_iter()
            .partition(|(_, query)| is_special_query(query.as_str()));
        let special_queries = special_rows
            .into_iter()
            .map(|(_, query)| query)
            .collect::<Vec<_>>();
        let sql_queries = sql_rows
            .iter()
            .map(|(_, query)| query.clone())
            .collect::<Vec<_>>();

        let queried_tables = self.execute_queries(sql_queries).await.map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!("Error when querying main data. {}", e),
            )
        })?;
        let tables = sql_rows
            .into_iter()
            .zip(queried_tables)
            .map(|((collection_view_id, _), (name, table))| (collection_view_id, name, table))
            .collect::<Vec<_>>();

        Ok((tables, special_queries))
    }

    async fn get_all_collection_view_column_widths(
        &self,
    ) -> Result<HashMap<u32, HashMap<String, f32>>, Error> {
        let rows: Vec<(u32, String)> =
            sqlx::query_as("SELECT id, column_sizes FROM collection_view")
                .fetch_all(&*self.pool)
                .await
                .map_err(Error::other)?;

        let widths = rows
            .into_iter()
            .map(|(collection_view_id, widths_json)| {
                let parsed =
                    serde_json::from_str::<HashMap<String, f32>>(&widths_json).unwrap_or_default();
                (collection_view_id, parsed)
            })
            .collect::<HashMap<_, _>>();
        Ok(widths)
    }

    async fn update_collection_view_column_widths(
        &self,
        collection_view_id: u32,
        widths: HashMap<String, f32>,
    ) -> Result<(), Error> {
        let widths_json = serde_json::to_string(&widths).map_err(Error::other)?;
        sqlx::query("UPDATE collection_view SET column_sizes = ? WHERE id = ?")
            .bind(widths_json)
            .bind(collection_view_id)
            .execute(&*self.pool)
            .await
            .map_err(Error::other)?;

        Ok(())
    }
}
