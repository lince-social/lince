use crate::domain::{entities::table::Table, repositories::view::ViewRepository};
use std::io::Error;

pub struct ViewProvider {
    pub repository: std::sync::Arc<dyn ViewRepository>,
}

impl ViewProvider {
    pub fn new(repository: std::sync::Arc<dyn ViewRepository>) -> Self {
        Self { repository }
    }
    pub async fn toggle_by_view_id(&self, collection_id: u32, view_id: u32) -> Result<(), Error> {
        self.repository
            .toggle_by_view_id(collection_id, view_id)
            .await
    }
    pub async fn toggle_by_collection_id(&self, id: u32) -> Result<(), Error> {
        self.repository.toggle_by_collection_id(id).await
    }
    pub async fn execute_queries(
        &self,
        queries: Vec<String>,
    ) -> Result<Vec<(String, Table)>, Error> {
        self.repository.execute_queries(queries).await
    }
    pub async fn get_active_view_data(&self) -> Result<(Vec<(String, Table)>, Vec<String>), Error> {
        self.repository.get_active_view_data().await
    }
}
