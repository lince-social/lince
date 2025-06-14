use crate::domain::{entities::record::Record, repositories::record::RecordRepository};
use std::{io::Error, sync::Arc};

pub struct RecordProvider {
    pub repository: Arc<dyn RecordRepository>,
}

impl RecordProvider {
    pub fn new(repository: std::sync::Arc<dyn RecordRepository>) -> Self {
        Self { repository }
    }
    pub async fn set_quantity(&self, id: u32, quantity: f64) -> Result<(), Error> {
        self.repository.set_quantity(id, quantity).await
    }

    pub async fn get_by_id(&self, id: u32) -> Result<Record, Error> {
        self.repository.get_by_id(id).await
    }
}
