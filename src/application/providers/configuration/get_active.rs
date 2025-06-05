use crate::domain::{
    entities::configuration::Configuration, repositories::configuration::ConfigurationRepository,
};
use std::{io::Error, sync::Arc};

pub struct GetActiveConfigurationProvider {
    pub repository: Arc<dyn ConfigurationRepository>,
}

impl GetActiveConfigurationProvider {
    pub async fn execute(&self) -> Result<Configuration, Error> {
        self.repository.get_active().await
    }
}
