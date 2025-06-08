use crate::domain::entities::frequency::Frequency;
use async_trait::async_trait;
use std::io::Error;

#[async_trait]
pub trait FrequencyRepository: Send + Sync {
    async fn get(&self, id: u32) -> Result<Option<Frequency>, Error>;
    async fn update(&self, frequency: Frequency) -> Result<(), Error>;
} 