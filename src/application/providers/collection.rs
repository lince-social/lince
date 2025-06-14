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

    pub async fn get(&self) -> Result<Vec<CollectionRow>, Error> {
        self.repository.get().await
    }
    pub async fn set_active(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
