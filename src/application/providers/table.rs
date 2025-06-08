use crate::domain::repositories::table::TableRepository;
use std::io::Error;

pub struct TableProvider {
    pub repository: std::sync::Arc<dyn TableRepository>,
}

impl TableProvider {
    pub fn new(repository: std::sync::Arc<dyn TableRepository>) -> Self {
        Self { repository }
    }

    pub async fn delete_by_id(&self, table: String, id: String) -> Result<(), Error> {
        self.repository.delete_by_id(table, id).await
    }
}
