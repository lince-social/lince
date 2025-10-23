use crate::{
    domain::clean::collection::Collection,
    infrastructure::database::repositories::view::{QueriedView, QueriedViewWithCollectionId},
};
use async_trait::async_trait;
use sqlx::{Pool, Sqlite};
use std::{collections::HashMap, io::Error, sync::Arc};

pub type CollectionRow = (Collection, Vec<QueriedView>);

// pub fn default_collection_row() -> CollectionRow {}

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_active(&self) -> Result<Option<CollectionRow>, Error>;
    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error>;
    async fn set_active(&self, id: &str) -> Result<(), Error>;
}

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
        let collection: Option<Collection> =
            sqlx::query_as("SELECT id, name, quantity FROM collection WHERE quantity = 1")
                .fetch_optional(&*self.pool)
                .await
                .map_err(Error::other)?;
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
            ",
        )
        .fetch_all(&*self.pool)
        .await
        .unwrap();

        Ok(Some((collection, views)))
    }

    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error> {
        let collections: Vec<Collection> =
            sqlx::query_as("SELECT id, name, quantity FROM collection WHERE quantity <> 1")
                .fetch_all(&*self.pool)
                .await
                .unwrap();

        let views: Vec<QueriedViewWithCollectionId> = sqlx::query_as(
            "SELECT cv.collection_id, v.id, v.name, v.query, cv.quantity
            FROM collection_view cv
            JOIN view v ON v.id = cv.view_id
            WHERE cv.collection_id IN (SELECT id FROM collection WHERE quantity <> 1)",
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
}
