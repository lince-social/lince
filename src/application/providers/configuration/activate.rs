use crate::{
    domain::entities::collection::Configuration,
    infrastructure::database::repositories::configuration::{
        repository_configuration_get_active, repository_configuration_set_active,
    },
};
use std::io::Error;

pub async fn provider_activate_configuration(id: &str) -> Result<(), Error> {
    repository_configuration_set_active(id).await
}

pub async fn provider_configuration_get_active() -> Result<Configuration, Error> {
    repository_configuration_get_active().await
}
