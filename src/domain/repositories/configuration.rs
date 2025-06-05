use crate::domain::entities::configuration::Configuration;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait ConfigurationRepository: Send + Sync {
    async fn set_active(&self, id: &str) -> Result<(), Error>;
    async fn get_active(&self) -> Result<Configuration, Error>;
}
