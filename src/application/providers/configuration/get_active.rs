use crate::{
    domain::entities::configuration::Configuration,
    infrastructure::database::repositories::configuration::repository_configuration_get_active,
};

pub async fn provider_configuration_get_active() -> Configuration {
    repository_configuration_get_active().await.unwrap()
}
