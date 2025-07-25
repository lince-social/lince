use async_trait::async_trait;
use std::io::Error;

use crate::application::providers::collection::CollectionRow;

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_active(&self) -> Result<CollectionRow, Error>;
    async fn get_inactive(&self) -> Result<Vec<CollectionRow>, Error>;
    async fn set_active(&self, id: &str) -> Result<(), Error>;
}
