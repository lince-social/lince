use crate::domain::entities::{karma::Karma, table::Table};
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait KarmaRepository: Send + Sync {
    async fn get_condition(&self) -> Result<Vec<(String, Table)>, Error>;
    async fn get_consequence(&self) -> Result<Vec<(String, Table)>, Error>;
    async fn get_joined(&self) -> Result<Vec<(String, Table)>, Error>;
    async fn get(&self) -> Result<Vec<Karma>, Error>;
}
