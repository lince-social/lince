use crate::domain::entities::table::Table;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait ViewRepository: Send + Sync {
    async fn toggle_by_view_id(&self, collection_id: u32, view_id: u32) -> Result<(), Error>;
    async fn toggle_by_collection_id(&self, id: u32) -> Result<(), Error>;
    async fn execute_queries(&self, queries: Vec<String>) -> Result<Vec<(String, Table)>, Error>;
    async fn get_active_view_data(&self) -> Result<(Vec<(String, Table)>, Vec<String>), Error>;
}
