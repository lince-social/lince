use crate::{
    domain::{entities::collection::Collection, repositories::collection::CollectionRepository},
    infrastructure::database::repositories::view::QueriedView,
};
use std::{io::Error, sync::Arc};

pub type CollectionRow = (Collection, Vec<QueriedView>);

pub struct CollectionProvider {
    pub repository: Arc<dyn CollectionRepository>,
}

impl CollectionProvider {
    pub async fn get_active(&self) -> Result<Option<CollectionRow>, Error> {
        self.repository.get_active().await
    }
    pub async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error> {
        self.repository.get_inactive().await
    }
    pub async fn set_active(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
