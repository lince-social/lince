use crate::{
    domain::entities::collection::Collection,
    infrastructure::database::repositories::view::QueriedView,
};
use async_trait::async_trait;
use std::io::Error;

pub type CollectionRow = (Collection, Vec<QueriedView>);

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_active(&self) -> Result<Option<CollectionRow>, Error>;
    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error>;
    async fn set_active(&self, id: &str) -> Result<(), Error>;
}