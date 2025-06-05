use crate::domain::repositories::configuration::ConfigurationRepository;
use std::{io::Error, sync::Arc};

pub struct ActivateConfigurationProvider {
    pub repository: Arc<dyn ConfigurationRepository>,
}

impl ActivateConfigurationProvider {
    pub async fn execute(&self, id: &str) -> Result<(), Error> {
        self.repository.set_active(id).await
    }
}
