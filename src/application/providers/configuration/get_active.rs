use crate::{
    application::schema::configuration::row::ConfigurationRow,
    infrastructure::database::repositories::configuration::repository_configuration_get_active,
};

pub async fn provider_configuration_get_active() -> ConfigurationRow {
    let (configuration, queried_views) = repository_configuration_get_active().await.unwrap();
    (configuration, queried_views)
}
