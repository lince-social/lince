use crate::domain::{
    entities::configuration::Configuration, repositories::configuration::ConfigurationRepository,
};
use std::{io::Error, sync::Arc};

pub struct ConfigurationProvider {
    pub repository: Arc<dyn ConfigurationRepository>,
}

impl ConfigurationProvider {
    pub async fn get_active(&self) -> Result<Configuration, Error> {
        self.repository.get_active().await
    }

    pub async fn activate(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
