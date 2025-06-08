use crate::domain::entities::operation::Query;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait QueryRepository: Send + Sync {
    async fn get_by_id(&self, id: u32) -> Result<Query, Error>;
    async fn execute(&self, sql: String) -> Result<(), Error>;
} 