use crate::domain::{entities::operation::Query, repositories::query::QueryRepository};
use std::io::Error;

pub struct QueryProvider {
    pub repository: std::sync::Arc<dyn QueryRepository>,
}

impl QueryProvider {
    pub async fn get_by_id(&self, id: u32) -> Result<Query, Error> {
        self.repository.get_by_id(id).await
    }

    pub async fn execute(&self, sql: &str) -> Result<(), Error> {
        self.repository.execute(sql).await
    }
}
