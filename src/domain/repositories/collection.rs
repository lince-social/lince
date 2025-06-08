use crate::application::schema::collection::row::ConfigurationRow;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait CollectionRepository: Send + Sync {
    async fn get_active(&self) -> Result<ConfigurationRow, Error>;
    async fn get_inactive(&self) -> Result<Vec<ConfigurationRow>, Error>;
    async fn set_active(&self, id: u32) -> Result<(), Error>;
}
