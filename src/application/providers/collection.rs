use crate::{
    application::schema::collection::row::CollectionRow,
    domain::repositories::collection::CollectionRepository,
};
use std::io::Error;

pub struct CollectionProvider {
    pub repository: std::sync::Arc<dyn CollectionRepository>,
}

impl CollectionProvider {
    pub fn new(repository: std::sync::Arc<dyn CollectionRepository>) -> Self {
        Self { repository }
    }

    pub async fn get_active(&self) -> CollectionRow {
        let (collection, queried_views) = self.repository.get_active().await.unwrap();
        (collection, queried_views)
    }
    pub async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error> {
        self.repository.get_inactive().await
    }
    pub async fn set_active(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
