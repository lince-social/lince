use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait TableRepository: Send + Sync {
    async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error>;
} 