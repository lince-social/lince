use crate::domain::entities::command::Command;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait CommandRepository: Send + Sync {
    async fn get_by_id(&self, id: &str) -> Result<Option<Command>, Error>;
}
