use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait RecordRepository: Send + Sync {
    async fn set_quantity(&self, id: u32, quantity: f64) -> Result<(), Error>;
    async fn get_quantity_by_id(&self, id: u32) -> Result<String, Error>;
    async fn get_head_by_id(&self, id: u32) -> Result<String, Error>;
}
