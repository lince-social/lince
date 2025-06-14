use async_trait::async_trait;
use sqlx::{Pool, Sqlite};

use crate::{
    application::schema::{
        collection::row::CollectionRow,
        view::queried_view::{QueriedView, QueriedViewWithCollectionId},
    },
    domain::{entities::collection::Collection, repositories::collection::CollectionRepository},
};
use std::{collections::HashMap, io::Error, sync::Arc};
pub struct CollectionRepositoryImpl {
    pool: Arc<Pool<Sqlite>>,
}

impl CollectionRepositoryImpl {
    pub fn new(pool: Arc<Pool<Sqlite>>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CollectionRepository for CollectionRepositoryImpl {
    async fn get(&self) -> Result<Vec<CollectionRow>, Error> {
        let collections: Vec<Collection> =
            sqlx::query_as("SELECT * FROM collection ORDER BY quantity DESC, name ASC")
                .fetch_all(&*self.pool)
                .await
                .map_err(Error::other)?;

        let views: Vec<QueriedViewWithCollectionId> = sqlx::query_as(
            r#"
            SELECT cv.collection_id, v.id, v.name, v.query, cv.quantity
            FROM collection_view cv
            JOIN view v ON v.id = cv.view_id
            ORDER BY v.name ASC
            "#,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(Error::other)?;

        let mut grouped = HashMap::with_capacity(collections.len());
        for v in views {
            grouped
                .entry(v.collection_id)
                .or_insert_with(Vec::new)
                .push(QueriedView {
                    id: v.id,
                    name: v.name,
                    query: v.query,
                    quantity: v.quantity,
                });
        }

        let result = collections
            .into_iter()
            .map(|c| {
                let views = grouped.remove(&c.id).unwrap_or_default();
                (c, views)
            })
            .collect::<Vec<_>>();

        Ok(result)
    }

    async fn set_active(&self, id: &str) -> Result<(), Error> {
        sqlx::query(&format!(
            "UPDATE collection SET quantity = CASE WHEN id = {} THEN 1 ELSE 0 END",
            id
        ))
        .execute(&*self.pool)
        .await
        .map_err(|e| Error::other(e))?;
        Ok(())
    }
}
