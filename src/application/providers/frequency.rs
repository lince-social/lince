use crate::domain::{entities::frequency::Frequency, repositories::frequency::FrequencyRepository};
use std::io::Error;

pub struct FrequencyProvider {
    pub repository: std::sync::Arc<dyn FrequencyRepository>,
}

impl FrequencyProvider {
    pub async fn get(&self, id: u32) -> Result<Option<Frequency>, Error> {
        self.repository.get(id).await
    }

    pub async fn update(&self, frequency: Frequency) -> Result<(), Error> {
        self.repository.update(frequency).await
    }
}
