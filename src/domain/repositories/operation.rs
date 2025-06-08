use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait OperationRepository: Send + Sync {
    async fn get_column_names(&self, table: String) -> Result<Vec<String>, Error>;
    async fn create(&self, query: String) -> Result<(), Error>;
} 