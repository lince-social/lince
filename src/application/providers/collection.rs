use crate::{
    application::schema::collection::row::CollectionRow,
    domain::repositories::collection::CollectionRepository,
};
use std::{io::Error, sync::Arc};

pub struct CollectionProvider {
    pub repository: Arc<dyn CollectionRepository>,
}

impl CollectionProvider {
    pub async fn get_active(&self) -> Result<CollectionRow, Error> {
        self.repository.get_active().await
    }
    pub async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error> {
        self.repository.get_inactive().await
    }
    pub async fn set_active(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
