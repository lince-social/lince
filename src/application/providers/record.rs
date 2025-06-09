use crate::domain::repositories::record::RecordRepository;
use std::io::Error;

pub struct RecordProvider {
    pub repository: std::sync::Arc<dyn RecordRepository>,
}

impl RecordProvider {
    pub fn new(repository: std::sync::Arc<dyn RecordRepository>) -> Self {
        Self { repository }
    }
    pub async fn set_quantity(&self, id: u32, quantity: f64) -> Result<(), Error> {
        self.repository.set_quantity(id, quantity).await
    }

    pub fn set_quantity_sync(&self, id: u32, quantity: f64) -> Result<(), Error> {
        futures::executor::block_on(async { self.repository.set_quantity(id, quantity).await })
    }

    pub async fn get_quantity_by_id(&self, id: u32) -> Result<String, Error> {
        self.repository.get_quantity_by_id(id).await
    }

    pub fn get_quantity_by_id_sync(&self, id: u32) -> Result<String, Error> {
        futures::executor::block_on(async { self.repository.get_quantity_by_id(id).await })
    }

    pub async fn get_head_by_id(&self, id: u32) -> Result<String, Error> {
        self.repository.get_quantity_by_id(id).await
    }
}
