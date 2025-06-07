use crate::{
    application::providers::configuration::{
        activate::ActivateConfigurationProvider, get_active::GetActiveConfigurationProvider,
    },
    infrastructure::database::repositories::configuration::ConfigurationRepositoryImpl,
};
use sqlx::{Pool, Sqlite};
use std::sync::Arc;

pub struct ConfigurationProviders {
    pub activate: ActivateConfigurationProvider,
    pub get_active: GetActiveConfigurationProvider,
}

pub fn injection_providers_configuration(db: Arc<Pool<Sqlite>>) -> ConfigurationProviders {
    let configuration_repository = Arc::new(ConfigurationRepositoryImpl::new(db.clone()));

    let activate = ActivateConfigurationProvider {
        repository: configuration_repository.clone(),
    };

    let get_active = GetActiveConfigurationProvider {
        repository: configuration_repository.clone(),
    };

    ConfigurationProviders {
        activate,
        get_active,
    }
}
